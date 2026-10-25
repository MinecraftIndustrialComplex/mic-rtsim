// SPDX-License-Identifier: AGPL-3.0-or-later
//! The records between the conflict engine and a future conflict plugin, and their reference
//! codec. `docs/boundary.md` §9 is the contract; this file and that section change together.
//!
//! No body mod is chosen (P8: Goblin's Tyranny, Illager Invasion; `minecraft:pillager` is the
//! fallback raider), so the records name **abstract body kinds** (dictionary kind 3:
//! `mic_rtsim:conflict/goblin/warrior` and the rest, from the culture data); the plugin maps them
//! to a mod's entities (P15) and plays each party as one group (C25). D7: mob-like bodies.
//!
//! Everything in docs/boundary.md §1 and §2 holds: little-endian, 8-byte header (`kind` u16,
//! `size` u16, `aux` u32), records on 8-byte boundaries with sizes a multiple of 8, reserved fields
//! zero, unknown kinds skipped. Journaled observations carry `obsSeq` at 8 and `gameTime` at 16;
//! commands carry `cmdSeq` at 8.
//!
//! **Kind ranges** (reserved, docs/boundary.md §9.1): 0x0090 to 0x009F latest-wins conflict
//! observations, 0x0800 to 0x08FF journaled conflict observations, 0x8800 to 0x88FF conflict
//! commands. **Agents** are `0x43 << 56 | handle` (namespace byte 0x43, "C"). **Dictionary kind
//! 0x20** names Destroy pollution types (`destroy:smog`).

pub const BAND_STATE: u16 = 0x0090;
pub const CHIEF_STATE: u16 = 0x0091;
// 0x0092 is retired: HAVEN_CHEST was latest-wins there before it was journaled (MZB-446, review
// P3-3). 0x0093 is retired the same way: DIFFICULTY (MZB-17).
pub const FACTION_FOUND: u16 = 0x0800;
pub const TERRITORY_SIGNAL: u16 = 0x0801;
pub const POLLUTION: u16 = 0x0802;
pub const BAND_MATERIALIZED: u16 = 0x0803;
pub const BAND_SPAWN_REJECTED: u16 = 0x0804;
pub const BAND_DEMATERIALIZED: u16 = 0x0805;
pub const BAND_CASUALTIES: u16 = 0x0806;
pub const CHIEF_KILLED: u16 = 0x0807;
pub const ENVOY_ANSWER: u16 = 0x0808;
pub const TRIBUTE_PAID: u16 = 0x0809;
pub const GIFT_GIVEN: u16 = 0x080A;
pub const BAND_LOOTED: u16 = 0x080B;
pub const RAIL_PASS: u16 = 0x080C;
pub const TRAIN_PASSED: u16 = 0x080D;
pub const CAMP_DEATH: u16 = 0x080E;
pub const CAMP_CENSUS: u16 = 0x080F;
pub const TRADED: u16 = 0x0810;
pub const NOTICE_SHOWN: u16 = 0x0811;
pub const CAMP_BODIES: u16 = 0x0812;
pub const CAMP_OUT: u16 = 0x0813;
pub const CHIEF_PAID: u16 = 0x0814;
pub const DIFFICULTY: u16 = 0x0815;
/// An owed hunt was applied (MZB-425): the animals the raiding raptors would have hunted, killed.
pub const HUNT_APPLIED: u16 = 0x0816;
/// The chest the plugin chose for a grove's tribute (MZB-422), journaled so that a replay places
/// the stacks it placed (MZB-446).
pub const HAVEN_CHEST: u16 = 0x0817;

pub const TERRITORY: u16 = 0x8800;
pub const FACTION_NOTICE: u16 = 0x8801;
pub const ENVOY_MATERIALIZE: u16 = 0x8802;
pub const BAND_MATERIALIZE: u16 = 0x8803;
pub const BAND_STEER: u16 = 0x8804;
pub const BAND_RECALL: u16 = 0x8805;
pub const DEMAND_SETTLED: u16 = 0x8806;
pub const AMBUSH_SPOT: u16 = 0x8807;
pub const CAMP_RECONCILE: u16 = 0x8808;
pub const TRAIN_ROBBED: u16 = 0x8809;
pub const CHIEF_BODY: u16 = 0x880A;
pub const BAND_MARCH: u16 = 0x880B;
/// The animals a raid on an unwatched district leaves hunted (MZB-425), latest wins for each
/// faction and district.
pub const OWED_HUNT: u16 = 0x880C;

/// The agent namespace byte ("C"; dragons 0x44, ships 0x53, herds 0x48, industry 0x49).
pub const AGENT_NAMESPACE: u64 = 0x43;
/// Dictionary kind of Destroy pollution type names.
pub const DICT_POLLUTION: u8 = 0x20;

pub type Uuid = (u64, u64);

/// BAND_STATE flags.
pub const STATE_TICKING: u32 = 1;
pub const STATE_FIGHTING: u32 = 2;
/// FACTION_FOUND flags.
pub const FOUND_LATE: u32 = 1;
/// TERRITORY flags.
pub const TERRITORY_REMOVED: u32 = 1;
/// ENVOY_MATERIALIZE demand flags.
pub const DEMAND_TRIBUTE: u32 = 1;
pub const DEMAND_STOP: u32 = 2;
/// BAND_MATERIALIZE flags.
pub const BAND_HOSTILE: u8 = 1;
pub const BAND_CHIEF: u8 = 2;
pub const BAND_SURFACE: u8 = 4;
/// A fight's wave (`start_fight`'s and `wave`'s, which spawn on an arc), not a sighting's: Java
/// echoes the flags in BAND_MATERIALIZED, so that a replay tells a held decision's fight from a
/// sighting (docs/replay-gate.md §6.1, RP-56).
pub const BAND_FIGHT: u8 = 8;
/// BAND_MATERIALIZE member flags.
pub const MEMBER_CHIEF: u32 = 1;
/// The stack's units have no crystal on them (the unit fights as its `without_crystal` row).
pub const MEMBER_BARE: u32 = 2;
/// BAND_STEER flags.
pub const STEER_ATTACK: u32 = 1;

/// Who did something: the CASUALTIES and CHIEF_KILLED killer kinds.
pub const BY_NONE: u32 = 0;
pub const BY_PLAYER: u32 = 1;
pub const BY_MOB: u32 = 2;
pub const BY_OTHER: u32 = 3;
/// CAMP_DEATH only: a camp body carried off (Carry On, A10).
pub const BY_TAKEN: u32 = 4;

/// BAND_RECALL reasons.
pub const RECALL_BORED: u32 = 1;
pub const RECALL_ROUTED: u32 = 2;
pub const RECALL_DONE: u32 = 3;
pub const RECALL_PASSING: u32 = 4;
pub const RECALL_PLAYER_LEFT: u32 = 5;
pub const RECALL_SETTLED: u32 = 6;

/// ENVOY_ANSWER answers.
pub const ANSWER_ACCEPTED: u32 = 1;
pub const ANSWER_REFUSED: u32 = 2;
pub const ANSWER_ATTACKED: u32 = 3;

/// DEMAND_SETTLED outcomes.
pub const SETTLED_PAID: u32 = 1;
pub const SETTLED_CUT: u32 = 2;
pub const SETTLED_EXPIRED: u32 = 3;
pub const SETTLED_REFUSED: u32 = 4;
/// The envoy party was wiped out with no player among its killers: the demand is withdrawn.
pub const SETTLED_WITHDRAWN: u32 = 5;

/// TRAIN_PASSED flags.
pub const PASSED_WATCHED: u32 = 1;
/// CAMP_DEATH flags.
pub const DEATH_CHIEF: u32 = 1;
pub const DEATH_STRAY: u32 = 2;
/// CAMP_CENSUS flags.
pub const CENSUS_CHIEF: u32 = 1;
pub const CENSUS_CRYSTAL: u32 = 2;
pub const CENSUS_THRONE: u32 = 4;
/// CAMP_CENSUS body flags.
pub const BODY_STRAY: u32 = 1;
pub const BODY_ELSEWHERE: u32 = 2;
pub const BODY_CHIEF: u32 = 4;
/// The crystal is on this body (a chief body only; MZB-269, amended at review C).
pub const BODY_CRYSTAL: u32 = 8;
/// CAMP_BODIES outcomes, per line.
pub const LINE_DONE: u8 = 1;
pub const LINE_NOTHING: u8 = 2;
pub const LINE_STALE: u8 = 3;
/// CHIEF_PAID flags.
pub const PAID_STRAY: u32 = 1;
/// CHIEF_STATE and CHIEF_BODY flags.
pub const CHIEF_CRYSTAL: u32 = 1;
/// DIFFICULTY values (Minecraft's `Difficulty` ids).
pub const DIFFICULTY_PEACEFUL: u32 = 0;
pub const DIFFICULTY_EASY: u32 = 1;
pub const DIFFICULTY_NORMAL: u32 = 2;
pub const DIFFICULTY_HARD: u32 = 3;
/// AMBUSH_SPOT flags.
pub const AMBUSH_REMOVED: u32 = 1;
/// CAMP_RECONCILE flags.
pub const RECONCILE_THRONE: u32 = 1;
/// CAMP_RECONCILE line actions.
pub const ACTION_SPAWN: u8 = 1;
pub const ACTION_ADOPT: u8 = 2;
pub const ACTION_TAKE: u8 = 3;
pub const ACTION_SET_OUT: u8 = 4;
pub const ACTION_HAND_BACK: u8 = 5;
pub const ACTION_REMOVE: u8 = 6;
pub const ACTION_SEAT_HEIR: u8 = 7;
/// Take a warrior at home for the heir (slot 0, generation 0: Java picks the one farthest from
/// every player); always paired with `ACTION_SEAT_HEIR` in one CAMP_RECONCILE (MZB-408).
pub const ACTION_MASK: u8 = 8;
/// CAMP_RECONCILE line flags.
pub const LINE_CHIEF: u8 = 1;
pub const LINE_CRYSTAL: u8 = 2;
/// BAND_MARCH phases.
pub const MARCH_MUSTERING: u8 = 1;
pub const MARCH_OUT: u8 = 2;
pub const MARCH_WAITING: u8 = 3;
pub const MARCH_FIGHTING: u8 = 4;
pub const MARCH_HOME: u8 = 5;
/// BAND_MARCH flags.
pub const MARCH_CHIEF: u8 = 1;
pub const MARCH_ENDED: u8 = 2;
/// A joint attack's contingent (CONFLICT-LIFE.md §2.16): never set in this build.
pub const MARCH_JOINT: u8 = 4;

/// An item stack in a record: item dictionary id and count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    pub item: u32,
    pub count: i32,
}

/// A member stack: body dictionary id and count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stack {
    pub body: u32,
    pub count: u32,
    pub flags: u32,
}

// ---- Observations ----

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandState {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub dimension: u32,
    pub present: u32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FactionFound {
    pub faction: u64,
    pub flags: u32,
    pub dimension: u32,
    pub structure: u32,
    pub pos: (i32, i32, i32),
    /// The census at discovery: the camp's bodies at home, and `CENSUS_CHIEF` (the chief is
    /// present) and `CENSUS_CRYSTAL` (it holds its crystal).
    pub home_bodies: u16,
    pub census: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerritorySignal {
    /// The faction whose territory it is, 0 to find it by position.
    pub faction: u64,
    pub kind: u32,
    pub dimension: u32,
    pub count: u32,
    /// The acting player; (0, 0) for an unowned act (P14: the settlers).
    pub player: Uuid,
    pub pos: (i32, i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pollution {
    /// The industry district at the position, 0 for none.
    pub district: u64,
    pub dimension: u32,
    /// Pollution type, dictionary kind 0x20.
    pub ptype: u32,
    pub x: i32,
    pub z: i32,
    /// The change over the window, Destroy's units (an aggregate the plugin keeps per source).
    pub amount: i64,
    /// The window the amount was measured over, ticks; an amount of 0 says the source stopped.
    pub window_ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandMaterialized {
    pub agent: u64,
    pub epoch: u32,
    pub dimension: u32,
    pub wave: u32,
    pub spawned: u32,
    /// The BAND_MATERIALIZE's flags, echoed ([`BAND_FIGHT`] among them); 0 for an envoy's.
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandSpawnRejected {
    pub agent: u64,
    pub epoch: u32,
    pub wave: u32,
    /// docs/boundary.md §2.3's SPAWN_REJECTED codes, plus 9 unknown body kind.
    pub reason: u32,
    pub cmd_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandDematerialized {
    pub agent: u64,
    pub epoch: u32,
    pub reason: u32,
    /// Survivors per body (body dictionary id, count).
    pub survivors: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandCasualties {
    pub agent: u64,
    pub epoch: u32,
    pub by: u32,
    pub body: u32,
    pub count: u32,
    pub killer: Uuid,
    /// The plugin's op id (MZB-589). A record journaled on an entity can be committed again (the
    /// reassert: the same bytes under a new sequence), so the engine counts one op once per party
    /// (`Party::casualty_ops`), not in the world's `applied_ops`.
    pub op: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChiefKilled {
    pub agent: u64,
    pub epoch: u32,
    pub by: u32,
    pub killer: Uuid,
    pub op: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvoyAnswer {
    pub agent: u64,
    pub epoch: u32,
    pub answer: u32,
    pub player: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TributePaid {
    pub faction: u64,
    /// The envoy it was paid to, 0 at the camp.
    pub agent: u64,
    pub epoch: u32,
    pub player: Uuid,
    pub op: u64,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiftGiven {
    pub faction: u64,
    pub player: Uuid,
    pub op: u64,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandLooted {
    pub agent: u64,
    pub epoch: u32,
    pub op: u64,
    pub items: Vec<Item>,
}

/// The latest health and crystal of a camp's chief at home (latest-wins).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChiefState {
    pub faction: u64,
    /// `CHIEF_CRYSTAL`.
    pub flags: u32,
    pub health: f32,
}

/// A chest at a haven the tribute can go into (journaled, MZB-446; the position as LAIR_CHEST's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HavenChest {
    pub faction: u64,
    pub dimension: u32,
    pub pos: (i32, i32, i32),
    /// Its empty slots (a chest whose loot table is not rolled yet: all of them).
    pub free: u32,
}

/// The world's difficulty (journaled, at the game time it took effect): `DIFFICULTY_PEACEFUL` to
/// `DIFFICULTY_HARD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Difficulty {
    pub difficulty: u32,
}

/// A train's pass through a camp's range (MOWZIE.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailPass {
    pub faction: u64,
    /// SplitMix64 of Create's train UUID (docs/industry-boundary.md H19).
    pub train: u64,
    /// The train's owner; (0, 0) for none.
    pub owner: Uuid,
    pub dimension: u32,
    /// The sampled point nearest the camp.
    pub pos: (f64, f64, f64),
    pub entry: i64,
    pub exit: i64,
}

/// A train passing a lurking party's track point (MOWZIE.md §4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainPassed {
    pub agent: u64,
    pub epoch: u32,
    /// `PASSED_WATCHED`.
    pub flags: u32,
    /// The carriage nearest the point, from 0.
    pub carriage: u32,
    pub train: u64,
}

/// A camp body killed or carried off, at home or left behind (MOWZIE.md §4.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampDeath {
    pub faction: u64,
    /// The killer kind: `BY_NONE` to `BY_TAKEN`.
    pub by: u32,
    pub body: u32,
    /// `DEATH_CHIEF`, `DEATH_STRAY`.
    pub flags: u32,
    /// The body's slot and generation (0 for a stray).
    pub slot: u32,
    pub generation: u32,
    pub killer: Uuid,
    pub op: u64,
}

/// One body a census found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CensusBody {
    pub body: u32,
    /// `BODY_STRAY`, `BODY_ELSEWHERE`, `BODY_CHIEF`, `BODY_CRYSTAL`.
    pub flags: u32,
    pub slot: u32,
    pub generation: u32,
    pub uuid: Uuid,
}

/// A camp's bodies when it comes in (MOWZIE.md §4.9).
#[derive(Clone, Debug, PartialEq)]
pub struct CampCensus {
    pub faction: u64,
    /// The new epoch: a Java op id.
    pub epoch: u64,
    /// `CENSUS_CHIEF`, `CENSUS_CRYSTAL`, `CENSUS_THRONE`.
    pub flags: u32,
    pub health: f32,
    /// The Umvuthi's position and direction (the throne spot), with `CENSUS_THRONE`.
    pub throne: (f64, f64, f64),
    pub direction: f32,
    /// The body whose trade memory `traded` is.
    pub traded_slot: u32,
    pub traded_generation: u32,
    pub bodies: Vec<CensusBody>,
    pub traded: Vec<Uuid>,
}

/// A masked trade at a grove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Traded {
    pub faction: u64,
    pub player: Uuid,
    pub nuggets: u32,
    pub op: u64,
}

/// The plugin played a feud's in-world notice (FACTION_NOTICE) to its player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoticeShown {
    pub faction: u64,
    /// The notice's serial, as FACTION_NOTICE's (aux).
    pub serial: u32,
    /// The notice's player; (0, 0) for the settlers.
    pub player: Uuid,
}

/// One CAMP_RECONCILE line's answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineAnswer {
    /// The line's index in its CAMP_RECONCILE, from 0.
    pub line: u32,
    pub slot: u32,
    pub generation: u32,
    /// The action of the line this answers (`ACTION_*`): the answer carries it, so the engine books
    /// the world fact by it even when the slot's line is no longer in flight.
    pub action: u8,
    /// `LINE_DONE`, `LINE_NOTHING`, `LINE_STALE`.
    pub outcome: u8,
}

/// The answer to a CAMP_RECONCILE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CampBodies {
    pub faction: u64,
    pub epoch: u64,
    /// The reconcile's op id (the engine's).
    pub op: u64,
    pub lines: Vec<LineAnswer>,
}

/// A camp went out: a footprint chunk dropped below FULL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampOut {
    pub faction: u64,
    /// The epoch that ended.
    pub epoch: u64,
}

/// A player's first payment to an Umvuthi body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChiefPaid {
    pub faction: u64,
    pub slot: u32,
    pub generation: u32,
    /// `PAID_STRAY`.
    pub flags: u32,
    pub player: Uuid,
    pub op: u64,
    pub items: Vec<Item>,
}

/// An owed hunt was applied, whatever it killed (MZB-425, MZB-426): the hunt is spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HuntApplied {
    pub faction: u64,
    pub district: u64,
    pub op: u64,
    pub killed: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Obs {
    State(BandState),
    FactionFound(FactionFound),
    Signal(TerritorySignal),
    Pollution(Pollution),
    Materialized(BandMaterialized),
    SpawnRejected(BandSpawnRejected),
    Dematerialized(BandDematerialized),
    Casualties(BandCasualties),
    ChiefKilled(ChiefKilled),
    EnvoyAnswer(EnvoyAnswer),
    TributePaid(TributePaid),
    GiftGiven(GiftGiven),
    Looted(BandLooted),
    ChiefState(ChiefState),
    HavenChest(HavenChest),
    Difficulty(Difficulty),
    RailPass(RailPass),
    TrainPassed(TrainPassed),
    CampDeath(CampDeath),
    CampCensus(CampCensus),
    Traded(Traded),
    NoticeShown(NoticeShown),
    CampBodies(CampBodies),
    CampOut(CampOut),
    ChiefPaid(ChiefPaid),
    HuntApplied(HuntApplied),
}

impl Obs {
    pub fn kind(&self) -> u16 {
        match self {
            Obs::ChiefState(_) => CHIEF_STATE,
            Obs::HavenChest(_) => HAVEN_CHEST,
            Obs::Difficulty(_) => DIFFICULTY,
            Obs::RailPass(_) => RAIL_PASS,
            Obs::TrainPassed(_) => TRAIN_PASSED,
            Obs::CampDeath(_) => CAMP_DEATH,
            Obs::CampCensus(_) => CAMP_CENSUS,
            Obs::Traded(_) => TRADED,
            Obs::NoticeShown(_) => NOTICE_SHOWN,
            Obs::CampBodies(_) => CAMP_BODIES,
            Obs::CampOut(_) => CAMP_OUT,
            Obs::ChiefPaid(_) => CHIEF_PAID,
            Obs::HuntApplied(_) => HUNT_APPLIED,
            Obs::State(_) => BAND_STATE,
            Obs::FactionFound(_) => FACTION_FOUND,
            Obs::Signal(_) => TERRITORY_SIGNAL,
            Obs::Pollution(_) => POLLUTION,
            Obs::Materialized(_) => BAND_MATERIALIZED,
            Obs::SpawnRejected(_) => BAND_SPAWN_REJECTED,
            Obs::Dematerialized(_) => BAND_DEMATERIALIZED,
            Obs::Casualties(_) => BAND_CASUALTIES,
            Obs::ChiefKilled(_) => CHIEF_KILLED,
            Obs::EnvoyAnswer(_) => ENVOY_ANSWER,
            Obs::TributePaid(_) => TRIBUTE_PAID,
            Obs::GiftGiven(_) => GIFT_GIVEN,
            Obs::Looted(_) => BAND_LOOTED,
        }
    }

    pub fn is_journaled(&self) -> bool {
        self.kind() >= 0x0100
    }

    /// The plugin's op id, for the records that move items or trade (applied once, in the world's
    /// set). The camp's records have none to skip by: CAMP_DEATH is idempotent by its slot and
    /// generation, and CAMP_BODIES by its (op, line), kept per epoch (docs/boundary.md §9.6).
    /// HUNT_APPLIED's op is the engine's own, in a record that moves no items: it is booked by the
    /// state of the faction's hunts (review P3-6), so a set of them would only grow.
    /// BAND_CASUALTIES carries an op id too and is not here: there is one for every member death in
    /// the world's life, and the world's set never shrinks (D1), so the engine skips a copy by the
    /// party's own list (`Party::casualty_ops`, which goes with the party; MZB-589).
    pub fn op(&self) -> Option<u64> {
        match self {
            Obs::ChiefKilled(c) => Some(c.op),
            Obs::TributePaid(t) => Some(t.op),
            Obs::GiftGiven(g) => Some(g.op),
            Obs::Looted(l) => Some(l.op),
            Obs::Traded(t) => Some(t.op),
            Obs::ChiefPaid(c) => Some(c.op),
            _ => None,
        }
    }

    /// The party epoch the record names, as (agent, epoch): the engine's own numbering, which a
    /// replayed record or a copy folds into the agent's epoch floor, so that no epoch is used
    /// twice (docs/replay-gate.md §5.3, RP-7, RP-57). None for a kind that names none (the camp
    /// records' epoch is a camp's, Java's op id, which the engine never allocates) and for agent
    /// 0 (a tribute paid at the camp). Every kind is listed, so a new one must say.
    pub fn party_epoch(&self) -> Option<(u64, u32)> {
        let (agent, epoch) = match self {
            Obs::State(s) => (s.agent, s.epoch),
            Obs::Materialized(m) => (m.agent, m.epoch),
            Obs::SpawnRejected(r) => (r.agent, r.epoch),
            Obs::Dematerialized(d) => (d.agent, d.epoch),
            Obs::Casualties(c) => (c.agent, c.epoch),
            Obs::ChiefKilled(c) => (c.agent, c.epoch),
            Obs::EnvoyAnswer(a) => (a.agent, a.epoch),
            Obs::TributePaid(t) => (t.agent, t.epoch),
            Obs::Looted(l) => (l.agent, l.epoch),
            Obs::TrainPassed(t) => (t.agent, t.epoch),
            Obs::FactionFound(_)
            | Obs::Signal(_)
            | Obs::Pollution(_)
            | Obs::GiftGiven(_)
            | Obs::ChiefState(_)
            | Obs::HavenChest(_)
            | Obs::Difficulty(_)
            | Obs::RailPass(_)
            | Obs::CampDeath(_)
            | Obs::CampCensus(_)
            | Obs::Traded(_)
            | Obs::NoticeShown(_)
            | Obs::CampBodies(_)
            | Obs::CampOut(_)
            | Obs::ChiefPaid(_)
            | Obs::HuntApplied(_) => return None,
        };
        (agent != 0).then_some((agent, epoch))
    }
}

// ---- Commands ----

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Territory {
    pub faction: u64,
    pub flags: u32,
    pub dimension: u32,
    /// Which signals the plugin reports here (`SignalKind::bit`).
    pub signals: u32,
    pub x: i32,
    pub z: i32,
    pub radius: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notice {
    pub faction: u64,
    pub step: u32,
    /// The notice's identity (from 1, one per notice, counted engine-wide so that no two notices
    /// of a feud share one): NOTICE_SHOWN echoes it, so an answer to an older notice of the same
    /// feud changes nothing.
    pub serial: u32,
    /// Whom; (0, 0) for the settlers (any player near).
    pub player: Uuid,
    pub dimension: u32,
    pub signal: u32,
    pub x: i32,
    pub z: i32,
    pub grievance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnvoyMaterialize {
    pub agent: u64,
    pub epoch: u32,
    pub dimension: u32,
    pub faction: u64,
    pub pos: (f64, f64, f64),
    /// Whom the demand is to; (0, 0) for the settlers (any player).
    pub player: Uuid,
    pub demand_flags: u32,
    pub signal: u32,
    pub tribute_item: u32,
    pub tribute_count: i32,
    pub deadline: i64,
    pub members: Vec<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BandMaterialize {
    pub agent: u64,
    pub epoch: u32,
    pub dimension: u32,
    pub faction: u64,
    pub purpose: u8,
    pub flags: u8,
    pub wave: u16,
    /// The arc's centre: the watching player's position (x, z).
    pub arc_centre: (f64, f64),
    /// Where on the arc: degrees from +x toward +z, the band's side.
    pub bearing_deg: f32,
    pub arc_radius: u32,
    pub half_width_deg: f32,
    pub target: (f64, f64, f64),
    pub target_player: Uuid,
    pub speed: f32,
    pub members: Vec<Stack>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandSteer {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub speed: f32,
    pub pos: (f64, f64, f64),
    pub arrive: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandRecall {
    pub agent: u64,
    pub epoch: u32,
    pub reason: u32,
    pub timeout_ticks: u32,
    /// Toward where they leave (x, z).
    pub toward: (f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DemandSettled {
    pub faction: u64,
    pub outcome: u32,
    pub agent: u64,
    pub player: Uuid,
    pub signal: u32,
}

/// Where a lurking party waits beside a track (MOWZIE.md §4.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbushSpot {
    pub agent: u64,
    pub epoch: u32,
    /// `AMBUSH_REMOVED`: the lurk ended.
    pub flags: u32,
    pub dimension: u32,
    pub faction: u64,
    /// The sampled track point.
    pub track: (f64, f64, f64),
    /// Where the body stands.
    pub spot: (f64, f64, f64),
    /// Blocks from the track point within which a train passes it.
    pub reach: f32,
}

/// One CAMP_RECONCILE line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconcileLine {
    pub slot: u32,
    pub generation: u32,
    pub body: u32,
    /// `ACTION_SPAWN` to `ACTION_MASK`.
    pub action: u8,
    /// `LINE_CHIEF`, `LINE_CRYSTAL`.
    pub flags: u8,
    /// Adopt and remove a stray: its UUID. Set out and hand back: the party's agent and epoch.
    pub a: u64,
    pub b: u64,
}

/// The engine's order to a camp that is in (MOWZIE.md §4.9).
#[derive(Clone, Debug, PartialEq)]
pub struct CampReconcile {
    pub faction: u64,
    pub epoch: u64,
    /// Derived from the faction, epoch and decision (allocator byte 0x43, docs/boundary.md §9.2).
    pub op: u64,
    /// `RECONCILE_THRONE`: the throne spot is given.
    pub flags: u32,
    pub throne: (f64, f64, f64),
    pub direction: f32,
    pub lines: Vec<ReconcileLine>,
}

/// The train the robbed-train roar plays for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainRobbed {
    pub faction: u64,
    pub train: u64,
}

/// The hunt a raid left owed in a district (MZB-424, MZB-425): up to `count` animals within
/// `radius` blocks of the centre die when its chunks tick. One is in flight for each faction and
/// district, resent at each session start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwedHunt {
    pub faction: u64,
    pub district: u64,
    /// Booked once: the hunt's own id (the engine allocator's byte, the camp's hash).
    pub op: u64,
    /// The op this hunt replaced while it was in flight; 0 for none (MZB-449, the re-check).
    pub replaced: u64,
    pub dimension: u32,
    pub count: u32,
    /// The district's centre, in blocks.
    pub centre: (i32, i32),
    pub radius: u32,
    /// The animals to kill instead of `count` when the last op the plugin applied for this
    /// faction and district is `replaced`: what the replacing raid owes on its own.
    pub rest: u32,
}

/// A chief's health, crystal and trade memory, ahead of a stack, spawn line or hand-back.
#[derive(Clone, Debug, PartialEq)]
pub struct ChiefBody {
    pub faction: u64,
    /// The party's agent; 0 at home.
    pub agent: u64,
    /// The party's epoch, or the camp's at home.
    pub epoch: u64,
    pub health: f32,
    /// `CHIEF_CRYSTAL`.
    pub flags: u32,
    pub traded: Vec<Uuid>,
}

/// A party's march (MOWZIE.md §4.16): each record replaces the agent's earlier ones.
#[derive(Clone, Debug, PartialEq)]
pub struct BandMarch {
    pub agent: u64,
    pub revision: u32,
    pub dimension: u32,
    pub faction: u64,
    /// Dictionary kind 5.
    pub culture: u32,
    /// BAND_MATERIALIZE's purposes: 1 envoy to 6 plunder.
    pub kind: u8,
    /// `MARCH_MUSTERING` to `MARCH_HOME`.
    pub phase: u8,
    /// `MARCH_CHIEF`, `MARCH_ENDED`, `MARCH_JOINT`.
    pub flags: u8,
    /// The partner contingent's agent for a joint attack; 0 in this build.
    pub partner: u64,
    pub muster_end: i64,
    /// Unit counts by body kind.
    pub units: Vec<(u32, u32)>,
    /// From `route[seg]` at `times[seg]`: (x, z, tick), chart coordinates.
    pub waypoints: Vec<(i32, i32, i64)>,
}

impl BandMarch {
    /// Where the band is at game tick `t`, in the route's chart (MZB-454): the one position rule
    /// the engine's `party_chart_pos` and Java's `MarchPath.chartAt` share.
    ///
    /// - None with no waypoint; the first waypoint while mustering; the last while waiting or
    ///   fighting.
    /// - Out and home: the segment `s` is the largest index at most `len - 2` whose tick is at
    ///   most `t` (0 when none), and the position is `party_chart_pos`'s arithmetic on it: its
    ///   start when `t1 <= t0` or `t <= t0`, its end when `t >= t1`, else each axis
    ///   `(a + (b - a) * fr) as i32`, which truncates toward zero as Java's `(int)` does.
    ///
    /// The result is in the chart: `Conflict::party_pos` folds it into the canonical cell, Java's
    /// `MarchPath.at` folds the block centre.
    pub fn chart_pos(&self, t: i64) -> Option<(i32, i32)> {
        let w = &self.waypoints;
        let first = w.first()?;
        let last = w.last()?;
        match self.phase {
            MARCH_MUSTERING => Some((first.0, first.1)),
            MARCH_WAITING | MARCH_FIGHTING => Some((last.0, last.1)),
            MARCH_OUT | MARCH_HOME => {
                if w.len() < 2 {
                    return Some((first.0, first.1));
                }
                let s = w[..w.len() - 1].iter().rposition(|q| q.2 <= t).unwrap_or(0);
                let (a, b) = (w[s], w[s + 1]);
                if b.2 <= a.2 || t <= a.2 {
                    return Some((a.0, a.1));
                }
                if t >= b.2 {
                    return Some((b.0, b.1));
                }
                let fr = (t - a.2) as f64 / (b.2 - a.2) as f64;
                Some((
                    (f64::from(a.0) + (f64::from(b.0) - f64::from(a.0)) * fr) as i32,
                    (f64::from(a.1) + (f64::from(b.1) - f64::from(a.1)) * fr) as i32,
                ))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Territory(Territory),
    Notice(Notice),
    Envoy(EnvoyMaterialize),
    Materialize(BandMaterialize),
    Steer(BandSteer),
    Recall(BandRecall),
    Settled(DemandSettled),
    AmbushSpot(AmbushSpot),
    Reconcile(CampReconcile),
    TrainRobbed(TrainRobbed),
    ChiefBody(ChiefBody),
    March(BandMarch),
    OwedHunt(OwedHunt),
}

impl Cmd {
    pub fn kind(&self) -> u16 {
        match self {
            Cmd::AmbushSpot(_) => AMBUSH_SPOT,
            Cmd::Reconcile(_) => CAMP_RECONCILE,
            Cmd::TrainRobbed(_) => TRAIN_ROBBED,
            Cmd::ChiefBody(_) => CHIEF_BODY,
            Cmd::March(_) => BAND_MARCH,
            Cmd::OwedHunt(_) => OWED_HUNT,
            Cmd::Territory(_) => TERRITORY,
            Cmd::Notice(_) => FACTION_NOTICE,
            Cmd::Envoy(_) => ENVOY_MATERIALIZE,
            Cmd::Materialize(_) => BAND_MATERIALIZE,
            Cmd::Steer(_) => BAND_STEER,
            Cmd::Recall(_) => BAND_RECALL,
            Cmd::Settled(_) => DEMAND_SETTLED,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Cmd::Territory(_) => "TERRITORY",
            Cmd::Notice(_) => "FACTION_NOTICE",
            Cmd::Envoy(_) => "ENVOY_MATERIALIZE",
            Cmd::Materialize(_) => "BAND_MATERIALIZE",
            Cmd::Steer(_) => "BAND_STEER",
            Cmd::Recall(_) => "BAND_RECALL",
            Cmd::Settled(_) => "DEMAND_SETTLED",
            Cmd::AmbushSpot(_) => "AMBUSH_SPOT",
            Cmd::Reconcile(_) => "CAMP_RECONCILE",
            Cmd::TrainRobbed(_) => "TRAIN_ROBBED",
            Cmd::ChiefBody(_) => "CHIEF_BODY",
            Cmd::March(_) => "BAND_MARCH",
            Cmd::OwedHunt(_) => "OWED_HUNT",
        }
    }

    /// The agent a party's command addresses (dropped from the outbox at a session start, which
    /// derives them again).
    pub fn agent(&self) -> Option<u64> {
        match self {
            Cmd::Envoy(c) => Some(c.agent),
            Cmd::Materialize(c) => Some(c.agent),
            Cmd::Steer(c) => Some(c.agent),
            Cmd::Recall(c) => Some(c.agent),
            Cmd::AmbushSpot(c) => Some(c.agent),
            Cmd::March(c) => Some(c.agent),
            _ => None,
        }
    }
}

// ---- Codec ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    Short,
    BadSize,
    Unknown(u16),
    /// A list's count above [`MAX_ENTRIES`].
    TooMany,
}

/// The most entries any list of a variable-length record carries (docs/boundary.md §9.1). At it
/// every record's size still fits the header's u16: the largest, CAMP_CENSUS with both of its
/// lists at the cap, is 88 + 32 × 1024 + 16 × 1024 = 49,240 bytes. An encoder writes the first
/// `MAX_ENTRIES` entries of a longer list (the count it writes is theirs); a decoder refuses a
/// count above it.
pub const MAX_ENTRIES: usize = 1024;

/// The first [`MAX_ENTRIES`] of a list, as an encoder writes them.
fn capped<T>(v: &[T]) -> &[T] {
    &v[..v.len().min(MAX_ENTRIES)]
}

/// A list's count as read, refused above [`MAX_ENTRIES`].
fn count(n: u32) -> Result<usize, CodecError> {
    let n = n as usize;
    if n > MAX_ENTRIES {
        return Err(CodecError::TooMany);
    }
    Ok(n)
}

struct W<'a> {
    b: &'a mut Vec<u8>,
    start: usize,
}

impl W<'_> {
    fn at(&mut self, off: usize, v: &[u8]) {
        let i = self.start + off;
        if self.b.len() < i + v.len() {
            self.b.resize(i + v.len(), 0);
        }
        self.b[i..i + v.len()].copy_from_slice(v);
    }
    fn u8(&mut self, off: usize, v: u8) {
        self.at(off, &[v]);
    }
    fn u16(&mut self, off: usize, v: u16) {
        self.at(off, &v.to_le_bytes());
    }
    fn u32(&mut self, off: usize, v: u32) {
        self.at(off, &v.to_le_bytes());
    }
    fn i32(&mut self, off: usize, v: i32) {
        self.at(off, &v.to_le_bytes());
    }
    fn u64(&mut self, off: usize, v: u64) {
        self.at(off, &v.to_le_bytes());
    }
    fn i64(&mut self, off: usize, v: i64) {
        self.at(off, &v.to_le_bytes());
    }
    fn f32(&mut self, off: usize, v: f32) {
        self.at(off, &v.to_le_bytes());
    }
    fn f64(&mut self, off: usize, v: f64) {
        self.at(off, &v.to_le_bytes());
    }
    fn pos(&mut self, off: usize, p: (f64, f64, f64)) {
        self.f64(off, p.0);
        self.f64(off + 8, p.1);
        self.f64(off + 16, p.2);
    }
    fn ipos(&mut self, off: usize, p: (i32, i32, i32)) {
        self.i32(off, p.0);
        self.i32(off + 4, p.1);
        self.i32(off + 8, p.2);
    }
    fn uuid(&mut self, off: usize, u: Uuid) {
        self.u64(off, u.0);
        self.u64(off + 8, u.1);
    }
    fn items(&mut self, off: usize, items: &[Item]) {
        for (k, it) in items.iter().enumerate() {
            self.u32(off + 16 * k, it.item);
            self.i32(off + 16 * k + 4, it.count);
        }
    }
    fn pairs(&mut self, off: usize, v: &[(u32, u32)]) {
        for (k, p) in v.iter().enumerate() {
            self.u32(off + 8 * k, p.0);
            self.u32(off + 8 * k + 4, p.1);
        }
    }
    fn finish(&mut self, kind: u16, aux: u32, size: usize) {
        let size = size.div_ceil(8) * 8;
        // The caps keep every kind under it (MAX_ENTRIES); a record past it cannot be framed.
        assert!(
            size <= usize::from(u16::MAX),
            "a record of kind {kind:#06x} of {size} bytes does not fit its u16 size"
        );
        if self.b.len() < self.start + size {
            self.b.resize(self.start + size, 0);
        }
        self.u16(0, kind);
        self.u16(2, size as u16);
        self.u32(4, aux);
    }
}

struct R<'a> {
    b: &'a [u8],
}

impl R<'_> {
    fn get<const N: usize>(&self, off: usize) -> Result<[u8; N], CodecError> {
        self.b
            .get(off..off + N)
            .and_then(|s| s.try_into().ok())
            .ok_or(CodecError::Short)
    }
    fn u8(&self, off: usize) -> Result<u8, CodecError> {
        Ok(self.get::<1>(off)?[0])
    }
    fn u16(&self, off: usize) -> Result<u16, CodecError> {
        Ok(u16::from_le_bytes(self.get(off)?))
    }
    fn u32(&self, off: usize) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.get(off)?))
    }
    fn i32(&self, off: usize) -> Result<i32, CodecError> {
        Ok(i32::from_le_bytes(self.get(off)?))
    }
    fn u64(&self, off: usize) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.get(off)?))
    }
    fn i64(&self, off: usize) -> Result<i64, CodecError> {
        Ok(i64::from_le_bytes(self.get(off)?))
    }
    fn f32(&self, off: usize) -> Result<f32, CodecError> {
        Ok(f32::from_le_bytes(self.get(off)?))
    }
    fn f64(&self, off: usize) -> Result<f64, CodecError> {
        Ok(f64::from_le_bytes(self.get(off)?))
    }
    fn pos(&self, off: usize) -> Result<(f64, f64, f64), CodecError> {
        Ok((self.f64(off)?, self.f64(off + 8)?, self.f64(off + 16)?))
    }
    fn ipos(&self, off: usize) -> Result<(i32, i32, i32), CodecError> {
        Ok((self.i32(off)?, self.i32(off + 4)?, self.i32(off + 8)?))
    }
    fn uuid(&self, off: usize) -> Result<Uuid, CodecError> {
        Ok((self.u64(off)?, self.u64(off + 8)?))
    }
    fn items(&self, off: usize, n: usize) -> Result<Vec<Item>, CodecError> {
        (0..n)
            .map(|k| {
                Ok(Item {
                    item: self.u32(off + 16 * k)?,
                    count: self.i32(off + 16 * k + 4)?,
                })
            })
            .collect()
    }
    fn pairs(&self, off: usize, n: usize) -> Result<Vec<(u32, u32)>, CodecError> {
        (0..n)
            .map(|k| Ok((self.u32(off + 8 * k)?, self.u32(off + 8 * k + 4)?)))
            .collect()
    }
}

/// Appends one observation record. `seq` and `game_time` fill the journaled prefix (ignored for
/// latest-wins kinds).
pub fn encode_obs(seq: u64, game_time: i64, o: &Obs, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    if o.is_journaled() {
        w.u64(8, seq);
        w.i64(16, game_time);
    }
    match o {
        Obs::State(s) => {
            w.u64(8, s.agent);
            w.u32(16, s.epoch);
            w.u32(20, s.dimension);
            w.u32(24, s.present);
            w.pos(32, s.pos);
            w.finish(BAND_STATE, s.flags, 56);
        }
        Obs::FactionFound(f) => {
            w.u64(24, f.faction);
            w.u32(32, f.dimension);
            w.u32(36, f.structure);
            w.ipos(40, f.pos);
            w.u16(52, f.home_bodies);
            w.u8(54, f.census);
            w.finish(FACTION_FOUND, f.flags, 56);
        }
        Obs::Signal(s) => {
            w.u64(24, s.faction);
            w.u32(32, s.dimension);
            w.u32(36, s.count);
            w.uuid(40, s.player);
            w.ipos(56, s.pos);
            w.finish(TERRITORY_SIGNAL, s.kind, 72);
        }
        Obs::Pollution(p) => {
            w.u64(24, p.district);
            w.u32(32, p.dimension);
            w.u32(36, p.ptype);
            w.i32(40, p.x);
            w.i32(44, p.z);
            w.i64(48, p.amount);
            w.u32(56, p.window_ticks);
            w.finish(POLLUTION, 0, 64);
        }
        Obs::Materialized(m) => {
            w.u64(24, m.agent);
            w.u32(32, m.epoch);
            w.u32(36, m.dimension);
            w.u32(40, m.wave);
            w.u32(44, m.flags);
            w.finish(BAND_MATERIALIZED, m.spawned, 48);
        }
        Obs::SpawnRejected(r) => {
            w.u64(24, r.agent);
            w.u32(32, r.epoch);
            w.u32(36, r.wave);
            w.u64(40, r.cmd_seq);
            w.finish(BAND_SPAWN_REJECTED, r.reason, 48);
        }
        Obs::Dematerialized(d) => {
            let survivors = capped(&d.survivors);
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, survivors.len() as u32);
            w.pairs(40, survivors);
            w.finish(BAND_DEMATERIALIZED, d.reason, 40 + 8 * survivors.len());
        }
        Obs::Casualties(c) => {
            w.u64(24, c.agent);
            w.u32(32, c.epoch);
            w.u32(36, c.body);
            w.u32(40, c.count);
            w.uuid(48, c.killer);
            w.u64(64, c.op);
            w.finish(BAND_CASUALTIES, c.by, 72);
        }
        Obs::ChiefKilled(c) => {
            w.u64(24, c.agent);
            w.u32(32, c.epoch);
            w.uuid(40, c.killer);
            w.u64(56, c.op);
            w.finish(CHIEF_KILLED, c.by, 64);
        }
        Obs::EnvoyAnswer(a) => {
            w.u64(24, a.agent);
            w.u32(32, a.epoch);
            w.uuid(40, a.player);
            w.finish(ENVOY_ANSWER, a.answer, 56);
        }
        Obs::TributePaid(t) => {
            w.u64(24, t.faction);
            w.u64(32, t.agent);
            w.u32(40, t.epoch);
            w.uuid(48, t.player);
            w.u64(64, t.op);
            let items = capped(&t.items);
            w.items(72, items);
            w.finish(TRIBUTE_PAID, items.len() as u32, 72 + 16 * items.len());
        }
        Obs::GiftGiven(g) => {
            w.u64(24, g.faction);
            w.uuid(32, g.player);
            w.u64(48, g.op);
            let items = capped(&g.items);
            w.items(56, items);
            w.finish(GIFT_GIVEN, items.len() as u32, 56 + 16 * items.len());
        }
        Obs::Looted(l) => {
            w.u64(24, l.agent);
            w.u32(32, l.epoch);
            w.u64(40, l.op);
            let items = capped(&l.items);
            w.items(48, items);
            w.finish(BAND_LOOTED, items.len() as u32, 48 + 16 * items.len());
        }
        Obs::ChiefState(c) => {
            w.u64(8, c.faction);
            w.f32(16, c.health);
            w.finish(CHIEF_STATE, c.flags, 24);
        }
        Obs::HavenChest(h) => {
            w.u64(24, h.faction);
            w.u32(32, h.dimension);
            w.ipos(36, h.pos);
            w.u32(48, h.free);
            w.finish(HAVEN_CHEST, 0, 56);
        }
        Obs::Difficulty(d) => {
            w.u32(24, d.difficulty);
            w.finish(DIFFICULTY, 0, 32);
        }
        Obs::RailPass(p) => {
            w.u64(24, p.faction);
            w.u64(32, p.train);
            w.uuid(40, p.owner);
            w.u32(56, p.dimension);
            w.pos(64, p.pos);
            w.i64(88, p.entry);
            w.i64(96, p.exit);
            w.finish(RAIL_PASS, 0, 104);
        }
        Obs::TrainPassed(t) => {
            w.u64(24, t.agent);
            w.u32(32, t.epoch);
            w.u32(36, t.carriage);
            w.u64(40, t.train);
            w.finish(TRAIN_PASSED, t.flags, 48);
        }
        Obs::CampDeath(d) => {
            w.u64(24, d.faction);
            w.u32(32, d.body);
            w.u32(36, d.flags);
            w.u32(40, d.slot);
            w.u32(44, d.generation);
            w.uuid(48, d.killer);
            w.u64(64, d.op);
            w.finish(CAMP_DEATH, d.by, 72);
        }
        Obs::CampCensus(c) => {
            let (bodies, traded) = (capped(&c.bodies), capped(&c.traded));
            w.u64(24, c.faction);
            w.u64(32, c.epoch);
            w.u32(40, c.flags);
            w.f32(44, c.health);
            w.pos(48, c.throne);
            w.f32(72, c.direction);
            w.u32(76, traded.len() as u32);
            w.u32(80, c.traded_slot);
            w.u32(84, c.traded_generation);
            for (k, b) in bodies.iter().enumerate() {
                let o = 88 + 32 * k;
                w.u32(o, b.body);
                w.u32(o + 4, b.flags);
                w.u32(o + 8, b.slot);
                w.u32(o + 12, b.generation);
                w.uuid(o + 16, b.uuid);
            }
            let t0 = 88 + 32 * bodies.len();
            for (k, u) in traded.iter().enumerate() {
                w.uuid(t0 + 16 * k, *u);
            }
            w.finish(CAMP_CENSUS, bodies.len() as u32, t0 + 16 * traded.len());
        }
        Obs::Traded(t) => {
            w.u64(24, t.faction);
            w.uuid(32, t.player);
            w.u32(48, t.nuggets);
            w.u64(56, t.op);
            w.finish(TRADED, 0, 64);
        }
        Obs::NoticeShown(n) => {
            w.u64(24, n.faction);
            w.uuid(32, n.player);
            w.finish(NOTICE_SHOWN, n.serial, 48);
        }
        Obs::CampBodies(b) => {
            w.u64(24, b.faction);
            w.u64(32, b.epoch);
            w.u64(40, b.op);
            let lines = capped(&b.lines);
            for (k, l) in lines.iter().enumerate() {
                let o = 48 + 16 * k;
                w.u32(o, l.line);
                w.u32(o + 4, l.slot);
                w.u32(o + 8, l.generation);
                w.u8(o + 12, l.action);
                w.u8(o + 13, l.outcome);
                w.u16(o + 14, 0);
            }
            w.finish(CAMP_BODIES, lines.len() as u32, 48 + 16 * lines.len());
        }
        Obs::CampOut(c) => {
            w.u64(24, c.faction);
            w.u64(32, c.epoch);
            w.finish(CAMP_OUT, 0, 40);
        }
        Obs::HuntApplied(h) => {
            w.u64(24, h.faction);
            w.u64(32, h.district);
            w.u64(40, h.op);
            w.u32(48, h.killed);
            w.finish(HUNT_APPLIED, 0, 56);
        }
        Obs::ChiefPaid(c) => {
            w.u64(24, c.faction);
            w.u32(32, c.slot);
            w.u32(36, c.generation);
            w.u32(40, c.flags);
            w.uuid(48, c.player);
            w.u64(64, c.op);
            let items = capped(&c.items);
            w.items(72, items);
            w.finish(CHIEF_PAID, items.len() as u32, 72 + 16 * items.len());
        }
    }
}

/// A decoded observation: the journaled prefix (obsSeq, gameTime) when it has one.
pub type Decoded<T> = (Option<(u64, i64)>, T, usize);

/// Reads one observation record from the front of `b`: the journaled prefix, the record, and its
/// size.
pub fn decode_obs(b: &[u8]) -> Result<Decoded<Obs>, CodecError> {
    let r = R { b };
    let kind = r.u16(0)?;
    let size = usize::from(r.u16(2)?);
    if size < 8 || size % 8 != 0 || size > b.len() {
        return Err(CodecError::BadSize);
    }
    let r = R { b: &b[..size] };
    let aux = r.u32(4)?;
    let prefix = if kind >= 0x0100 {
        Some((r.u64(8)?, r.i64(16)?))
    } else {
        None
    };
    let o = match kind {
        BAND_STATE => Obs::State(BandState {
            agent: r.u64(8)?,
            epoch: r.u32(16)?,
            flags: aux,
            dimension: r.u32(20)?,
            present: r.u32(24)?,
            pos: r.pos(32)?,
        }),
        FACTION_FOUND => Obs::FactionFound(FactionFound {
            faction: r.u64(24)?,
            flags: aux,
            dimension: r.u32(32)?,
            structure: r.u32(36)?,
            pos: r.ipos(40)?,
            home_bodies: r.u16(52)?,
            census: r.u8(54)?,
        }),
        TERRITORY_SIGNAL => Obs::Signal(TerritorySignal {
            faction: r.u64(24)?,
            kind: aux,
            dimension: r.u32(32)?,
            count: r.u32(36)?,
            player: r.uuid(40)?,
            pos: r.ipos(56)?,
        }),
        POLLUTION => Obs::Pollution(Pollution {
            district: r.u64(24)?,
            dimension: r.u32(32)?,
            ptype: r.u32(36)?,
            x: r.i32(40)?,
            z: r.i32(44)?,
            amount: r.i64(48)?,
            window_ticks: r.u32(56)?,
        }),
        BAND_MATERIALIZED => Obs::Materialized(BandMaterialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            dimension: r.u32(36)?,
            wave: r.u32(40)?,
            spawned: aux,
            flags: r.u32(44)?,
        }),
        BAND_SPAWN_REJECTED => Obs::SpawnRejected(BandSpawnRejected {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            wave: r.u32(36)?,
            reason: aux,
            cmd_seq: r.u64(40)?,
        }),
        BAND_DEMATERIALIZED => {
            let n = count(r.u32(36)?)?;
            Obs::Dematerialized(BandDematerialized {
                agent: r.u64(24)?,
                epoch: r.u32(32)?,
                reason: aux,
                survivors: r.pairs(40, n)?,
            })
        }
        BAND_CASUALTIES => Obs::Casualties(BandCasualties {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: aux,
            body: r.u32(36)?,
            count: r.u32(40)?,
            killer: r.uuid(48)?,
            op: r.u64(64)?,
        }),
        CHIEF_KILLED => Obs::ChiefKilled(ChiefKilled {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: aux,
            killer: r.uuid(40)?,
            op: r.u64(56)?,
        }),
        ENVOY_ANSWER => Obs::EnvoyAnswer(EnvoyAnswer {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            answer: aux,
            player: r.uuid(40)?,
        }),
        TRIBUTE_PAID => Obs::TributePaid(TributePaid {
            faction: r.u64(24)?,
            agent: r.u64(32)?,
            epoch: r.u32(40)?,
            player: r.uuid(48)?,
            op: r.u64(64)?,
            items: r.items(72, count(aux)?)?,
        }),
        GIFT_GIVEN => Obs::GiftGiven(GiftGiven {
            faction: r.u64(24)?,
            player: r.uuid(32)?,
            op: r.u64(48)?,
            items: r.items(56, count(aux)?)?,
        }),
        BAND_LOOTED => Obs::Looted(BandLooted {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            op: r.u64(40)?,
            items: r.items(48, count(aux)?)?,
        }),
        CHIEF_STATE => Obs::ChiefState(ChiefState {
            faction: r.u64(8)?,
            flags: aux,
            health: r.f32(16)?,
        }),
        HAVEN_CHEST => Obs::HavenChest(HavenChest {
            faction: r.u64(24)?,
            dimension: r.u32(32)?,
            pos: r.ipos(36)?,
            free: r.u32(48)?,
        }),
        DIFFICULTY => Obs::Difficulty(Difficulty {
            difficulty: r.u32(24)?,
        }),
        RAIL_PASS => Obs::RailPass(RailPass {
            faction: r.u64(24)?,
            train: r.u64(32)?,
            owner: r.uuid(40)?,
            dimension: r.u32(56)?,
            pos: r.pos(64)?,
            entry: r.i64(88)?,
            exit: r.i64(96)?,
        }),
        TRAIN_PASSED => Obs::TrainPassed(TrainPassed {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            flags: aux,
            carriage: r.u32(36)?,
            train: r.u64(40)?,
        }),
        CAMP_DEATH => Obs::CampDeath(CampDeath {
            faction: r.u64(24)?,
            by: aux,
            body: r.u32(32)?,
            flags: r.u32(36)?,
            slot: r.u32(40)?,
            generation: r.u32(44)?,
            killer: r.uuid(48)?,
            op: r.u64(64)?,
        }),
        CAMP_CENSUS => {
            let n = count(aux)?;
            let t = count(r.u32(76)?)?;
            let bodies = (0..n)
                .map(|k| {
                    let o = 88 + 32 * k;
                    Ok(CensusBody {
                        body: r.u32(o)?,
                        flags: r.u32(o + 4)?,
                        slot: r.u32(o + 8)?,
                        generation: r.u32(o + 12)?,
                        uuid: r.uuid(o + 16)?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            let t0 = 88 + 32 * n;
            let traded = (0..t)
                .map(|k| r.uuid(t0 + 16 * k))
                .collect::<Result<Vec<_>, CodecError>>()?;
            Obs::CampCensus(CampCensus {
                faction: r.u64(24)?,
                epoch: r.u64(32)?,
                flags: r.u32(40)?,
                health: r.f32(44)?,
                throne: r.pos(48)?,
                direction: r.f32(72)?,
                traded_slot: r.u32(80)?,
                traded_generation: r.u32(84)?,
                bodies,
                traded,
            })
        }
        TRADED => Obs::Traded(Traded {
            faction: r.u64(24)?,
            player: r.uuid(32)?,
            nuggets: r.u32(48)?,
            op: r.u64(56)?,
        }),
        NOTICE_SHOWN => Obs::NoticeShown(NoticeShown {
            faction: r.u64(24)?,
            serial: aux,
            player: r.uuid(32)?,
        }),
        CAMP_BODIES => {
            let lines = (0..count(aux)?)
                .map(|k| {
                    let o = 48 + 16 * k;
                    Ok(LineAnswer {
                        line: r.u32(o)?,
                        slot: r.u32(o + 4)?,
                        generation: r.u32(o + 8)?,
                        action: r.u8(o + 12)?,
                        outcome: r.u8(o + 13)?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Obs::CampBodies(CampBodies {
                faction: r.u64(24)?,
                epoch: r.u64(32)?,
                op: r.u64(40)?,
                lines,
            })
        }
        CAMP_OUT => Obs::CampOut(CampOut {
            faction: r.u64(24)?,
            epoch: r.u64(32)?,
        }),
        HUNT_APPLIED => Obs::HuntApplied(HuntApplied {
            faction: r.u64(24)?,
            district: r.u64(32)?,
            op: r.u64(40)?,
            killed: r.u32(48)?,
        }),
        CHIEF_PAID => Obs::ChiefPaid(ChiefPaid {
            faction: r.u64(24)?,
            slot: r.u32(32)?,
            generation: r.u32(36)?,
            flags: r.u32(40)?,
            player: r.uuid(48)?,
            op: r.u64(64)?,
            items: r.items(72, count(aux)?)?,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((prefix, o, size))
}

/// Appends one command record with sequence `seq`.
pub fn encode_cmd(seq: u64, c: &Cmd, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    w.u64(8, seq);
    match c {
        Cmd::Territory(t) => {
            w.u64(16, t.faction);
            w.u32(24, t.dimension);
            w.u32(28, t.signals);
            w.i32(32, t.x);
            w.i32(36, t.z);
            w.u32(40, t.radius);
            w.finish(TERRITORY, t.flags, 48);
        }
        Cmd::Notice(n) => {
            w.u64(16, n.faction);
            w.uuid(24, n.player);
            w.u32(40, n.dimension);
            w.u32(44, n.signal);
            w.i32(48, n.x);
            w.i32(52, n.z);
            w.f32(56, n.grievance);
            w.u32(60, n.serial);
            w.finish(FACTION_NOTICE, n.step, 64);
        }
        Cmd::Envoy(e) => {
            w.u64(16, e.agent);
            w.u32(24, e.epoch);
            w.u32(28, e.dimension);
            w.u64(32, e.faction);
            w.pos(40, e.pos);
            w.uuid(64, e.player);
            w.u32(80, e.demand_flags);
            w.u32(84, e.signal);
            w.u32(88, e.tribute_item);
            w.i32(92, e.tribute_count);
            w.i64(96, e.deadline);
            let members = capped(&e.members);
            w.pairs(104, members);
            w.finish(
                ENVOY_MATERIALIZE,
                members.len() as u32,
                104 + 8 * members.len(),
            );
        }
        Cmd::Materialize(m) => {
            w.u64(16, m.agent);
            w.u32(24, m.epoch);
            w.u32(28, m.dimension);
            w.u64(32, m.faction);
            w.u8(40, m.purpose);
            w.u8(41, m.flags);
            w.u16(42, m.wave);
            w.f32(44, m.bearing_deg);
            w.f64(48, m.arc_centre.0);
            w.f64(56, m.arc_centre.1);
            w.u32(64, m.arc_radius);
            w.f32(68, m.half_width_deg);
            w.pos(72, m.target);
            w.uuid(96, m.target_player);
            w.f32(112, m.speed);
            let members = capped(&m.members);
            for (k, s) in members.iter().enumerate() {
                w.u32(120 + 16 * k, s.body);
                w.u32(124 + 16 * k, s.count);
                w.u32(128 + 16 * k, s.flags);
            }
            w.finish(
                BAND_MATERIALIZE,
                members.len() as u32,
                120 + 16 * members.len(),
            );
        }
        Cmd::Steer(s) => {
            w.u64(16, s.agent);
            w.u32(24, s.epoch);
            w.f32(28, s.speed);
            w.pos(32, s.pos);
            w.i64(56, s.arrive);
            w.finish(BAND_STEER, s.flags, 64);
        }
        Cmd::Recall(r) => {
            w.u64(16, r.agent);
            w.u32(24, r.epoch);
            w.u32(28, r.timeout_ticks);
            w.f64(32, r.toward.0);
            w.f64(40, r.toward.1);
            w.finish(BAND_RECALL, r.reason, 48);
        }
        Cmd::Settled(s) => {
            w.u64(16, s.faction);
            w.u64(24, s.agent);
            w.uuid(32, s.player);
            w.u32(48, s.signal);
            w.finish(DEMAND_SETTLED, s.outcome, 56);
        }
        Cmd::AmbushSpot(a) => {
            w.u64(16, a.agent);
            w.u32(24, a.epoch);
            w.u32(28, a.dimension);
            w.u64(32, a.faction);
            w.pos(40, a.track);
            w.pos(64, a.spot);
            w.f32(88, a.reach);
            w.finish(AMBUSH_SPOT, a.flags, 96);
        }
        Cmd::Reconcile(c) => {
            w.u64(16, c.faction);
            w.u64(24, c.epoch);
            w.u64(32, c.op);
            w.pos(40, c.throne);
            w.f32(64, c.direction);
            w.u32(68, c.flags);
            let lines = capped(&c.lines);
            for (k, l) in lines.iter().enumerate() {
                let o = 72 + 32 * k;
                w.u32(o, l.slot);
                w.u32(o + 4, l.generation);
                w.u32(o + 8, l.body);
                w.u8(o + 12, l.action);
                w.u8(o + 13, l.flags);
                w.u64(o + 16, l.a);
                w.u64(o + 24, l.b);
            }
            w.finish(CAMP_RECONCILE, lines.len() as u32, 72 + 32 * lines.len());
        }
        Cmd::TrainRobbed(t) => {
            w.u64(16, t.faction);
            w.u64(24, t.train);
            w.finish(TRAIN_ROBBED, 0, 32);
        }
        Cmd::ChiefBody(c) => {
            w.u64(16, c.faction);
            w.u64(24, c.agent);
            w.u64(32, c.epoch);
            w.f32(40, c.health);
            w.u32(44, c.flags);
            let traded = capped(&c.traded);
            for (k, u) in traded.iter().enumerate() {
                w.uuid(48 + 16 * k, *u);
            }
            w.finish(CHIEF_BODY, traded.len() as u32, 48 + 16 * traded.len());
        }
        Cmd::OwedHunt(h) => {
            w.u64(16, h.faction);
            w.u64(24, h.district);
            w.u64(32, h.op);
            w.u64(40, h.replaced);
            w.u32(48, h.dimension);
            w.u32(52, h.count);
            w.i32(56, h.centre.0);
            w.i32(60, h.centre.1);
            w.u32(64, h.radius);
            w.u32(68, h.rest);
            w.finish(OWED_HUNT, 0, 72);
        }
        Cmd::March(m) => {
            w.u64(16, m.agent);
            w.u32(24, m.revision);
            w.u32(28, m.dimension);
            w.u64(32, m.faction);
            w.u32(40, m.culture);
            w.u8(44, m.kind);
            w.u8(45, m.phase);
            w.u8(46, m.flags);
            w.u64(48, m.partner);
            w.i64(56, m.muster_end);
            let (units, waypoints) = (capped(&m.units), capped(&m.waypoints));
            w.u32(64, units.len() as u32);
            w.pairs(72, units);
            let w0 = 72 + 8 * units.len();
            for (k, p) in waypoints.iter().enumerate() {
                w.i32(w0 + 16 * k, p.0);
                w.i32(w0 + 16 * k + 4, p.1);
                w.i64(w0 + 16 * k + 8, p.2);
            }
            w.finish(
                BAND_MARCH,
                waypoints.len() as u32,
                w0 + 16 * waypoints.len(),
            );
        }
    }
}

/// Reads one command record: its sequence, the command and its size.
pub fn decode_cmd(b: &[u8]) -> Result<(u64, Cmd, usize), CodecError> {
    let r = R { b };
    let kind = r.u16(0)?;
    let size = usize::from(r.u16(2)?);
    if size < 16 || size % 8 != 0 || size > b.len() {
        return Err(CodecError::BadSize);
    }
    let r = R { b: &b[..size] };
    let aux = r.u32(4)?;
    let seq = r.u64(8)?;
    let c = match kind {
        TERRITORY => Cmd::Territory(Territory {
            faction: r.u64(16)?,
            flags: aux,
            dimension: r.u32(24)?,
            signals: r.u32(28)?,
            x: r.i32(32)?,
            z: r.i32(36)?,
            radius: r.u32(40)?,
        }),
        FACTION_NOTICE => Cmd::Notice(Notice {
            faction: r.u64(16)?,
            step: aux,
            player: r.uuid(24)?,
            dimension: r.u32(40)?,
            signal: r.u32(44)?,
            x: r.i32(48)?,
            z: r.i32(52)?,
            grievance: r.f32(56)?,
            serial: r.u32(60)?,
        }),
        ENVOY_MATERIALIZE => Cmd::Envoy(EnvoyMaterialize {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            dimension: r.u32(28)?,
            faction: r.u64(32)?,
            pos: r.pos(40)?,
            player: r.uuid(64)?,
            demand_flags: r.u32(80)?,
            signal: r.u32(84)?,
            tribute_item: r.u32(88)?,
            tribute_count: r.i32(92)?,
            deadline: r.i64(96)?,
            members: r.pairs(104, count(aux)?)?,
        }),
        BAND_MATERIALIZE => {
            let n = count(aux)?;
            let members = (0..n)
                .map(|k| {
                    Ok(Stack {
                        body: r.u32(120 + 16 * k)?,
                        count: r.u32(124 + 16 * k)?,
                        flags: r.u32(128 + 16 * k)?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Cmd::Materialize(BandMaterialize {
                agent: r.u64(16)?,
                epoch: r.u32(24)?,
                dimension: r.u32(28)?,
                faction: r.u64(32)?,
                purpose: r.u8(40)?,
                flags: r.u8(41)?,
                wave: r.u16(42)?,
                bearing_deg: r.f32(44)?,
                arc_centre: (r.f64(48)?, r.f64(56)?),
                arc_radius: r.u32(64)?,
                half_width_deg: r.f32(68)?,
                target: r.pos(72)?,
                target_player: r.uuid(96)?,
                speed: r.f32(112)?,
                members,
            })
        }
        BAND_STEER => Cmd::Steer(BandSteer {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            flags: aux,
            speed: r.f32(28)?,
            pos: r.pos(32)?,
            arrive: r.i64(56)?,
        }),
        BAND_RECALL => Cmd::Recall(BandRecall {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            reason: aux,
            timeout_ticks: r.u32(28)?,
            toward: (r.f64(32)?, r.f64(40)?),
        }),
        DEMAND_SETTLED => Cmd::Settled(DemandSettled {
            faction: r.u64(16)?,
            outcome: aux,
            agent: r.u64(24)?,
            player: r.uuid(32)?,
            signal: r.u32(48)?,
        }),
        AMBUSH_SPOT => Cmd::AmbushSpot(AmbushSpot {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            flags: aux,
            dimension: r.u32(28)?,
            faction: r.u64(32)?,
            track: r.pos(40)?,
            spot: r.pos(64)?,
            reach: r.f32(88)?,
        }),
        CAMP_RECONCILE => {
            let lines = (0..count(aux)?)
                .map(|k| {
                    let o = 72 + 32 * k;
                    Ok(ReconcileLine {
                        slot: r.u32(o)?,
                        generation: r.u32(o + 4)?,
                        body: r.u32(o + 8)?,
                        action: r.u8(o + 12)?,
                        flags: r.u8(o + 13)?,
                        a: r.u64(o + 16)?,
                        b: r.u64(o + 24)?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Cmd::Reconcile(CampReconcile {
                faction: r.u64(16)?,
                epoch: r.u64(24)?,
                op: r.u64(32)?,
                throne: r.pos(40)?,
                direction: r.f32(64)?,
                flags: r.u32(68)?,
                lines,
            })
        }
        TRAIN_ROBBED => Cmd::TrainRobbed(TrainRobbed {
            faction: r.u64(16)?,
            train: r.u64(24)?,
        }),
        CHIEF_BODY => Cmd::ChiefBody(ChiefBody {
            faction: r.u64(16)?,
            agent: r.u64(24)?,
            epoch: r.u64(32)?,
            health: r.f32(40)?,
            flags: r.u32(44)?,
            traded: (0..count(aux)?)
                .map(|k| r.uuid(48 + 16 * k))
                .collect::<Result<Vec<_>, CodecError>>()?,
        }),
        OWED_HUNT => Cmd::OwedHunt(OwedHunt {
            faction: r.u64(16)?,
            district: r.u64(24)?,
            op: r.u64(32)?,
            replaced: r.u64(40)?,
            dimension: r.u32(48)?,
            count: r.u32(52)?,
            centre: (r.i32(56)?, r.i32(60)?),
            radius: r.u32(64)?,
            rest: r.u32(68)?,
        }),
        BAND_MARCH => {
            let k = count(r.u32(64)?)?;
            let w0 = 72 + 8 * k;
            Cmd::March(BandMarch {
                agent: r.u64(16)?,
                revision: r.u32(24)?,
                dimension: r.u32(28)?,
                faction: r.u64(32)?,
                culture: r.u32(40)?,
                kind: r.u8(44)?,
                phase: r.u8(45)?,
                flags: r.u8(46)?,
                partner: r.u64(48)?,
                muster_end: r.i64(56)?,
                units: r.pairs(72, k)?,
                waypoints: (0..count(aux)?)
                    .map(|j| {
                        let o = w0 + 16 * j;
                        Ok((r.i32(o)?, r.i32(o + 4)?, r.i64(o + 8)?))
                    })
                    .collect::<Result<Vec<_>, CodecError>>()?,
            })
        }
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((seq, c, size))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn obs() -> Vec<Obs> {
        let items = vec![Item { item: 3, count: 12 }, Item { item: 9, count: -1 }];
        vec![
            Obs::State(BandState {
                agent: AGENT_NAMESPACE << 56 | 5,
                epoch: 2,
                flags: STATE_TICKING | STATE_FIGHTING,
                dimension: 1,
                present: 9,
                pos: (1.5, 64.0, -3.25),
            }),
            Obs::FactionFound(FactionFound {
                faction: 77,
                flags: FOUND_LATE,
                dimension: 1,
                structure: 4,
                pos: (900, 70, 1000),
                home_bodies: 7,
                census: (CENSUS_CHIEF | CENSUS_CRYSTAL) as u8,
            }),
            Obs::Signal(TerritorySignal {
                faction: 77,
                kind: 2,
                dimension: 1,
                count: 64,
                player: (11, 22),
                pos: (850, 66, 900),
            }),
            Obs::Pollution(Pollution {
                district: 3,
                dimension: 1,
                ptype: 1,
                x: 700,
                z: 780,
                amount: 40,
                window_ticks: 1200,
            }),
            Obs::Materialized(BandMaterialized {
                agent: 5,
                epoch: 1,
                dimension: 1,
                wave: 2,
                spawned: 8,
                flags: u32::from(BAND_HOSTILE | BAND_FIGHT),
            }),
            Obs::SpawnRejected(BandSpawnRejected {
                agent: 5,
                epoch: 1,
                wave: 0,
                reason: 9,
                cmd_seq: 99,
            }),
            Obs::Dematerialized(BandDematerialized {
                agent: 5,
                epoch: 1,
                reason: 2,
                survivors: vec![(1, 4), (2, 0), (3, 1)],
            }),
            Obs::Casualties(BandCasualties {
                agent: 5,
                epoch: 1,
                by: BY_PLAYER,
                body: 1,
                count: 3,
                killer: (11, 22),
                op: 1 << 63 | 3,
            }),
            Obs::ChiefKilled(ChiefKilled {
                agent: 5,
                epoch: 1,
                by: BY_PLAYER,
                killer: (11, 22),
                op: 1 << 63 | 4,
            }),
            Obs::EnvoyAnswer(EnvoyAnswer {
                agent: 6,
                epoch: 1,
                answer: ANSWER_REFUSED,
                player: (11, 22),
            }),
            Obs::TributePaid(TributePaid {
                faction: 77,
                agent: 6,
                epoch: 1,
                player: (11, 22),
                op: 1 << 63 | 5,
                items: items.clone(),
            }),
            Obs::GiftGiven(GiftGiven {
                faction: 77,
                player: (11, 22),
                op: 1 << 63 | 6,
                items: items.clone(),
            }),
            Obs::Looted(BandLooted {
                agent: 5,
                epoch: 1,
                op: 1 << 63 | 7,
                items: items.clone(),
            }),
            Obs::ChiefState(ChiefState {
                faction: 77,
                flags: CHIEF_CRYSTAL,
                health: 0.75,
            }),
            Obs::HavenChest(HavenChest {
                faction: 77,
                dimension: 1,
                pos: (905, 71, 1002),
                free: 27,
            }),
            Obs::Difficulty(Difficulty {
                difficulty: DIFFICULTY_PEACEFUL,
            }),
            Obs::RailPass(RailPass {
                faction: 77,
                train: 0xDEAD_BEEF,
                owner: (11, 22),
                dimension: 1,
                pos: (930.5, 66.0, 1010.25),
                entry: 4_000,
                exit: 4_160,
            }),
            Obs::TrainPassed(TrainPassed {
                agent: 5,
                epoch: 1,
                flags: PASSED_WATCHED,
                carriage: 2,
                train: 0xDEAD_BEEF,
            }),
            Obs::CampDeath(CampDeath {
                faction: 77,
                by: BY_TAKEN,
                body: 3,
                flags: DEATH_CHIEF,
                slot: 4,
                generation: 2,
                killer: (11, 22),
                op: 1 << 63 | 8,
            }),
            Obs::CampCensus(CampCensus {
                faction: 77,
                epoch: 1 << 63 | 9,
                flags: CENSUS_CHIEF | CENSUS_THRONE,
                health: 0.5,
                throne: (900.5, 70.0, 1000.5),
                direction: 90.0,
                traded_slot: 1,
                traded_generation: 3,
                bodies: vec![
                    CensusBody {
                        body: 1,
                        flags: 0,
                        slot: 2,
                        generation: 1,
                        uuid: (5, 6),
                    },
                    CensusBody {
                        body: 1,
                        flags: BODY_STRAY,
                        slot: 0,
                        generation: 0,
                        uuid: (7, 8),
                    },
                    CensusBody {
                        body: 4,
                        flags: BODY_CHIEF | BODY_ELSEWHERE,
                        slot: 1,
                        generation: 3,
                        uuid: (9, 10),
                    },
                ],
                traded: vec![(11, 22), (33, 44)],
            }),
            Obs::Traded(Traded {
                faction: 77,
                player: (11, 22),
                nuggets: 9,
                op: 1 << 63 | 10,
            }),
            Obs::NoticeShown(NoticeShown {
                faction: 77,
                serial: 3,
                player: (11, 22),
            }),
            Obs::CampBodies(CampBodies {
                faction: 77,
                epoch: 1 << 63 | 9,
                op: AGENT_NAMESPACE << 56 | 12,
                lines: vec![
                    LineAnswer {
                        line: 0,
                        slot: 2,
                        generation: 1,
                        action: ACTION_SPAWN,
                        outcome: LINE_DONE,
                    },
                    LineAnswer {
                        line: 1,
                        slot: 0,
                        generation: 0,
                        action: ACTION_REMOVE,
                        outcome: LINE_STALE,
                    },
                ],
            }),
            Obs::CampOut(CampOut {
                faction: 77,
                epoch: 1 << 63 | 9,
            }),
            Obs::ChiefPaid(ChiefPaid {
                faction: 77,
                slot: 1,
                generation: 3,
                flags: 0,
                player: (11, 22),
                op: 1 << 63 | 11,
                items,
            }),
            Obs::HuntApplied(HuntApplied {
                faction: 77,
                district: 9_000_000_017,
                op: AGENT_NAMESPACE << 56 | 31,
                killed: 3,
            }),
        ]
    }

    /// One command of each kind (the replay gate's table test reads them too, `outbox.rs`).
    pub(crate) fn cmds() -> Vec<Cmd> {
        vec![
            Cmd::Territory(Territory {
                faction: 77,
                flags: 0,
                dimension: 1,
                signals: 0b111100,
                x: 900,
                z: 1000,
                radius: 400,
            }),
            Cmd::Notice(Notice {
                faction: 77,
                step: 1,
                serial: 3,
                player: (0, 0),
                dimension: 1,
                signal: 0,
                x: 700,
                z: 780,
                grievance: 12.5,
            }),
            Cmd::Envoy(EnvoyMaterialize {
                agent: 6,
                epoch: 3,
                dimension: 1,
                faction: 77,
                pos: (1.0, 2.0, 3.0),
                player: (11, 22),
                demand_flags: DEMAND_TRIBUTE | DEMAND_STOP,
                signal: 0,
                tribute_item: 2,
                tribute_count: 15,
                deadline: 123_456,
                members: vec![(4, 1), (1, 2)],
            }),
            Cmd::Materialize(BandMaterialize {
                agent: 5,
                epoch: 1,
                dimension: 1,
                faction: 77,
                purpose: 3,
                flags: BAND_HOSTILE | BAND_CHIEF,
                wave: 1,
                arc_centre: (700.0, 780.0),
                bearing_deg: 45.0,
                arc_radius: 112,
                half_width_deg: 30.0,
                target: (700.0, 64.0, 780.0),
                target_player: (11, 22),
                speed: 0.02,
                members: vec![
                    Stack {
                        body: 1,
                        count: 8,
                        flags: MEMBER_CHIEF | MEMBER_BARE,
                    },
                    Stack {
                        body: 2,
                        count: 4,
                        flags: 0,
                    },
                ],
            }),
            Cmd::Steer(BandSteer {
                agent: 5,
                epoch: 1,
                flags: STEER_ATTACK,
                speed: 0.02,
                pos: (5.0, 6.0, 7.0),
                arrive: 999,
            }),
            Cmd::Recall(BandRecall {
                agent: 5,
                epoch: 1,
                reason: RECALL_ROUTED,
                timeout_ticks: 1200,
                toward: (900.0, 1000.0),
            }),
            Cmd::Settled(DemandSettled {
                faction: 77,
                outcome: SETTLED_WITHDRAWN,
                agent: 6,
                player: (11, 22),
                signal: 0,
            }),
            Cmd::AmbushSpot(AmbushSpot {
                agent: 5,
                epoch: 1,
                flags: AMBUSH_REMOVED,
                dimension: 1,
                faction: 77,
                track: (930.5, 66.0, 1010.25),
                spot: (934.5, 66.0, 1013.25),
                reach: 8.0,
            }),
            Cmd::Reconcile(CampReconcile {
                faction: 77,
                epoch: 1 << 63 | 9,
                op: AGENT_NAMESPACE << 56 | 12,
                flags: RECONCILE_THRONE,
                throne: (900.5, 70.0, 1000.5),
                direction: 90.0,
                lines: vec![
                    ReconcileLine {
                        slot: 2,
                        generation: 1,
                        body: 1,
                        action: ACTION_SPAWN,
                        flags: 0,
                        a: 0,
                        b: 0,
                    },
                    ReconcileLine {
                        slot: 3,
                        generation: 2,
                        body: 1,
                        action: ACTION_ADOPT,
                        flags: 0,
                        a: 7,
                        b: 8,
                    },
                    ReconcileLine {
                        slot: 1,
                        generation: 3,
                        body: 4,
                        action: ACTION_SET_OUT,
                        flags: LINE_CHIEF | LINE_CRYSTAL,
                        a: 5,
                        b: 1,
                    },
                ],
            }),
            Cmd::TrainRobbed(TrainRobbed {
                faction: 77,
                train: 0xDEAD_BEEF,
            }),
            Cmd::ChiefBody(ChiefBody {
                faction: 77,
                agent: 0,
                epoch: 1 << 63 | 9,
                health: 0.5,
                flags: CHIEF_CRYSTAL,
                traded: vec![(11, 22), (33, 44), (55, 66)],
            }),
            Cmd::OwedHunt(OwedHunt {
                faction: 77,
                district: 9_000_000_017,
                op: AGENT_NAMESPACE << 56 | 31,
                replaced: AGENT_NAMESPACE << 56 | 30,
                dimension: 1,
                count: 8,
                centre: (-1200, 3400),
                radius: 48,
                rest: 3,
            }),
            Cmd::OwedHunt(OwedHunt {
                faction: 77,
                district: 9_000_000_017,
                op: AGENT_NAMESPACE << 56 | 32,
                replaced: 0,
                dimension: 1,
                count: 2,
                centre: (i32::MIN, i32::MAX),
                radius: 48,
                rest: 0,
            }),
            Cmd::March(BandMarch {
                agent: 5,
                revision: 4,
                dimension: 1,
                faction: 77,
                culture: 2,
                kind: 3,
                phase: MARCH_OUT,
                flags: MARCH_CHIEF,
                partner: 0,
                muster_end: 2_400,
                units: vec![(1, 8), (2, 4)],
                waypoints: vec![(900, 1000, 2_400), (850, 960, 3_100), (700, 780, 9_000)],
            }),
        ]
    }

    /// docs/boundary.md §9.3 and §9.4: each record's size.
    fn documented_size(kind: u16, aux: u32, b: &[u8]) -> usize {
        let n = aux as usize;
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
        match kind {
            BAND_STATE => 56,
            CHIEF_STATE => 24,
            HAVEN_CHEST => 56,
            DIFFICULTY => 32,
            FACTION_FOUND => 56,
            TERRITORY_SIGNAL => 72,
            POLLUTION => 64,
            BAND_MATERIALIZED | BAND_SPAWN_REJECTED => 48,
            BAND_DEMATERIALIZED => 40 + 8 * u32_at(36),
            BAND_CASUALTIES => 72,
            CHIEF_KILLED => 64,
            ENVOY_ANSWER => 56,
            TRIBUTE_PAID => 72 + 16 * n,
            GIFT_GIVEN => 56 + 16 * n,
            BAND_LOOTED => 48 + 16 * n,
            RAIL_PASS => 104,
            TRAIN_PASSED => 48,
            CAMP_DEATH => 72,
            CAMP_CENSUS => 88 + 32 * n + 16 * u32_at(76),
            TRADED => 64,
            NOTICE_SHOWN => 48,
            CAMP_BODIES => 48 + 16 * n,
            CAMP_OUT => 40,
            CHIEF_PAID => 72 + 16 * n,
            HUNT_APPLIED => 56,
            TERRITORY => 48,
            FACTION_NOTICE => 64,
            ENVOY_MATERIALIZE => 104 + 8 * n,
            BAND_MATERIALIZE => 120 + 16 * n,
            BAND_STEER => 64,
            BAND_RECALL => 48,
            DEMAND_SETTLED => 56,
            AMBUSH_SPOT => 96,
            CAMP_RECONCILE => 72 + 32 * n,
            TRAIN_ROBBED => 32,
            OWED_HUNT => 72,
            CHIEF_BODY => 48 + 16 * n,
            BAND_MARCH => 72 + 8 * u32_at(64) + 16 * n,
            k => panic!("kind {k:#06x} has no documented size"),
        }
    }

    #[test]
    fn every_record_has_its_documented_size() {
        let mut seen = BTreeSet::new();
        for o in obs() {
            let mut b = Vec::new();
            encode_obs(1, 2, &o, &mut b);
            let (kind, aux) = (o.kind(), u32::from_le_bytes(b[4..8].try_into().unwrap()));
            assert_eq!(b.len(), documented_size(kind, aux, &b), "{o:?}");
            assert_eq!(usize::from(u16::from_le_bytes([b[2], b[3]])), b.len());
            seen.insert(kind);
        }
        for c in cmds() {
            let mut b = Vec::new();
            encode_cmd(1, &c, &mut b);
            let (kind, aux) = (c.kind(), u32::from_le_bytes(b[4..8].try_into().unwrap()));
            assert_eq!(b.len(), documented_size(kind, aux, &b), "{c:?}");
            assert_eq!(usize::from(u16::from_le_bytes([b[2], b[3]])), b.len());
            seen.insert(kind);
        }
        // Every kind of docs/boundary.md §9 has a sample (the hunt pair among them, MZB-425).
        assert_eq!(seen.len(), 39);
        // BAND_MARCH: 72 + 8 a body kind + 16 a waypoint (MOWZIE.md §4.16): 408 for 20
        // waypoints and two body kinds.
        let mut b = Vec::new();
        let Cmd::March(mut m) = cmds().pop().unwrap() else {
            panic!("the last sample is a march")
        };
        m.waypoints = (0..20).map(|i| (i, -i, i64::from(i) * 100)).collect();
        encode_cmd(1, &Cmd::March(m), &mut b);
        assert_eq!(b.len(), 408);
    }

    #[test]
    fn faction_found_carries_the_census_in_its_reserved_word() {
        let mut b = Vec::new();
        encode_obs(1, 2, &obs()[1], &mut b);
        assert_eq!(u16::from_le_bytes([b[52], b[53]]), 7, "home bodies at 52");
        assert_eq!(b[54], (CENSUS_CHIEF | CENSUS_CRYSTAL) as u8, "flags at 54");
        assert_eq!(b[55], 0, "55 reserved");
    }

    #[test]
    fn a_march_writes_no_partner_and_no_joint_flag_when_none_is_given() {
        let mut b = Vec::new();
        let c = cmds().pop().unwrap();
        encode_cmd(1, &c, &mut b);
        assert_eq!(&b[48..56], &[0; 8], "the partner agent");
        assert_eq!(b[46] & MARCH_JOINT, 0);
        assert_eq!(&b[68..72], &[0; 4], "68 reserved");
    }

    #[test]
    fn observations_round_trip() {
        let mut buf = Vec::new();
        let all = obs();
        for (i, o) in all.iter().enumerate() {
            encode_obs(100 + i as u64, 5_000 + i as i64, o, &mut buf);
        }
        let mut off = 0;
        for (i, o) in all.iter().enumerate() {
            assert_eq!(off % 8, 0);
            let (prefix, back, size) = decode_obs(&buf[off..]).unwrap();
            assert_eq!(&back, o);
            if o.is_journaled() {
                assert_eq!(prefix, Some((100 + i as u64, 5_000 + i as i64)));
            } else {
                assert_eq!(prefix, None);
            }
            off += size;
        }
        assert_eq!(off, buf.len());
    }

    #[test]
    fn commands_round_trip() {
        let mut buf = Vec::new();
        let all = cmds();
        for (i, c) in all.iter().enumerate() {
            encode_cmd(7 + i as u64, c, &mut buf);
        }
        let mut off = 0;
        for (i, c) in all.iter().enumerate() {
            let (seq, back, size) = decode_cmd(&buf[off..]).unwrap();
            assert_eq!(seq, 7 + i as u64);
            assert_eq!(&back, c);
            assert!(back.kind() >= 0x8800 && back.kind() <= 0x88FF);
            off += size;
        }
    }

    #[test]
    fn kinds_stay_in_their_ranges() {
        for o in obs() {
            let k = o.kind();
            assert!((0x0090..=0x009F).contains(&k) || (0x0800..=0x08FF).contains(&k));
        }
        let mut b = vec![0u8; 24];
        b[0..2].copy_from_slice(&0x08FFu16.to_le_bytes());
        b[2..4].copy_from_slice(&24u16.to_le_bytes());
        assert_eq!(decode_obs(&b), Err(CodecError::Unknown(0x08FF)));
        b[2..4].copy_from_slice(&12u16.to_le_bytes());
        assert_eq!(decode_obs(&b), Err(CodecError::BadSize));
    }

    /// The variable-length kinds (docs/boundary.md §9.1): where each list's count is, aux or a
    /// field.
    fn counts(kind: u16) -> &'static [Option<usize>] {
        match kind {
            BAND_DEMATERIALIZED => &[Some(36)],
            CAMP_CENSUS => &[None, Some(76)],
            BAND_MARCH => &[Some(64), None],
            TRIBUTE_PAID | GIFT_GIVEN | BAND_LOOTED | CAMP_BODIES | CHIEF_PAID
            | ENVOY_MATERIALIZE | BAND_MATERIALIZE | CAMP_RECONCILE | CHIEF_BODY => &[None],
            _ => &[],
        }
    }

    fn many_items(n: usize) -> Vec<Item> {
        (0..n as u32)
            .map(|i| Item {
                item: i + 1,
                count: i as i32,
            })
            .collect()
    }

    /// A sample with every list `n` entries long (by index, so a shorter one is a prefix).
    fn long_obs(o: &Obs, n: usize) -> Option<Obs> {
        let mut o = o.clone();
        let m = n as u32;
        match &mut o {
            Obs::Dematerialized(d) => d.survivors = (0..m).map(|i| (i + 1, i)).collect(),
            Obs::TributePaid(t) => t.items = many_items(n),
            Obs::GiftGiven(g) => g.items = many_items(n),
            Obs::Looted(l) => l.items = many_items(n),
            Obs::ChiefPaid(c) => c.items = many_items(n),
            Obs::CampCensus(c) => {
                c.bodies = (0..m)
                    .map(|i| CensusBody {
                        body: i + 1,
                        flags: 0,
                        slot: i,
                        generation: 1,
                        uuid: (u64::from(i), 7),
                    })
                    .collect();
                c.traded = (0..u64::from(m)).map(|i| (i, i + 1)).collect();
            }
            Obs::CampBodies(b) => {
                b.lines = (0..m)
                    .map(|i| LineAnswer {
                        line: i,
                        slot: i,
                        generation: 1,
                        action: ACTION_TAKE,
                        outcome: LINE_DONE,
                    })
                    .collect();
            }
            _ => return None,
        }
        Some(o)
    }

    fn long_cmd(c: &Cmd, n: usize) -> Option<Cmd> {
        let mut c = c.clone();
        let m = n as u32;
        match &mut c {
            Cmd::Envoy(e) => e.members = (0..m).map(|i| (i + 1, i)).collect(),
            Cmd::Materialize(b) => {
                b.members = (0..m)
                    .map(|i| Stack {
                        body: i + 1,
                        count: i,
                        flags: 0,
                    })
                    .collect();
            }
            Cmd::Reconcile(r) => {
                r.lines = (0..m)
                    .map(|i| ReconcileLine {
                        slot: i,
                        generation: 1,
                        body: 1,
                        action: ACTION_SPAWN,
                        flags: 0,
                        a: 0,
                        b: u64::from(i),
                    })
                    .collect();
            }
            Cmd::ChiefBody(b) => b.traded = (0..u64::from(m)).map(|i| (i, i + 1)).collect(),
            Cmd::March(b) => {
                b.units = (0..m).map(|i| (i + 1, i)).collect();
                b.waypoints = (0..m as i32).map(|i| (i, -i, i64::from(i))).collect();
            }
            _ => return None,
        }
        Some(c)
    }

    /// Every variable-length sample at `n` entries a list, encoded: (kind, the record).
    fn long_records(n: usize) -> Vec<(u16, Vec<u8>)> {
        let mut v = Vec::new();
        for o in obs().iter().filter_map(|o| long_obs(o, n)) {
            let mut b = Vec::new();
            encode_obs(1, 2, &o, &mut b);
            v.push((o.kind(), b));
        }
        for c in cmds().iter().filter_map(|c| long_cmd(c, n)) {
            let mut b = Vec::new();
            encode_cmd(1, &c, &mut b);
            v.push((c.kind(), b));
        }
        v
    }

    fn decode_any(b: &[u8]) -> Result<(), CodecError> {
        if u16::from_le_bytes([b[0], b[1]]) >= 0x8800 {
            decode_cmd(b).map(|_| ())
        } else {
            decode_obs(b).map(|_| ())
        }
    }

    /// Item 9 of the M1 review: past `MAX_ENTRIES` a list is cut to its first `MAX_ENTRIES`, so
    /// every kind's size fits the header's u16 (CAMP_CENSUS was past it at 2,048 bodies).
    #[test]
    fn every_list_is_capped_so_every_size_fits_its_u16() {
        let mut kinds = BTreeSet::new();
        for o in obs() {
            let Some(long) = long_obs(&o, MAX_ENTRIES + 5) else {
                continue;
            };
            let mut b = Vec::new();
            encode_obs(1, 2, &long, &mut b);
            let aux = u32::from_le_bytes(b[4..8].try_into().unwrap());
            assert_eq!(b.len(), documented_size(o.kind(), aux, &b));
            assert_eq!(usize::from(u16::from_le_bytes([b[2], b[3]])), b.len());
            let (_, back, size) = decode_obs(&b).unwrap();
            assert_eq!(size, b.len());
            assert_eq!(
                back,
                long_obs(&o, MAX_ENTRIES).unwrap(),
                "the first ones are kept"
            );
            kinds.insert(o.kind());
        }
        for c in cmds() {
            let Some(long) = long_cmd(&c, MAX_ENTRIES + 5) else {
                continue;
            };
            let mut b = Vec::new();
            encode_cmd(1, &long, &mut b);
            let aux = u32::from_le_bytes(b[4..8].try_into().unwrap());
            assert_eq!(b.len(), documented_size(c.kind(), aux, &b));
            assert_eq!(usize::from(u16::from_le_bytes([b[2], b[3]])), b.len());
            let (_, back, size) = decode_cmd(&b).unwrap();
            assert_eq!(size, b.len());
            assert_eq!(
                back,
                long_cmd(&c, MAX_ENTRIES).unwrap(),
                "the first ones are kept"
            );
            kinds.insert(c.kind());
        }
        // Every variable-length kind: 7 observations and 5 commands.
        assert_eq!(kinds.len(), 12);
        assert!(kinds.iter().all(|k| !counts(*k).is_empty()));
        // The largest record there is, CAMP_CENSUS with both lists at the cap.
        let census = long_records(MAX_ENTRIES)
            .into_iter()
            .find(|(k, _)| *k == CAMP_CENSUS)
            .unwrap();
        assert_eq!(census.1.len(), 49_240);
    }

    /// A count above `MAX_ENTRIES` is refused, even when the record holds that many entries.
    #[test]
    fn a_count_above_the_cap_is_refused() {
        let mut tried = 0;
        for (kind, rec) in long_records(MAX_ENTRIES) {
            assert_eq!(decode_any(&rec), Ok(()));
            for at in counts(kind) {
                let mut b = rec.clone();
                let too_many = (MAX_ENTRIES as u32 + 1).to_le_bytes();
                b[at.unwrap_or(4)..at.unwrap_or(4) + 4].copy_from_slice(&too_many);
                let aux = u32::from_le_bytes(b[4..8].try_into().unwrap());
                let size = documented_size(kind, aux, &b);
                b.resize(size, 0);
                b[2..4].copy_from_slice(&(size as u16).to_le_bytes());
                assert_eq!(decode_any(&b), Err(CodecError::TooMany), "{kind:#06x}");
                tried += 1;
            }
        }
        assert_eq!(tried, 14);
    }

    /// A record whose size stops short of the entries its counts say is refused. Cut by 16 bytes,
    /// a whole entry of every list (an item's second half is reserved, so 8 would cut nothing
    /// read).
    #[test]
    fn a_record_cut_short_of_its_lists_is_refused() {
        let records = long_records(3);
        assert_eq!(records.len(), 12);
        for (kind, rec) in records {
            let mut b = rec.clone();
            let size = b.len() - 16;
            b.truncate(size);
            b[2..4].copy_from_slice(&(size as u16).to_le_bytes());
            assert_eq!(decode_any(&b), Err(CodecError::Short), "{kind:#06x}");
            // Cut without its size: the size says more than there is.
            assert_eq!(
                decode_any(&rec[..rec.len() - 8]),
                Err(CodecError::BadSize),
                "{kind:#06x}"
            );
        }
    }

    /// Item 2 of the M1 review: FACTION_NOTICE carries its serial at 60, NOTICE_SHOWN in aux.
    #[test]
    fn a_notice_and_its_answer_carry_the_serial() {
        let notice = cmds()
            .into_iter()
            .find(|c| matches!(c, Cmd::Notice(_)))
            .unwrap();
        let mut b = Vec::new();
        encode_cmd(1, &notice, &mut b);
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
            1,
            "aux: the rung"
        );
        assert_eq!(
            u32::from_le_bytes(b[60..64].try_into().unwrap()),
            3,
            "60: the serial"
        );
        let shown = obs()
            .into_iter()
            .find(|o| matches!(o, Obs::NoticeShown(_)))
            .unwrap();
        let mut b = Vec::new();
        encode_obs(1, 2, &shown, &mut b);
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
            3,
            "aux: the serial"
        );
    }

    /// Item 6 of the M1 review: DIFFICULTY is journaled (obsSeq at 8, gameTime at 16).
    #[test]
    fn difficulty_is_journaled_at_its_game_time() {
        let o = Obs::Difficulty(Difficulty {
            difficulty: DIFFICULTY_HARD,
        });
        assert!(o.is_journaled());
        let mut b = Vec::new();
        encode_obs(41, 9_000, &o, &mut b);
        assert_eq!(b.len(), 32);
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 0x0815);
        assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 3);
        assert_eq!(decode_obs(&b), Ok((Some((41, 9_000)), o, 32)));
    }

    /// MZB-446 (review P3-3): HAVEN_CHEST is journaled (obsSeq at 8, gameTime at 16) at 0x0817, so
    /// a replay places the stacks the session did; the latest-wins id it had is retired.
    #[test]
    fn haven_chest_is_journaled_and_its_old_id_is_retired() {
        let o = Obs::HavenChest(HavenChest {
            faction: 77,
            dimension: 1,
            pos: (905, 71, -1002),
            free: 27,
        });
        assert!(o.is_journaled());
        let mut b = Vec::new();
        encode_obs(52, 12_000, &o, &mut b);
        assert_eq!(b.len(), 56);
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 0x0817);
        assert_eq!(u64::from_le_bytes(b[24..32].try_into().unwrap()), 77);
        assert_eq!(u32::from_le_bytes(b[48..52].try_into().unwrap()), 27);
        assert_eq!(decode_obs(&b), Ok((Some((52, 12_000)), o, 56)));
        // The retired id decodes as nothing.
        b[0..2].copy_from_slice(&0x0092u16.to_le_bytes());
        assert_eq!(decode_obs(&b), Err(CodecError::Unknown(0x0092)));
    }

    fn march_on(phase: u8, waypoints: Vec<(i32, i32, i64)>) -> BandMarch {
        BandMarch {
            agent: 5,
            revision: 1,
            dimension: 1,
            faction: 77,
            culture: 2,
            kind: 3,
            phase,
            flags: 0,
            partner: 0,
            muster_end: 0,
            units: Vec::new(),
            waypoints,
        }
    }

    /// MZB-454, MZB-477: the march's position rule, the flat vector. Out, route (0, 0), (100, 37),
    /// (100, -50) at ticks 1000, 1100, 1200. The -6 is truncation toward zero, not floor.
    #[test]
    fn a_march_is_placed_by_the_party_chart_pos_rule() {
        let m = march_on(
            MARCH_OUT,
            vec![(0, 0, 1_000), (100, 37, 1_100), (100, -50, 1_200)],
        );
        for (t, want) in [
            (999, (0, 0)),
            (1_000, (0, 0)),
            (1_050, (50, 18)),
            (1_100, (100, 37)),
            (1_150, (100, -6)),
            (1_200, (100, -50)),
            (1_300, (100, -50)),
        ] {
            assert_eq!(m.chart_pos(t), Some(want), "tick {t}");
        }
        // Home walks the same way: the rule is the phase's two arms, not its number.
        let h = march_on(MARCH_HOME, m.waypoints.clone());
        assert_eq!(h.chart_pos(1_150), Some((100, -6)));
        // Mustering is the route's first point, whatever the time; waiting and fighting are its
        // last.
        let w = vec![(0, 0, 1_000), (100, 37, 1_100), (100, -50, 1_200)];
        assert_eq!(
            march_on(MARCH_MUSTERING, w.clone()).chart_pos(1_150),
            Some((0, 0))
        );
        assert_eq!(
            march_on(MARCH_WAITING, w.clone()).chart_pos(0),
            Some((100, -50))
        );
        assert_eq!(march_on(MARCH_FIGHTING, w).chart_pos(0), Some((100, -50)));
        // No waypoint: no place. One waypoint out: that one.
        assert_eq!(march_on(MARCH_OUT, Vec::new()).chart_pos(5), None);
        assert_eq!(
            march_on(MARCH_HOME, vec![(7, -9, 40)]).chart_pos(500),
            Some((7, -9))
        );
        // A segment of no duration holds its start (`t1 <= t0`), and the walk goes on after it.
        let z = march_on(MARCH_OUT, vec![(0, 0, 10), (50, 50, 10), (90, 10, 30)]);
        assert_eq!(z.chart_pos(9), Some((0, 0)));
        assert_eq!(z.chart_pos(10), Some((50, 50)));
        assert_eq!(z.chart_pos(20), Some((70, 30)));
    }
}
