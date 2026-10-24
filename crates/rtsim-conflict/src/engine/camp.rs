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
    ACTION_ADOPT, ACTION_HAND_BACK, ACTION_MASK, ACTION_REMOVE, ACTION_SEAT_HEIR, ACTION_SET_OUT,
    ACTION_SPAWN, ACTION_TAKE, BODY_CHIEF, BODY_ELSEWHERE, BODY_STRAY, CENSUS_CHIEF,
    CENSUS_CRYSTAL, CENSUS_THRONE, CHIEF_CRYSTAL, CampBodies, CampCensus, CampDeath, CampOut,
    CampReconcile, CensusBody, ChiefBody, ChiefState, DEATH_CHIEF, DEATH_STRAY, LINE_CHIEF,
    LINE_CRYSTAL, LINE_DONE, LINE_NOTHING, RECONCILE_THRONE, ReconcileLine,
};

/// Whole days after a body's first miss before the third miss writes its member off
/// (MOWZIE.md §4.9; PLACEHOLDER).
pub const WRITE_OFF_DAYS: u64 = 2;
/// Misses before a member is written off: a body found nowhere in three censuses.
pub const WRITE_OFF_MISSES: u8 = 3;
/// The chief's slot number (MZB-401): -1 as a Java int. It sorts after every warrior's slot, so
/// [`Camp::index`]'s search holds, and it is outside the pool: no count, draw, refill or audit sum
/// of the pool includes it.
pub const CHIEF_SLOT: u32 = u32::MAX;
/// The players a chief's trade memory keeps (PLACEHOLDER, MZB-420): the census's own cap.
pub const TRADED_CAP: usize = boundary::MAX_ENTRIES;
/// The most items one tribute transfer carries: a stack (MZB-446).
pub const STACK_ITEMS: i64 = 64;
/// How long after a pair that seated no one the heir is ordered again while the camp stays in:
/// one game hour (PLACEHOLDER, MZB-445). The pair's answer would otherwise order the next at once.
const HEIR_RETRY: u64 = TICKS_PER_DAY / 24;
/// The slot space of the tribute transfers' op ids (`rtsim_world::ribbits::identity_op`).
const TRIBUTE_SLOT: u64 = 0x4d5a_0009 << 32;

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
    /// The faction's chief when the slot was made: the notable whose body this is, if it is the
    /// chief's body. A chief body's death fells the chief only while the faction's chief is still
    /// that one; a chief the books have since replaced (dead of age, succeeded by whatever asked
    /// for the chief first) is no one's death to answer for, whatever the order of the records.
    pub chief: ActorId,
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

/// A chief to come (MZB-405, MZB-406): the camp has no chief slot, or its slot is dead away, and
/// its next chief is due at `due`. `found_empty` marks a camp found without its Umvuthi, whose wait
/// an untagged one seen in the footprint cancels (a chief killed at its camp is not cut short).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heir {
    pub due: Tick,
    pub found_empty: bool,
}

/// A tribute good the engine took out of its books and sent to a haven chest, not yet answered
/// (MZB-423).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placing {
    pub op: u64,
    pub good: u32,
    /// Milli-units, whole items (at most one stack).
    pub amount: i64,
    /// The chest it goes to, as HAVEN_CHEST named it (Java's dimension id): kept here so a transfer
    /// rewritten after a restart still knows where.
    pub dimension: u32,
    pub pos: (i32, i32, i32),
}

/// The haven chest the plugin chose for a camp (HAVEN_CHEST, MZB-422): unsaved, latest wins, and
/// forgotten at CAMP_OUT and at a session start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chest {
    pub dimension: u32,
    pub pos: (i32, i32, i32),
    /// Its empty slots.
    pub free: u32,
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
    /// The chief to come, when the camp has no chief (MZB-405, MZB-406).
    pub heir: Option<Heir>,
    /// The stamp of the booked `Ev::CampHeir`.
    pub heir_stamp: u32,
    /// The players that have paid the chief, as (high, low) halves of their UUIDs, sorted
    /// (MZB-420).
    pub traded: Vec<(u64, u64)>,
    /// Tribute booked and not yet in a chest: (good, milli-units), by good (MZB-423).
    pub owed_tribute: Vec<(u32, i64)>,
    /// Stacks sent to the chest so far: the next one's op id derives from it (MZB-423).
    pub stacks: u32,
    /// The chest the plugin chose, while the camp is in (unsaved).
    pub chest: Option<Chest>,
    /// The MASK line in flight: its reconcile's op and its index there (MZB-403). It names no
    /// slot, so no slot's line stands for it.
    pub mask: Option<(u64, u32)>,
    /// The SEAT_HEIR line in flight, the same way (the heir's slot is made when it is answered).
    pub seat: Option<(u64, u32)>,
}

impl Camp {
    pub fn is_in(&self) -> bool {
        self.epoch != 0
    }

    pub fn index(&self, id: u32, generation: u32) -> Option<usize> {
        let i = self.slots.binary_search_by_key(&id, |s| s.id).ok()?;
        (self.slots[i].generation == generation).then_some(i)
    }

    /// The chief's slot, if the camp has one (it sorts last).
    pub fn chief_index(&self) -> Option<usize> {
        self.slots
            .last()
            .filter(|s| s.id == CHIEF_SLOT)
            .map(|_| self.slots.len() - 1)
    }

    /// The chief's slot.
    pub fn chief(&self) -> Option<&Slot> {
        self.chief_index().map(|i| &self.slots[i])
    }

    /// The slots of the pool's members: every slot but the chief's (MZB-401).
    fn members(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(|s| s.id != CHIEF_SLOT)
    }

    /// Slots in the pool: at home, body or owed.
    pub fn home_count(&self) -> u32 {
        self.members().filter(|s| s.state.home()).count() as u32
    }

    /// Slots in war parties.
    pub fn away_count(&self) -> u32 {
        self.members().filter(|s| s.state.away()).count() as u32
    }

    /// Slots a draw may take: at home with no line in flight. None while the heir's pair is in
    /// flight: the warrior it masks must not be mustered (MZB-409).
    pub fn drawable(&self) -> u32 {
        if self.mask.is_some() || self.seat.is_some() {
            return 0;
        }
        self.members()
            .filter(|s| s.state.home() && s.line.is_none())
            .count() as u32
    }

    /// Lines in flight, the heir's pair included.
    pub fn in_flight(&self) -> u32 {
        self.slots.iter().filter(|s| s.line.is_some()).count() as u32
            + u32::from(self.mask.is_some())
            + u32::from(self.seat.is_some())
    }

    /// The generation the heir is seated with: derived from the camp and the tick the heir is due
    /// (which does not move while it waits), and never the generation the chief slot last held, so
    /// the census can tell the heir an answer to a lost reconcile made from a body of an old chief
    /// (MZB-409). None while no heir waits.
    pub fn heir_generation(&self, key: u64) -> Option<u32> {
        let h = self.heir?;
        let avoid = self
            .chief()
            .map(|s| s.generation)
            .or_else(|| self.retired_gen(CHIEF_SLOT));
        Some(derive_gen(key, CHIEF_SLOT, h.due, avoid))
    }

    /// The chief's slot, new, in state `state`.
    fn alloc_chief(
        &mut self,
        generation: u32,
        body: u32,
        state: SlotState,
        party: Handle<Party>,
        chief: ActorId,
    ) {
        debug_assert!(self.chief_index().is_none(), "a second chief slot");
        self.slots.push(Slot {
            id: CHIEF_SLOT,
            generation,
            body,
            state,
            party,
            misses: 0,
            missed_at: Tick::ZERO,
            line: None,
            chief,
        });
    }

    /// A player that paid the chief joins its memory (MZB-420): sorted, no one twice, at most
    /// [`TRADED_CAP`]. False if the memory is full and the player not in it.
    pub fn remember(&mut self, player: boundary::Uuid) -> bool {
        match self.traded.binary_search(&player) {
            Ok(_) => true,
            Err(_) if self.traded.len() >= TRADED_CAP => false,
            Err(at) => {
                self.traded.insert(at, player);
                true
            }
        }
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

    /// Voids every line in flight, the heir's pair included; how many there were.
    fn void(&mut self) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            n += usize::from(s.line.take().is_some());
        }
        n + usize::from(self.mask.take().is_some()) + usize::from(self.seat.take().is_some())
    }

    /// Bytes on the heap (the paging budget).
    pub fn heap_bytes(&self) -> usize {
        std::mem::size_of::<Camp>()
            + self.slots.capacity() * std::mem::size_of::<Slot>()
            + self.retired.capacity() * std::mem::size_of::<Retired>()
            + self.answered.capacity() * std::mem::size_of::<Answered>()
            + self.traded.capacity() * std::mem::size_of::<(u64, u64)>()
            + self.owed_tribute.capacity() * std::mem::size_of::<(u32, i64)>()
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

/// The chief's memory for a body at home (MZB-421): agent 0, the camp's epoch, health 0 (the body
/// keeps its own), no flags.
fn chief_body_record(key: u64, camp: &Camp) -> Cmd {
    Cmd::ChiefBody(ChiefBody {
        faction: key,
        agent: 0,
        epoch: camp.epoch,
        health: 0.0,
        flags: 0,
        traded: camp.traded.clone(),
    })
}

/// The op id of a camp's `n`th tribute transfer: the core's namespace, derived from the camp and
/// the count so a rewritten transfer is the same op (MZB-423).
fn tribute_op(key: u64, n: u32) -> u64 {
    rtsim_world::ribbits::identity_op(key, TRIBUTE_SLOT | u64::from(n))
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
    /// camp's chief, ledger 9; a chief body's slot is the chief, MZB-403) and whether its crystal
    /// is held.
    fn line_flags(&self, f: u32, slot: u32) -> u8 {
        let chief = slot == CHIEF_SLOT || self.chief_camp(f);
        let crystal = self.factions[f as usize].crystal;
        (if chief { LINE_CHIEF } else { 0 }) | (if crystal { LINE_CRYSTAL } else { 0 })
    }

    /// The engine's body id of the camp's chief body kind (MZB-401), if its culture keeps one.
    fn chief_body_id(&self, f: u32) -> Option<u32> {
        self.culture(f)
            .def
            .chief_body
            .as_deref()
            .map(|n| body_id(&self.cultures, n))
    }

    /// Whether the camp's pool member is its chief: its home unit has a row of its own without
    /// the crystal (`bare[u] != u`, ledger 9). Only such a camp has a crystal to hold or lose.
    fn chief_camp(&self, f: u32) -> bool {
        let u = self.home_unit(f);
        self.culture(f).bare[usize::from(u)] != u
    }

    /// The chief's memory as a record, for a camp that is in.
    fn chief_body_cmd(&self, f: u32) -> Option<Cmd> {
        let camp = self.camp(f).filter(|c| c.is_in())?;
        Some(chief_body_record(self.factions[f as usize].key, camp))
    }

    fn line(&self, f: u32, s: &Slot, action: u8, a: u64, b: u64) -> ReconcileLine {
        ReconcileLine {
            slot: s.id,
            generation: s.generation,
            body: s.body,
            action,
            flags: self.line_flags(f, s.id),
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
        // A chief body's camp starts with its chief owed, for the first census's untagged Umvuthi
        // to fill, or waits for an heir at the cap when found without one (MZB-405).
        if let Some(body) = self.chief_body_id(f)
            && self
                .camp(f)
                .is_some_and(|c| c.chief_index().is_none() && c.heir.is_none())
        {
            if u32::from(census) & CENSUS_CHIEF != 0 {
                self.camp_alloc_chief(f, at, body, SlotState::Owed, Handle::NULL);
            } else {
                self.found_empty_heir(f, at);
            }
        }
        self.plan_arrival(f, at);
    }

    /// A camp found without its chief waits for an heir as a camp farmed to the cap does
    /// (MZB-405; EQUILIBRIUM.md §2.3): the wait is the cap's, from now.
    fn found_empty_heir(&mut self, f: u32, at: Tick) {
        let Some(r) = self.culture(f).chief_renewal else {
            return;
        };
        let day = (at.0 / TICKS_PER_DAY) as u32;
        let pressure = Pressure::at_cap(&r.rule, day);
        let due = at + r.rule.delay_ticks(pressure, day).max(1);
        self.camp_mut(f).expect("a ledger").pressure = pressure;
        self.set_heir(f, due, true, at);
    }

    /// Books the chief to come, due at `due`, and the event that looks at it (MZB-406). A heir
    /// booked before is replaced.
    fn set_heir(&mut self, f: u32, due: Tick, found_empty: bool, at: Tick) {
        let camp = self.camp_mut(f).expect("a ledger");
        camp.heir = Some(Heir { due, found_empty });
        camp.heir_stamp = camp.heir_stamp.wrapping_add(1);
        let stamp = camp.heir_stamp;
        self.schedule(due.max(at), Ev::CampHeir { faction: f, stamp });
    }

    /// The chief's slot, new, in the camp's ledger: its generation from the tick `t`.
    fn camp_alloc_chief(
        &mut self,
        f: u32,
        t: Tick,
        body: u32,
        state: SlotState,
        party: Handle<Party>,
    ) {
        let key = self.factions[f as usize].key;
        let chief = self.factions[f as usize].chief;
        let camp = self.camp_mut(f).expect("a ledger");
        let generation = derive_gen(key, CHIEF_SLOT, t, camp.retired_gen(CHIEF_SLOT));
        camp.alloc_chief(generation, body, state, party, chief);
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
        // A chief body's party away with its chief has the chief's slot left (MZB-401); the first
        // census says whether its body is still in the camp.
        if let Some(cb) = self.chief_body_id(f) {
            let led = self
                .parties
                .iter()
                .find(|(_, x)| x.faction == f && x.kind.is_war() && x.chief)
                .map(|(h, _)| h);
            if let Some(p) = led {
                self.camp_alloc_chief(f, at, cb, SlotState::Left, p);
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
        let chief = self.factions[f as usize].chief;
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
                chief,
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
        self.unblock_lines(f, at);
        // A grove that stays in gets its heir as soon as the wait is over and a warrior is home
        // (MOWZIE.md 4.10): a due tick with no warrior is looked at again at each change of the
        // camp's lines. It returns at once unless the heir is due, the camp is in and no pair is
        // in flight.
        self.camp_heir(f, at);
    }

    /// `unblock` without a look at the heir.
    fn unblock_lines(&mut self, f: u32, at: Tick) {
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
        let c = self.culture(f);
        let boss = c.renewal.or(c.chief_renewal).is_some_and(|r| r.boss);
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
        // A new chief holds its crystal; a camp whose member has no such row has none to give
        // (MZB-256).
        if self.chief_camp(f) {
            self.factions[f as usize].crystal = true;
        }
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
            .filter(|s| s.id != CHIEF_SLOT && s.state == SlotState::Owed && s.line.is_none())
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
        // Every order to a grove carries its throne once a census gave one (MZB-404).
        let (flags, throne, direction) = match camp.throne {
            Some(t) => (RECONCILE_THRONE, t.spot, t.direction),
            None => (0, (0.0, 0.0, 0.0), 0.0),
        };
        // At most one line in flight per slot (I4): a second for a slot is a bug, and not sent. The
        // heir's pair names no slot that stands for it, so each has a marker of its own.
        let lines: Vec<ReconcileLine> = lines
            .into_iter()
            .filter(|l| {
                let busy = match l.action {
                    ACTION_MASK => camp.mask.is_some(),
                    ACTION_SEAT_HEIR => camp.seat.is_some(),
                    _ => {
                        l.generation != 0
                            && camp
                                .index(l.slot, l.generation)
                                .is_some_and(|i| camp.slots[i].line.is_some())
                    }
                };
                debug_assert!(!busy, "a second line for slot {} in flight", l.slot);
                !busy
            })
            .collect();
        if lines.is_empty() {
            return;
        }
        let mut out = Vec::new();
        let mut orders = 0;
        for chunk in lines.chunks(boundary::MAX_ENTRIES) {
            let op = derive_op(key, epoch, camp.seq);
            camp.seq += 1;
            for (k, l) in chunk.iter().enumerate() {
                match l.action {
                    ACTION_MASK => camp.mask = Some((op, k as u32)),
                    ACTION_SEAT_HEIR => camp.seat = Some((op, k as u32)),
                    _ => {}
                }
                if l.generation != 0
                    && let Some(i) = camp.index(l.slot, l.generation)
                {
                    camp.slots[i].line = Some(Line {
                        action: l.action,
                        op,
                    });
                }
            }
            // The chief's memory goes ahead of every chief spawn, so the body is made knowing whom
            // it has traded with (MZB-421).
            if chunk
                .iter()
                .any(|l| l.slot == CHIEF_SLOT && l.action == ACTION_SPAWN)
            {
                out.push(chief_body_record(key, camp));
            }
            out.push(Cmd::Reconcile(CampReconcile {
                faction: key,
                epoch,
                op,
                flags,
                throne,
                direction,
                lines: chunk.to_vec(),
            }));
            orders += 1;
        }
        self.stats.camp_orders += orders;
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
            .filter(|&i| {
                camp.slots[i].id != CHIEF_SLOT
                    && camp.slots[i].state.home()
                    && camp.slots[i].line.is_none()
            })
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

    /// Whether the camp's chief can lead a party now (MZB-413): its slot exists, at home (body or
    /// owed), with no line in flight, and no heir waits (a chief that has just been seated is the
    /// heir's, and the pair is still being booked while one waits).
    pub(super) fn chief_can_lead(&self, f: u32) -> bool {
        self.camp(f).is_some_and(|c| {
            c.heir.is_none()
                && c.chief()
                    .is_some_and(|s| s.state.home() && s.line.is_none())
        })
    }

    /// A party that leads with the chief takes it (MZB-413): at home with its body the slot is
    /// left, owed it is gone.
    pub(super) fn camp_draw_chief(&mut self, f: u32, p: Handle<Party>) {
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        let Some(i) = camp.chief_index() else {
            return;
        };
        let s = &mut camp.slots[i];
        debug_assert!(s.state.home() && s.line.is_none(), "a chief drawn unfit");
        s.state = if s.state == SlotState::Body {
            SlotState::Left
        } else {
            SlotState::Gone
        };
        s.party = p;
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
                    if s.state == want && s.party == p && s.id != CHIEF_SLOT {
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
            // A chief body has no puppet before M11: it comes home as a spawn (MZB-415).
            let l = if s.id != CHIEF_SLOT && puppets > 0 {
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

    /// A camp's chief at home reported its crystal (CHIEF_STATE, latest-wins, MZB-268): the steal
    /// sends it with the flag off, and the faction's crystal follows, so the next body asked for
    /// (an arrival, a body handed back) is asked for without it. A change only is noted. The
    /// health is not kept: nothing of M6 reads it (the census carries it). A record for a faction
    /// this engine does not know, or one whose camp holds no bodies, is ignored.
    pub(super) fn chief_state(&mut self, c: &ChiefState, at: Tick) {
        let Some(&f) = self.by_key.get(&c.faction) else {
            return;
        };
        if !self.culture(f).def.camp_bodies {
            return;
        }
        let crystal = c.flags & CHIEF_CRYSTAL != 0;
        if self.factions[f as usize].crystal == crystal {
            return;
        }
        self.factions[f as usize].crystal = crystal;
        self.note(
            at,
            Some(f),
            "crystal",
            format!(
                "the chief's crystal is {}",
                if crystal { "back" } else { "gone" }
            ),
        );
    }

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
        // The flags the lines are made with; the crystal in them is mended below, once the bodies
        // the books keep are known (MZB-269, 299). The chief's slot has flags of its own.
        let (member_flags, chief_flags) = (self.line_flags(f, 0), self.line_flags(f, CHIEF_SLOT));
        let line = |s: &Slot, action: u8, a: u64, b: u64| ReconcileLine {
            slot: s.id,
            generation: s.generation,
            body: s.body,
            action,
            flags: if s.id == CHIEF_SLOT {
                chief_flags
            } else {
                member_flags
            },
            a,
            b,
        };
        // A camp whose chief is a body of its own (MZB-401).
        let chief_kind = self.chief_body_id(f);
        let (fkey, chief_actor) = (
            self.factions[f as usize].key,
            self.factions[f as usize].chief,
        );
        let mut chief_written_off = false;
        let mut empty_heir = false;
        // A camp made by this census (no FACTION_FOUND before it) has to learn whether it has a
        // chief: an untagged Umvuthi in the footprint is its chief owed, else it waits for an heir.
        if let Some(cb) = chief_kind
            && camp.chief_index().is_none()
            && camp.heir.is_none()
        {
            let stray = c.bodies.iter().any(|b| {
                b.flags & BODY_CHIEF != 0 && (b.flags & BODY_STRAY != 0 || b.generation == 0)
            });
            if stray {
                let g = derive_gen(fkey, CHIEF_SLOT, at, camp.retired_gen(CHIEF_SLOT));
                camp.alloc_chief(g, cb, SlotState::Owed, Handle::NULL, chief_actor);
            } else {
                empty_heir = true;
            }
        }
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
        // Whether a chief body the books keep is among the bodies (MZB-269).
        let mut kept_chief = false;
        for (id, generation) in ids {
            let Some(i) = camp.index(id, generation) else {
                continue;
            };
            let found = held.contains_key(&(id, generation));
            let chief_body = held
                .get(&(id, generation))
                .is_some_and(|b| b.flags & BODY_CHIEF != 0);
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
                            if id == CHIEF_SLOT {
                                // The chief died unseen: a death at the grove by no one, the
                                // generation retired at a death so a body found later is removed,
                                // never a second chief (MZB-411). The pool is not its.
                                camp.free(i, true);
                                chief_written_off = true;
                                self.stats.camp_written_off += 1;
                                continue;
                            }
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
            // A found body of a slot that is a member of the camp now (a Dead slot's is removed,
            // and a body the books do not keep says nothing about the chief the books do).
            if chief_body
                && camp.index(id, generation).is_some_and(|i| {
                    matches!(camp.slots[i].state, SlotState::Body | SlotState::Left)
                })
            {
                kept_chief = true;
            }
        }
        // Tagged bodies no slot holds, and strays: a body whose generation the books retired at
        // a death is removed; the rest fill slots owed, then new slots while the pool and the
        // members away are under the cap, and are removed above it (ledger 6). A chief body of a
        // culture that keeps one is the chief's alone (MZB-402), never a warrior's.
        let mut rest: Vec<&CensusBody> = Vec::new();
        let mut chief_rest: Vec<&CensusBody> = Vec::new();
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
            if chief_kind.is_some() && b.flags & BODY_CHIEF != 0 {
                chief_rest.push(b);
            } else {
                rest.push(b);
            }
        }
        // The chief's slot, owed and with no body yet: the one a chief body may be adopted into.
        let mut chief_owed: Option<(u32, u32)> = camp
            .chief()
            .filter(|s| s.state == SlotState::Owed)
            .map(|s| (s.id, s.generation));
        let mut dropped_wait = false;
        for b in chief_rest {
            let stray = b.flags & BODY_STRAY != 0 || b.generation == 0;
            // The heir of a pair whose answer was lost or came late: the generation it was ordered
            // with is the one the books derive, so the body is the chief the books were waiting
            // for (MZB-409).
            if !stray
                && b.slot == CHIEF_SLOT
                && camp.heir_generation(fkey) == Some(b.generation)
                && chief_owed.is_none()
            {
                if let Some(i) = camp.chief_index() {
                    if camp.slots[i].state != SlotState::Dead {
                        lines.push(by_uuid(b, ACTION_REMOVE));
                        continue;
                    }
                    camp.free(i, true);
                }
                camp.alloc_chief(
                    b.generation,
                    if b.body != 0 {
                        b.body
                    } else {
                        chief_kind.unwrap_or(body)
                    },
                    SlotState::Body,
                    Handle::NULL,
                    chief_actor,
                );
                camp.heir = None;
                camp.heir_stamp = camp.heir_stamp.wrapping_add(1);
                camp.traded.clear();
                matched.insert((CHIEF_SLOT, b.generation));
                kept_chief |= b.flags & BODY_CHIEF != 0;
                continue;
            }
            // The chief owed a body adopts the first one seen, tagged or not (MZB-402).
            if let Some((id, generation)) = chief_owed.take() {
                let i = camp.index(id, generation).expect("owed");
                if b.body != 0 {
                    camp.slots[i].body = b.body;
                }
                kept_chief |= b.flags & BODY_CHIEF != 0;
                lines.push(line(&camp.slots[i], ACTION_ADOPT, b.uuid.0, b.uuid.1));
                continue;
            }
            // A camp found without its Umvuthi adopts an untagged one it sees before its heir
            // comes, and the wait is cancelled; after a chief's death a stray does not cut the
            // wait, and any other untagged Umvuthi is the jar's own (MZB-402, MZB-405).
            if stray
                && camp.chief_index().is_none()
                && camp.heir.is_some_and(|h| h.found_empty)
                && !dropped_wait
            {
                dropped_wait = true;
                camp.heir = None;
                camp.heir_stamp = camp.heir_stamp.wrapping_add(1);
                // As if the camp had never been found empty: the cap's pressure was only a stand-in.
                camp.pressure = Pressure::default();
                let g = derive_gen(fkey, CHIEF_SLOT, at, camp.retired_gen(CHIEF_SLOT));
                camp.alloc_chief(
                    g,
                    if b.body != 0 {
                        b.body
                    } else {
                        chief_kind.unwrap_or(body)
                    },
                    SlotState::Owed,
                    Handle::NULL,
                    chief_actor,
                );
                let i = camp.chief_index().expect("just made");
                kept_chief |= b.flags & BODY_CHIEF != 0;
                lines.push(line(&camp.slots[i], ACTION_ADOPT, b.uuid.0, b.uuid.1));
                continue;
            }
            // A tagged chief body the books do not keep is a second chief: removed. An untagged
            // one is left alone.
            if !stray {
                lines.push(by_uuid(b, ACTION_REMOVE));
            }
        }
        let mut owed: std::collections::VecDeque<(u32, u32)> = camp
            .slots
            .iter()
            .filter(|s| s.id != CHIEF_SLOT && s.state == SlotState::Owed)
            .map(|s| (s.id, s.generation))
            .collect();
        for b in rest {
            if let Some((id, generation)) = owed.pop_front() {
                let i = camp.index(id, generation).expect("owed");
                if b.body != 0 {
                    camp.slots[i].body = b.body;
                }
                kept_chief |= b.flags & BODY_CHIEF != 0;
                lines.push(line(&camp.slots[i], ACTION_ADOPT, b.uuid.0, b.uuid.1));
            } else if camp.home_count() + camp.away_count() < cap {
                kept_chief |= b.flags & BODY_CHIEF != 0;
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
        // Slots still owed: a spawn line each. The chief's goes after its memory (MZB-421).
        for (id, generation) in owed.into_iter().chain(chief_owed) {
            let i = camp.index(id, generation).expect("owed");
            lines.push(line(&camp.slots[i], ACTION_SPAWN, 0, 0));
        }
        // The chief's memory: the census's list is the live chief's own when it names its slot and
        // generation, and the two are unioned, so a payment the engine has and the body has not yet
        // written stays (MZB-420). A body that lacks some of the engine's is written to (MZB-421).
        let mut chief_short = false;
        if chief_kind.is_some()
            && c.traded_slot == CHIEF_SLOT
            && camp
                .chief()
                .is_some_and(|s| s.state == SlotState::Body && s.generation == c.traded_generation)
        {
            for u in &c.traded {
                camp.remember(*u);
            }
            chief_short = camp.traded.iter().any(|t| !c.traded.contains(t));
        }
        self.factions[f as usize].camp = Some(camp);
        // The chief the census found says whether its crystal is on it, when it is a chief the
        // books keep: this mends a CHIEF_STATE lost in a crash, which is latest-wins and not
        // journaled (MZB-269). The census names no body for the crystal byte, and the plugin's
        // chief may be the leftover body of a slot that died away (a robbed Frostmaw killed on a
        // raid, its body left in the den): that one is to be removed and says nothing, or the
        // arrival's crystal would be read off a body about to go (MZB-269). The lines made so far
        // are asked again with the crystal as it now stands. A grove's chief holds no crystal.
        if c.flags & CENSUS_CHIEF != 0 && kept_chief && self.chief_camp(f) {
            self.factions[f as usize].crystal = c.flags & CENSUS_CRYSTAL != 0;
            for l in lines.iter_mut().filter(|l| l.generation != 0) {
                l.flags = self.line_flags(f, l.slot);
            }
        }
        // A body missing from the camp is a player's loss (EQUILIBRIUM item 11; rtsim cannot tell
        // an absence, MZB-260), booked once the camp is put back and before the wait is planned.
        self.renewal_bump(f, written_off, LossCause::Player, at);
        // The chief nobody saw die, and the camp found without its Umvuthi (MZB-411, MZB-405).
        if chief_written_off {
            self.chief_lost(f, LossCause::Player, at);
        } else if empty_heir {
            self.found_empty_heir(f, at);
        }
        if chief_short {
            let cmd = self.chief_body_cmd(f);
            self.outbox.extend(cmd);
        }
        self.send_lines(f, lines, at);
        self.unblock(f, at);
        // A chief to come is looked at again at each census (MZB-407).
        self.camp_heir(f, at);
    }

    // ---- tribute, trade and the chest (MZB-416 to MZB-423) -----------------------------------

    /// A masked trade with a grove's minion (TRADED, MZB-417): the gold nuggets on either side are
    /// worth `nuggets / tribute_per_coin` coins, which soften the trader's feud at a tenth of a
    /// gift's rate ([`Conflict::traded`]).
    pub(super) fn trade_booked(&mut self, w: &mut World, t: &boundary::Traded, at: Tick) {
        let Some(&f) = self.by_key.get(&t.faction) else {
            return;
        };
        let good = self.tribute_good(w, f);
        let per = self.tribute_per_coin(w, f, good);
        if per <= 0.0 {
            return;
        }
        let trader = self.actor_of_uuid(w, t.player);
        self.traded(w, t.faction, trader, f64::from(t.nuggets) / per, at);
    }

    /// A player's first payment to an Umvuthi body (CHIEF_PAID, MZB-418): booked as tribute through
    /// [`Conflict::pay_tribute`], and the payer joins the trade memory only when the slot and
    /// generation are the live chief's (Body). A stray Umvuthi's payment (`PAID_STRAY`) is tribute
    /// with no memory.
    pub(super) fn chief_paid(&mut self, w: &mut World, c: &boundary::ChiefPaid, at: Tick) {
        let Some(&f) = self.by_key.get(&c.faction) else {
            return;
        };
        if c.flags & boundary::PAID_STRAY == 0
            && c.slot == CHIEF_SLOT
            && let Some(camp) = self.camp_mut(f)
            && camp
                .index(c.slot, c.generation)
                .is_some_and(|i| camp.slots[i].state == SlotState::Body)
        {
            camp.remember(c.player);
        }
        let goods = self.goods_of(w, &c.items);
        if goods.is_empty() {
            return;
        }
        let payer = self.actor_of_uuid(w, c.player);
        self.pay_tribute(w, c.faction, payer, None, &goods, at);
    }

    /// Tribute or a gift the grove took in is owed to its chest (MZB-423): only its tribute good,
    /// since another item's stack may exceed its limit and the transfer fails above it (MZB-446).
    /// Then the next stack is sent if the chest is known.
    pub(super) fn owe_tribute(
        &mut self,
        w: &mut World,
        f: u32,
        stocked: &BTreeMap<GoodId, Amount>,
        at: Tick,
    ) {
        if self.culture(f).def.chief_body.is_none() {
            return;
        }
        let good = self.tribute_good(w, f);
        let Some(&a) = stocked.get(&good).filter(|a| **a > 0) else {
            return;
        };
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        let g = u32::from(good.0);
        match camp.owed_tribute.binary_search_by_key(&g, |e| e.0) {
            Ok(i) => camp.owed_tribute[i].1 += a,
            Err(i) => camp.owed_tribute.insert(i, (g, a)),
        }
        self.place_tribute(w, f, at);
    }

    /// HAVEN_CHEST (MZB-422): the chest the plugin chose for a grove, latest wins. Unsaved.
    pub(super) fn haven_chest(&mut self, w: &mut World, h: &boundary::HavenChest, at: Tick) {
        let Some(&f) = self.by_key.get(&h.faction) else {
            return;
        };
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        camp.chest = Some(Chest {
            dimension: h.dimension,
            pos: h.pos,
            free: h.free,
        });
        self.place_tribute(w, f, at);
    }

    /// Sends the next stack of the tribute a grove owes its chest (MZB-423): while the camp is in,
    /// its chest is known with room, and nothing is being placed, one good leaves the books, the
    /// least of what is owed, what the faction's stock holds and one stack. It is a TRANSFER_TO_
    /// CONTAINER the link writes through its outbox ([`Conflict::placings`]), booked as leaving to
    /// the players until it is answered.
    pub(super) fn place_tribute(&mut self, w: &mut World, f: u32, at: Tick) {
        let node = self.factions[f as usize].node;
        let key = self.factions[f as usize].key;
        if self.placing.iter().any(|(g, _)| *g == f) {
            return;
        }
        let Some(camp) = self.camp(f).filter(|c| c.is_in()) else {
            return;
        };
        let Some(chest) = camp.chest.filter(|c| c.free > 0) else {
            return;
        };
        let m = w.market(node);
        let pick = camp.owed_tribute.iter().find_map(|&(g, owed)| {
            let good = GoodId(u16::try_from(g).ok()?);
            let held = m.lines.get(good.index())?.stock;
            let n = owed.min(held).min(STACK_ITEMS * MILLI) / MILLI * MILLI;
            (n >= MILLI).then_some((good, n))
        });
        let Some((good, n)) = pick else {
            return;
        };
        let taken = w.stock_to_players(node, &BTreeMap::from([(good, n)]), at);
        if taken.get(&good).copied() != Some(n) {
            return;
        }
        let Some(camp) = self.camp_mut(f) else {
            return;
        };
        let op = tribute_op(key, camp.stacks);
        camp.stacks += 1;
        // A stack may fill a slot: the chest has one less until the plugin says otherwise.
        camp.chest = Some(Chest {
            free: chest.free - 1,
            ..chest
        });
        self.placing.push((
            f,
            Placing {
                op,
                good: u32::from(good.0),
                amount: n,
                dimension: chest.dimension,
                pos: chest.pos,
            },
        ));
    }

    /// The tribute stacks sent to chests and not yet answered, by op id (MZB-423).
    pub fn placings(&self) -> impl Iterator<Item = (u64, Placing)> + '_ {
        self.placing.iter().map(|(_, p)| (p.op, *p))
    }

    /// Takes the stack `op` out of those in flight: its faction and what it was.
    fn unplace(&mut self, op: u64) -> Option<(u32, Placing)> {
        let i = self.placing.iter().position(|(_, p)| p.op == op)?;
        Some(self.placing.remove(i))
    }

    /// The plugin put a stack in its chest (APPLIED or DUPLICATE_OP): the amount is no longer owed,
    /// and the next stack goes. False for an op that is not a tribute stack.
    pub fn tribute_placed(&mut self, w: &mut World, op: u64, at: Tick) -> bool {
        let Some((f, p)) = self.unplace(op) else {
            return false;
        };
        if let Some(camp) = self.camp_mut(f)
            && let Ok(i) = camp.owed_tribute.binary_search_by_key(&p.good, |e| e.0)
        {
            camp.owed_tribute[i].1 -= p.amount;
            if camp.owed_tribute[i].1 <= 0 {
                camp.owed_tribute.remove(i);
            }
        }
        self.stats.tribute_stacks += 1;
        self.place_tribute(w, f, at);
        true
    }

    /// The plugin could not put a stack in its chest (FAILED): the goods go back to the faction's
    /// stock, the tribute is still owed, and the chest is forgotten until the plugin names one
    /// again.
    pub fn tribute_failed(&mut self, w: &mut World, op: u64, at: Tick) -> bool {
        let Some((f, p)) = self.unplace(op) else {
            return false;
        };
        let node = self.factions[f as usize].node;
        if let Some(camp) = self.camp_mut(f) {
            camp.chest = None;
        }
        let good = GoodId(u16::try_from(p.good).unwrap_or(0));
        w.stock_from_players(node, &BTreeMap::from([(good, p.amount)]), at);
        true
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
        camp.chest = None;
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
            camp.chest = None;
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
        let mut heir_changed = false;
        let mut pair_cleared = false;
        for a in &b.lines {
            // The heir's pair names no slot the engine keeps a line on (MZB-403, MZB-409).
            if a.action == ACTION_MASK || a.action == ACTION_SEAT_HEIR {
                if !camp.answer(b.op, a.line, self.obs_seq) {
                    continue;
                }
                let marker = if a.action == ACTION_MASK {
                    &mut camp.mask
                } else {
                    &mut camp.seat
                };
                if marker.is_some_and(|m| m == (b.op, a.line)) {
                    *marker = None;
                    cleared += 1;
                    pair_cleared = true;
                }
                if a.outcome != LINE_DONE && a.outcome != LINE_NOTHING {
                    continue;
                }
                self.stats.camp_answers += 1;
                if a.action == ACTION_MASK {
                    self.masked(&mut camp, f, a, at);
                } else if self.seated(&mut camp, f, a) {
                    heir_changed = true;
                }
                continue;
            }
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
            let flags = self.line_flags(f, a.slot);
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
        if cleared > 0 || heir_changed {
            // A pair that seated no one is ordered again an hour on, not by this very answer.
            let retry = pair_cleared && !heir_changed;
            match self.camp(f).map(|c| (c.heir.is_some(), c.heir_stamp)) {
                Some((true, stamp)) if retry => {
                    self.unblock_lines(f, at);
                    self.schedule(at + HEIR_RETRY, Ev::CampHeir { faction: f, stamp });
                }
                _ => self.unblock(f, at),
            }
        }
    }

    /// MASK answered done: the warrior the plugin took is the chief to be, so its member leaves
    /// the pool (MZB-409). The slot is freed all the same when a line of its own is in flight; that
    /// line's late answer finds a retired generation. A warrior the books have away lost its body
    /// as a take does, and keeps its place in the party.
    fn masked(&mut self, camp: &mut Camp, f: u32, a: &boundary::LineAnswer, at: Tick) {
        if a.generation == 0 || a.slot == CHIEF_SLOT {
            return;
        }
        let Some(i) = camp.index(a.slot, a.generation) else {
            return;
        };
        match camp.slots[i].state {
            SlotState::Body | SlotState::Owed => {
                camp.free(i, false);
                self.pool_shift(f, -1.0, at);
            }
            SlotState::Left => camp.slots[i].state = SlotState::Gone,
            SlotState::Gone | SlotState::Dead => {}
        }
    }

    /// SEAT_HEIR answered done or with nothing to do: the heir stands, tagged, so the chief's slot
    /// is its body, and the wait and the memory of the chief before it are over (MZB-409). A slot
    /// the books hold live is another chief's: it keeps its place and the census removes the
    /// extra body. True if the books changed.
    fn seated(&mut self, camp: &mut Camp, f: u32, a: &boundary::LineAnswer) -> bool {
        if a.slot != CHIEF_SLOT || a.generation == 0 {
            return false;
        }
        // Only the heir that waits is seated (review P3-1): a copy of an answer for an older chief,
        // one that died away or one the books replaced, is booked by state and changes nothing,
        // and neither does a heir whose death the books have (its generation is not the next
        // heir's, which derives from a later due tick).
        let key = self.factions[f as usize].key;
        if camp.heir_generation(key) != Some(a.generation) {
            return false;
        }
        match camp.chief_index() {
            Some(i) if camp.slots[i].state != SlotState::Dead => {
                if camp.slots[i].generation == a.generation {
                    return false;
                }
            }
            Some(i) => camp.free(i, true),
            None => {}
        }
        if camp.chief_index().is_none() {
            let body = self.chief_body_id(f).unwrap_or(0);
            let chief = self.factions[f as usize].chief;
            camp.alloc_chief(a.generation, body, SlotState::Body, Handle::NULL, chief);
        }
        camp.heir = None;
        camp.heir_stamp = camp.heir_stamp.wrapping_add(1);
        camp.traded.clear();
        true
    }

    // ---- CAMP_DEATH ------------------------------------------------------------------------

    /// The heir's body died before the answer to its SEAT_HEIR reached the books (the answer lost
    /// and held for the next camp-in, or a death first): the books seat it now, so its death is
    /// booked as any chief's, and the answer that comes late finds a generation retired at a death
    /// (`seated`). Otherwise the death would book nothing and the answer would seat a chief with
    /// no body, to be written off after three misses with its fall never booked.
    fn seat_before_death(&mut self, f: u32, generation: u32) {
        let key = self.factions[f as usize].key;
        let Some(mut camp) = self.factions[f as usize].camp.take() else {
            return;
        };
        // A body tagged with the waiting heir's generation exists only if the world did its
        // SEAT_HEIR, whether or not the line is still in flight here: a CAMP_OUT or a session
        // start voids the markers, and a holder's record is journaled again at the next camp-in.
        if generation != 0
            && camp.index(CHIEF_SLOT, generation).is_none()
            && camp.heir_generation(key) == Some(generation)
        {
            let a = boundary::LineAnswer {
                line: 0,
                slot: CHIEF_SLOT,
                generation,
                action: ACTION_SEAT_HEIR,
                outcome: LINE_DONE,
            };
            self.seated(&mut camp, f, &a);
        }
        self.factions[f as usize].camp = Some(camp);
    }

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
        if d.slot == CHIEF_SLOT {
            self.seat_before_death(f, d.generation);
        }
        let Some(i) = self.camp(f).and_then(|c| c.index(d.slot, d.generation)) else {
            return;
        };
        self.stats.camp_deaths += 1;
        let (state, party, body_of) = {
            let s = &self.camp(f).expect("a ledger").slots[i];
            (s.state, s.party, s.chief)
        };
        // The chief's own slot has its own rules (MZB-406, MZB-412): outside the pool, its loss
        // steps the chief's pressure and sets the heir.
        if d.slot == CHIEF_SLOT && self.chief_body_id(f).is_some() {
            self.chief_death(w, f, i, d, at);
            self.unblock(f, at);
            return;
        }
        // A member that died away was lost then: its body's death at the camp is no second loss.
        // The bump goes first: the `unblock` that follows plans the wait from it.
        let cause = self.renewal_cause(f, d.by);
        if state != SlotState::Dead {
            self.renewal_bump(f, 1, cause, at);
        }
        // The chief's body falls the chief at its camp (MZB-265); a Dead slot's is no second fall.
        // Who killed it is a player for a player's kill or a body taken, else no one.
        let chief = d.flags & DEATH_CHIEF != 0 && state != SlotState::Dead;
        let named = matches!(d.by, boundary::BY_PLAYER | boundary::BY_TAKEN) && d.killer != (0, 0);
        let killer = (chief && named).then(|| self.player(w, d.killer));
        match state {
            SlotState::Body | SlotState::Owed => {
                self.camp_mut(f).expect("a ledger").free(i, true);
                self.pool_shift(f, -1.0, at);
                if chief {
                    self.chief_killed_at_camp(w, f, body_of, killer, at);
                }
            }
            SlotState::Dead => self.camp_mut(f).expect("a ledger").free(i, true),
            SlotState::Left | SlotState::Gone => {
                self.camp_mut(f).expect("a ledger").free(i, true);
                self.pool_shift(f, 0.0, at);
                // Before the member's loss to its party, so a band that loses its chief unit
                // routs as a chief's band.
                if chief {
                    self.chief_killed_at_camp(w, f, body_of, killer, at);
                }
                self.member_died(w, party, d.body, at);
            }
        }
        self.unblock(f, at);
    }

    /// The body of a chief slot died or was carried off (CAMP_DEATH on `CHIEF_SLOT`, MZB-406): at
    /// home, or left with its take still held, it is the war chief's fall at its camp; its slot is
    /// freed with its generation retired at a death, the memory is cleared and an heir is due. A
    /// dead slot's body is no second fall. The chief is no unit of its party, so a party that
    /// loses it loses no warrior, and one that led with it routs as a chief's band does.
    fn chief_death(&mut self, w: &mut World, f: u32, i: usize, d: &CampDeath, at: Tick) {
        let s = &self.camp(f).expect("a ledger").slots[i];
        let (state, body_of) = (s.state, s.chief);
        let named = matches!(d.by, boundary::BY_PLAYER | boundary::BY_TAKEN) && d.killer != (0, 0);
        self.camp_mut(f).expect("a ledger").free(i, true);
        if state == SlotState::Dead {
            return;
        }
        let killer = named.then(|| self.player(w, d.killer));
        self.chief_killed_at_camp(w, f, body_of, killer, at);
        self.chief_lost(f, self.renewal_cause(f, d.by), at);
    }

    /// The chief's slot is lost to the books, by `cause` (a player's, which steps the camp's
    /// pressure by one, or no one's or a predator's, which does not): its memory is cleared, and
    /// the heir is due after the wait the pressure sets, never before the camp has recovered from
    /// its cowing (MZB-406, MZB-407). The caller has freed the slot.
    fn chief_lost(&mut self, f: u32, cause: LossCause, at: Tick) {
        let Some(r) = self.culture(f).chief_renewal else {
            return;
        };
        let cowed = self.factions[f as usize].cowed_until;
        let day = (at.0 / TICKS_PER_DAY) as u32;
        let camp = self.camp_mut(f).expect("a ledger");
        if cause == LossCause::Player {
            camp.pressure.bump(day, 1.0, r.rule.half_life_days);
        }
        camp.traded = Vec::new();
        let due = cowed.max(at + r.rule.delay_ticks(camp.pressure, day).max(1));
        if cause == LossCause::Player {
            self.stats.renewal_losses += 1;
        }
        self.set_heir(f, due, false, at);
        self.note(
            at,
            Some(f),
            "heir",
            format!(
                "the chief is lost; its heir is due in {:.1} days",
                (due.0 - at.0) as f64 / TICKS_PER_DAY as f64
            ),
        );
    }

    /// The chief of a party died away (MZB-412; the engine half, which only a test reaches before
    /// M11 sends a chief's casualty): a left slot becomes dead, its body still in the camp, to be
    /// removed or seated over; a gone slot is freed. Both retire the generation at a death and set
    /// the heir. False if the camp has no chief slot away.
    pub fn chief_died_away(&mut self, f: u32, cause: LossCause, at: Tick) -> bool {
        let Some(i) = self
            .camp(f)
            .and_then(Camp::chief_index)
            .filter(|&i| self.camp(f).is_some_and(|c| c.slots[i].state.away()))
        else {
            return false;
        };
        self.camp_sync(f, at);
        let camp = self.camp_mut(f).expect("a ledger");
        match camp.slots[i].state {
            SlotState::Left => {
                let s = &mut camp.slots[i];
                s.state = SlotState::Dead;
                s.party = Handle::NULL;
                s.line = None;
            }
            _ => camp.free(i, true),
        }
        self.chief_lost(f, cause, at);
        self.unblock(f, at);
        true
    }

    // ---- the heir ---------------------------------------------------------------------------

    /// `Ev::CampHeir`: the chief to come is due (MZB-406). Its stamp is the booking's; a changed
    /// heir (cancelled, replaced) leaves it stale.
    pub(super) fn camp_heir_event(&mut self, f: u32, stamp: u32, at: Tick) {
        match self.camp(f) {
            Some(c) if c.heir_stamp == stamp && c.heir.is_some() => {}
            _ => {
                self.stats.stale += 1;
                return;
            }
        }
        self.camp_heir(f, at);
    }

    /// Orders the heir, if it is due: one CAMP_RECONCILE of two lines (MZB-408), MASK of a
    /// warrior at home and SEAT_HEIR of the new chief, in the generation the books derive from the
    /// heir's due tick. Nothing is ordered while the camp is out, while no warrior is home (a body
    /// with no line), or while the pair is in flight; the camp is looked at again at each census
    /// and each due tick.
    pub(super) fn camp_heir(&mut self, f: u32, at: Tick) {
        let Some(camp) = self.camp(f).filter(|c| c.is_in()) else {
            return;
        };
        if !camp.heir.is_some_and(|h| h.due <= at) || camp.mask.is_some() || camp.seat.is_some() {
            return;
        }
        let warrior_home = camp
            .members()
            .any(|s| s.state == SlotState::Body && s.line.is_none());
        let Some(chief_kind) = self.chief_body_id(f).filter(|_| warrior_home) else {
            return;
        };
        let key = self.factions[f as usize].key;
        let generation = camp.heir_generation(key).expect("an heir");
        // A chief slot dead away: the heir is seated over its body in place (MZB-410).
        let old = camp
            .chief()
            .filter(|s| s.state == SlotState::Dead)
            .map_or(0, |s| u64::from(s.generation));
        let mask = ReconcileLine {
            slot: 0,
            generation: 0,
            body: self.home_body(f),
            action: ACTION_MASK,
            flags: self.line_flags(f, 0),
            a: 0,
            b: 0,
        };
        let seat = ReconcileLine {
            slot: CHIEF_SLOT,
            generation,
            body: chief_kind,
            action: ACTION_SEAT_HEIR,
            flags: self.line_flags(f, CHIEF_SLOT),
            a: old,
            b: 0,
        };
        self.send_lines(f, vec![mask, seat], at);
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
        // The chief's slot (MZB-401, MZB-406): only a culture with a chief body keeps one, it is
        // the last slot, and a chief to come is waited for exactly while it has no live slot.
        let chief_kind = self.culture(f).def.chief_body.is_some();
        match c.chief() {
            Some(_) if !chief_kind => {
                bad.push("MZB-401: a chief slot in a camp with no chief body".into())
            }
            Some(s) => {
                if s.state.away() && !self.parties.get(s.party).is_some_and(|p| p.chief) {
                    bad.push(format!(
                        "MZB-413: the chief is {:?} in a party that is not its own",
                        s.state
                    ));
                }
                if (s.state == SlotState::Dead) != c.heir.is_some() {
                    bad.push(format!(
                        "MZB-406: the chief slot is {:?} and the heir is {:?}",
                        s.state, c.heir
                    ));
                }
            }
            None if chief_kind && c.heir.is_none() => {
                bad.push("MZB-406: no chief slot and no heir waits".into());
            }
            None => {}
        }
        if c.seat.is_some() && c.heir.is_none() {
            bad.push("MZB-409: the heir is seated in flight with no heir".into());
        }
        if c.traded.windows(2).any(|w| w[0] >= w[1]) || c.traded.len() > TRADED_CAP {
            bad.push(
                "MZB-420: the trade memory is unsorted, repeats a player or is over its cap".into(),
            );
        }
        if c.chief().is_none_or(|s| s.state != SlotState::Body)
            && !c.traded.is_empty()
            && c.heir.is_some()
        {
            bad.push("MZB-420: a memory kept for a chief that is gone".into());
        }
        bad
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::CampRun;

    /// What a Frostmaw's loss does to the den's pressure depends on who dealt it (MZB-259): a
    /// district's walls are a player's, a village's are the faction's own affair. Either way a boss
    /// whose last unit falls loses its chief (MZB-266).
    #[test]
    fn a_villages_walls_are_no_players_loss_and_a_districts_are() {
        for (cause, pressure) in [(LossCause::Faction, 0.0), (LossCause::Player, 1.0)] {
            let mut r = CampRun::new(61, "frostmaw", 1);
            r.camp_in();
            let to = r.c.now() + 10;
            r.run(to);
            let p = r.raid().expect("a raid goes");
            r.c.lose_units(&mut r.w, p, |_| true, 1, cause);
            assert_eq!(r.c.stats.chiefs_killed, 1, "{cause:?}");
            let camp = r.c.camp(r.f).expect("a ledger");
            assert_eq!(camp.pressure.parts().0, pressure, "{cause:?}");
            assert!(camp.arrival_at.is_some(), "{cause:?}: a wait is booked");
        }
    }
}
