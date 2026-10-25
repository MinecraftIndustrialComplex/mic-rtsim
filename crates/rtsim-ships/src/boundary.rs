// SPDX-License-Identifier: AGPL-3.0-or-later
//! The records between the ships engine and a future ships plugin, and their reference codec.
//! `docs/boundary.md` §7 is the contract; this file and that section change together.
//!
//! No body mod is chosen (P8: Small Ships, Create Aeronautics Structures, the Seven Seas add-on),
//! so the records name **abstract body kinds** (dictionary kind 3: `mic_rtsim:ship/sea_merchant`
//! and the rest, [`crate::ShipTunables`]); the plugin maps them to a mod's entities (P15) and plays
//! the ship and its crew as one group (C25). Ports and havens are reported by the plugin
//! (PORT_FOUND, HAVEN_FOUND), however it finds them (a structure, a marker block: also P8).
//!
//! Everything in docs/boundary.md §1 and §2 holds: little-endian, 8-byte header (`kind` u16,
//! `size` u16, `aux` u32), records on 8-byte boundaries with sizes a multiple of 8, reserved fields
//! zero, unknown kinds skipped. Journaled observations carry `obsSeq` at 8 and `gameTime` at 16;
//! commands carry `cmdSeq` at 8.
//!
//! **Kind ranges** (reserved, docs/boundary.md §7.1): 0x0070 to 0x007F latest-wins ship
//! observations, 0x0600 to 0x06FF journaled ship observations, 0x8600 to 0x86FF ship commands.
//! **Agents** are `0x53 << 56 | handle` (namespace byte 0x53, "S").

use rtsim_travel::Medium;

pub const SHIP_STATE: u16 = 0x0070;
pub const SHIP_MATERIALIZED: u16 = 0x0600;
pub const SHIP_SPAWN_REJECTED: u16 = 0x0601;
pub const SHIP_DEMATERIALIZED: u16 = 0x0602;
pub const SHIP_DAMAGED: u16 = 0x0603;
pub const SHIP_SUNK: u16 = 0x0604;
pub const SHIP_BOARDED: u16 = 0x0605;
pub const SHIP_CAPTURED: u16 = 0x0606;
pub const CARGO_LOOTED: u16 = 0x0607;
pub const SHIP_FIGHT: u16 = 0x0608;
pub const PORT_FOUND: u16 = 0x0609;
pub const HAVEN_FOUND: u16 = 0x060A;

pub const SHIP_MATERIALIZE: u16 = 0x8600;
pub const SHIP_TIMETABLE: u16 = 0x8601;
pub const SHIP_STEER: u16 = 0x8602;
pub const SHIP_DEPART: u16 = 0x8603;
pub const SHIP_CARGO: u16 = 0x8604;
pub const SHIP_DESPAWN: u16 = 0x8605;

/// The agent namespace byte (R13's scheme: dragons are 0x44).
pub const AGENT_NAMESPACE: u64 = 0x53;

pub type Uuid = (u64, u64);

/// SHIP_STATE flags.
pub const STATE_TICKING: u32 = 1;
pub const STATE_PLAYER_ABOARD: u32 = 2;
pub const STATE_IN_COMBAT: u32 = 4;

/// SHIP_MATERIALIZE flags.
pub const MAT_CLAMP_ABOVE_SURFACE: u32 = 1;
pub const MAT_HOSTILE: u32 = 2;
pub const MAT_DOCKED: u32 = 4;

/// SHIP_TIMETABLE flags.
pub const TT_DOCK_AT_END: u32 = 1;

/// SHIP_STEER flags.
pub const STEER_ATTACK: u32 = 1;

/// A ship's role in the records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Merchant = 1,
    Pirate = 2,
    Navy = 3,
}

impl Role {
    fn from_u8(v: u8) -> Option<Role> {
        Some(match v {
            1 => Role::Merchant,
            2 => Role::Pirate,
            3 => Role::Navy,
            _ => return None,
        })
    }
}

/// Who did something to a ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum By {
    None = 0,
    Player = 1,
    /// Another ship of the sim (its agent is in the record).
    Ship = 2,
    Other = 3,
}

impl By {
    fn from_u32(v: u32) -> Option<By> {
        Some(match v {
            0 => By::None,
            1 => By::Player,
            2 => By::Ship,
            3 => By::Other,
            _ => return None,
        })
    }
}

/// An item stack in a record: item dictionary id and count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    pub item: u32,
    pub count: i32,
}

fn medium_u8(m: Medium) -> u8 {
    match m {
        Medium::Sea => 1,
        Medium::Sky => 2,
        Medium::Road => 0,
    }
}

fn medium_of(v: u32) -> Result<Medium, CodecError> {
    match v {
        1 => Ok(Medium::Sea),
        2 => Ok(Medium::Sky),
        _ => Err(CodecError::BadValue("medium")),
    }
}

// ---- Observations ----

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub dimension: u32,
    pub hull_milli: u32,
    pub crew: u32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Materialized {
    pub agent: u64,
    pub epoch: u32,
    pub dimension: u32,
    pub entity: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRejected {
    pub agent: u64,
    pub epoch: u32,
    /// docs/boundary.md §2.3's SPAWN_REJECTED codes, plus 9 unknown body kind.
    pub reason: u32,
    pub cmd_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dematerialized {
    pub agent: u64,
    pub epoch: u32,
    /// docs/boundary.md §2.3's DEMATERIALIZED codes.
    pub reason: u32,
    pub dimension: u32,
    pub hull_milli: u32,
    pub crew: u32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Damaged {
    pub agent: u64,
    pub epoch: u32,
    pub by: By,
    pub hull_milli: u32,
    pub crew_lost: u32,
    pub player: Uuid,
    pub ship: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sunk {
    pub agent: u64,
    pub epoch: u32,
    pub by: By,
    pub dimension: u32,
    pub player: Uuid,
    pub ship: u64,
    pub op: u64,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boarded {
    pub agent: u64,
    pub epoch: u32,
    pub by: By,
    pub player: Uuid,
    pub ship: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Captured {
    pub agent: u64,
    pub epoch: u32,
    pub by: By,
    /// Crew the captor took into its own (M2's crew conversion); a player's capture takes none.
    pub crew_taken: u32,
    pub player: Uuid,
    pub ship: u64,
    pub op: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CargoLooted {
    pub agent: u64,
    pub epoch: u32,
    pub by: By,
    pub player: Uuid,
    pub ship: u64,
    pub op: u64,
    pub items: Vec<Item>,
}

/// A real fight the plugin saw to its end: what the observed floors learn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fight {
    pub attacker: u64,
    pub defender: u64,
    /// 1 the attacker won, 2 the defender won, 3 broke off.
    pub outcome: u32,
    pub attacker_crew: (u32, u32),
    pub defender_crew: (u32, u32),
    pub attacker_hull_milli: u32,
    pub defender_hull_milli: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortFound {
    /// The village's key (docs/boundary.md §4's packing).
    pub village: u64,
    pub medium: Medium,
    pub dimension: u32,
    /// Structure dictionary id (kind 4), 0 when the port is not a structure.
    pub structure: u32,
    pub dock: (i32, i32, i32),
    pub berths: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HavenFound {
    pub haven: u64,
    /// 1 sea pirates, 2 sky pirates.
    pub media: u32,
    pub dimension: u32,
    pub structure: u32,
    pub dock: (i32, i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Obs {
    State(State),
    Materialized(Materialized),
    SpawnRejected(SpawnRejected),
    Dematerialized(Dematerialized),
    Damaged(Damaged),
    Sunk(Sunk),
    Boarded(Boarded),
    Captured(Captured),
    CargoLooted(CargoLooted),
    Fight(Fight),
    PortFound(PortFound),
    HavenFound(HavenFound),
}

impl Obs {
    pub fn kind(&self) -> u16 {
        match self {
            Obs::State(_) => SHIP_STATE,
            Obs::Materialized(_) => SHIP_MATERIALIZED,
            Obs::SpawnRejected(_) => SHIP_SPAWN_REJECTED,
            Obs::Dematerialized(_) => SHIP_DEMATERIALIZED,
            Obs::Damaged(_) => SHIP_DAMAGED,
            Obs::Sunk(_) => SHIP_SUNK,
            Obs::Boarded(_) => SHIP_BOARDED,
            Obs::Captured(_) => SHIP_CAPTURED,
            Obs::CargoLooted(_) => CARGO_LOOTED,
            Obs::Fight(_) => SHIP_FIGHT,
            Obs::PortFound(_) => PORT_FOUND,
            Obs::HavenFound(_) => HAVEN_FOUND,
        }
    }

    pub fn is_journaled(&self) -> bool {
        self.kind() >= 0x0100
    }

    /// The ship and the epoch a record names (docs/replay-gate.md §5.3, RP-57): every kind that
    /// carries an epoch, and none that does not (the fight, the port and the haven are about no
    /// body). The replay's floor reads it, so a kind added here without an arm does not compile.
    pub fn ship_epoch(&self) -> Option<(u64, u32)> {
        match self {
            Obs::State(r) => Some((r.agent, r.epoch)),
            Obs::Materialized(r) => Some((r.agent, r.epoch)),
            Obs::SpawnRejected(r) => Some((r.agent, r.epoch)),
            Obs::Dematerialized(r) => Some((r.agent, r.epoch)),
            Obs::Damaged(r) => Some((r.agent, r.epoch)),
            Obs::Sunk(r) => Some((r.agent, r.epoch)),
            Obs::Boarded(r) => Some((r.agent, r.epoch)),
            Obs::Captured(r) => Some((r.agent, r.epoch)),
            Obs::CargoLooted(r) => Some((r.agent, r.epoch)),
            Obs::Fight(_) | Obs::PortFound(_) | Obs::HavenFound(_) => None,
        }
    }
}

// ---- Commands ----

/// A member of a ship's group (C25): the ship first, then its crew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub body: u32,
    /// 1 the captain.
    pub flags: u32,
}

/// A voyage's phase boundaries, absolute game ticks: departure, cruise, approach, arrival.
pub type Times = [i64; 4];

#[derive(Clone, Debug, PartialEq)]
pub struct Materialize {
    pub agent: u64,
    pub epoch: u32,
    pub body: u32,
    pub dimension: u32,
    pub flags: u32,
    pub pos: (f64, f64, f64),
    pub medium: Medium,
    pub role: Role,
    /// 0 dock, 1 depart, 2 cruise, 3 approach, 4 hunting, 5 sailing (a pirate's own leg).
    pub phase: u8,
    pub platform: u8,
    pub cruise_y: i32,
    /// The ship a pirate attacks (0 none).
    pub target: u64,
    pub crew: u32,
    pub hull_milli: u32,
    pub times: Times,
    /// Blocks covered departing and approaching (the slow harbour phases), so the plugin can
    /// follow the same positions the engine computes.
    pub harbour_blocks: u32,
    /// The voyage's path (block x, z), from its dock to the next.
    pub waypoints: Vec<(i32, i32)>,
    pub cargo: Vec<Item>,
    pub members: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Timetable {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub times: Times,
    pub cruise_y: i32,
    pub dock_y: i32,
    pub platform: u8,
    pub harbour_blocks: u32,
    pub waypoints: Vec<(i32, i32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub speed: f32,
    pub pos: (f64, f64, f64),
    pub target: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Depart {
    pub agent: u64,
    pub epoch: u32,
    /// 1 bored, 2 hold full, 3 damaged, 4 passing through, 5 recalled.
    pub reason: u32,
    pub timeout_ticks: u32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetCargo {
    pub agent: u64,
    pub epoch: u32,
    pub op: u64,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Despawn {
    pub agent: u64,
    pub epoch: u32,
    /// 1 lost, 2 recalled.
    pub reason: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Materialize(Materialize),
    Timetable(Timetable),
    Steer(Steer),
    Depart(Depart),
    SetCargo(SetCargo),
    Despawn(Despawn),
}

impl Cmd {
    pub fn kind(&self) -> u16 {
        match self {
            Cmd::Materialize(_) => SHIP_MATERIALIZE,
            Cmd::Timetable(_) => SHIP_TIMETABLE,
            Cmd::Steer(_) => SHIP_STEER,
            Cmd::Depart(_) => SHIP_DEPART,
            Cmd::SetCargo(_) => SHIP_CARGO,
            Cmd::Despawn(_) => SHIP_DESPAWN,
        }
    }

    /// The kind's name, for the replay gate's log (docs/replay-gate.md §4.3).
    pub fn name(&self) -> &'static str {
        match self {
            Cmd::Materialize(_) => "SHIP_MATERIALIZE",
            Cmd::Timetable(_) => "SHIP_TIMETABLE",
            Cmd::Steer(_) => "SHIP_STEER",
            Cmd::Depart(_) => "SHIP_DEPART",
            Cmd::SetCargo(_) => "SHIP_CARGO",
            Cmd::Despawn(_) => "SHIP_DESPAWN",
        }
    }

    pub fn agent(&self) -> u64 {
        match self {
            Cmd::Materialize(c) => c.agent,
            Cmd::Timetable(c) => c.agent,
            Cmd::Steer(c) => c.agent,
            Cmd::Depart(c) => c.agent,
            Cmd::SetCargo(c) => c.agent,
            Cmd::Despawn(c) => c.agent,
        }
    }
}

// ---- Codec ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    Short,
    BadSize,
    Unknown(u16),
    BadValue(&'static str),
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
    fn points(&mut self, off: usize, pts: &[(i32, i32)]) {
        for (k, p) in pts.iter().enumerate() {
            self.i32(off + 8 * k, p.0);
            self.i32(off + 8 * k + 4, p.1);
        }
    }
    fn times(&mut self, off: usize, t: &Times) {
        for (k, v) in t.iter().enumerate() {
            self.i64(off + 8 * k, *v);
        }
    }
    /// Writes the header for a record of `size` bytes (padded to 8).
    fn finish(&mut self, kind: u16, aux: u32, size: usize) {
        let size = size.div_ceil(8) * 8;
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
    fn points(&self, off: usize, n: usize) -> Result<Vec<(i32, i32)>, CodecError> {
        (0..n)
            .map(|k| Ok((self.i32(off + 8 * k)?, self.i32(off + 8 * k + 4)?)))
            .collect()
    }
    fn times(&self, off: usize) -> Result<Times, CodecError> {
        Ok([
            self.i64(off)?,
            self.i64(off + 8)?,
            self.i64(off + 16)?,
            self.i64(off + 24)?,
        ])
    }
}

fn by_of(v: u32) -> Result<By, CodecError> {
    By::from_u32(v).ok_or(CodecError::BadValue("by"))
}

/// Appends one observation record. `seq` and `game_time` fill the journaled prefix (ignored for
/// latest-wins kinds).
pub fn encode_obs(seq: u64, game_time: i64, o: &Obs, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    if o.is_journaled() {
        w.u64(8, seq);
        w.u64(16, game_time as u64);
    }
    match o {
        Obs::State(s) => {
            w.u64(8, s.agent);
            w.u32(16, s.epoch);
            w.u32(20, s.dimension);
            w.u32(24, s.hull_milli);
            w.u32(28, s.crew);
            w.pos(32, s.pos);
            w.finish(SHIP_STATE, s.flags, 56);
        }
        Obs::Materialized(m) => {
            w.u64(24, m.agent);
            w.u32(32, m.epoch);
            w.u32(36, m.dimension);
            w.uuid(40, m.entity);
            w.finish(SHIP_MATERIALIZED, 0, 56);
        }
        Obs::SpawnRejected(r) => {
            w.u64(24, r.agent);
            w.u32(32, r.epoch);
            w.u64(40, r.cmd_seq);
            w.finish(SHIP_SPAWN_REJECTED, r.reason, 48);
        }
        Obs::Dematerialized(d) => {
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.dimension);
            w.u32(40, d.hull_milli);
            w.u32(44, d.crew);
            w.pos(48, d.pos);
            w.finish(SHIP_DEMATERIALIZED, d.reason, 72);
        }
        Obs::Damaged(d) => {
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.hull_milli);
            w.u32(40, d.crew_lost);
            w.uuid(48, d.player);
            w.u64(64, d.ship);
            w.finish(SHIP_DAMAGED, d.by as u32, 72);
        }
        Obs::Sunk(s) => {
            w.u64(24, s.agent);
            w.u32(32, s.epoch);
            w.u32(36, s.dimension);
            w.uuid(40, s.player);
            w.u64(56, s.ship);
            w.u64(64, s.op);
            w.pos(72, s.pos);
            w.finish(SHIP_SUNK, s.by as u32, 96);
        }
        Obs::Boarded(b) => {
            w.u64(24, b.agent);
            w.u32(32, b.epoch);
            w.uuid(40, b.player);
            w.u64(56, b.ship);
            w.finish(SHIP_BOARDED, b.by as u32, 64);
        }
        Obs::Captured(c) => {
            w.u64(24, c.agent);
            w.u32(32, c.epoch);
            w.u32(36, c.crew_taken);
            w.uuid(40, c.player);
            w.u64(56, c.ship);
            w.u64(64, c.op);
            w.finish(SHIP_CAPTURED, c.by as u32, 72);
        }
        Obs::CargoLooted(c) => {
            w.u64(24, c.agent);
            w.u32(32, c.epoch);
            w.u32(36, c.by as u32);
            w.uuid(40, c.player);
            w.u64(56, c.ship);
            w.u64(64, c.op);
            w.items(72, &c.items);
            w.finish(CARGO_LOOTED, c.items.len() as u32, 72 + 16 * c.items.len());
        }
        Obs::Fight(f) => {
            w.u64(24, f.attacker);
            w.u64(32, f.defender);
            w.u32(40, f.attacker_crew.0);
            w.u32(44, f.attacker_crew.1);
            w.u32(48, f.defender_crew.0);
            w.u32(52, f.defender_crew.1);
            w.u32(56, f.attacker_hull_milli);
            w.u32(60, f.defender_hull_milli);
            w.finish(SHIP_FIGHT, f.outcome, 64);
        }
        Obs::PortFound(p) => {
            w.u64(24, p.village);
            w.u32(32, p.dimension);
            w.u32(36, p.structure);
            w.ipos(40, p.dock);
            w.u32(52, p.berths);
            w.finish(PORT_FOUND, u32::from(medium_u8(p.medium)), 56);
        }
        Obs::HavenFound(h) => {
            w.u64(24, h.haven);
            w.u32(32, h.dimension);
            w.u32(36, h.structure);
            w.ipos(40, h.dock);
            w.finish(HAVEN_FOUND, h.media, 56);
        }
    }
}

/// Reads one observation at the start of `b`: (record size, obsSeq, gameTime, record). Seq and
/// time are 0 for latest-wins kinds.
pub fn decode_obs(b: &[u8]) -> Result<(usize, u64, i64, Obs), CodecError> {
    let r = R { b };
    let kind = r.u16(0)?;
    let size = usize::from(r.u16(2)?);
    let aux = r.u32(4)?;
    if size < 8 || size % 8 != 0 || size > b.len() {
        return Err(CodecError::BadSize);
    }
    let r = R { b: &b[..size] };
    let journaled = kind >= 0x0100;
    let (seq, time) = if journaled {
        (r.u64(8)?, r.u64(16)? as i64)
    } else {
        (0, 0)
    };
    let o = match kind {
        SHIP_STATE => Obs::State(State {
            agent: r.u64(8)?,
            epoch: r.u32(16)?,
            flags: aux,
            dimension: r.u32(20)?,
            hull_milli: r.u32(24)?,
            crew: r.u32(28)?,
            pos: r.pos(32)?,
        }),
        SHIP_MATERIALIZED => Obs::Materialized(Materialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            dimension: r.u32(36)?,
            entity: r.uuid(40)?,
        }),
        SHIP_SPAWN_REJECTED => Obs::SpawnRejected(SpawnRejected {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            reason: aux,
            cmd_seq: r.u64(40)?,
        }),
        SHIP_DEMATERIALIZED => Obs::Dematerialized(Dematerialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            reason: aux,
            dimension: r.u32(36)?,
            hull_milli: r.u32(40)?,
            crew: r.u32(44)?,
            pos: r.pos(48)?,
        }),
        SHIP_DAMAGED => Obs::Damaged(Damaged {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: by_of(aux)?,
            hull_milli: r.u32(36)?,
            crew_lost: r.u32(40)?,
            player: r.uuid(48)?,
            ship: r.u64(64)?,
        }),
        SHIP_SUNK => Obs::Sunk(Sunk {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: by_of(aux)?,
            dimension: r.u32(36)?,
            player: r.uuid(40)?,
            ship: r.u64(56)?,
            op: r.u64(64)?,
            pos: r.pos(72)?,
        }),
        SHIP_BOARDED => Obs::Boarded(Boarded {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: by_of(aux)?,
            player: r.uuid(40)?,
            ship: r.u64(56)?,
        }),
        SHIP_CAPTURED => Obs::Captured(Captured {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            by: by_of(aux)?,
            crew_taken: r.u32(36)?,
            player: r.uuid(40)?,
            ship: r.u64(56)?,
            op: r.u64(64)?,
        }),
        CARGO_LOOTED => {
            let n = aux as usize;
            if 72 + 16 * n > size {
                return Err(CodecError::BadSize);
            }
            Obs::CargoLooted(CargoLooted {
                agent: r.u64(24)?,
                epoch: r.u32(32)?,
                by: by_of(r.u32(36)?)?,
                player: r.uuid(40)?,
                ship: r.u64(56)?,
                op: r.u64(64)?,
                items: r.items(72, n)?,
            })
        }
        SHIP_FIGHT => Obs::Fight(Fight {
            attacker: r.u64(24)?,
            defender: r.u64(32)?,
            outcome: aux,
            attacker_crew: (r.u32(40)?, r.u32(44)?),
            defender_crew: (r.u32(48)?, r.u32(52)?),
            attacker_hull_milli: r.u32(56)?,
            defender_hull_milli: r.u32(60)?,
        }),
        PORT_FOUND => Obs::PortFound(PortFound {
            village: r.u64(24)?,
            medium: medium_of(aux)?,
            dimension: r.u32(32)?,
            structure: r.u32(36)?,
            dock: r.ipos(40)?,
            berths: r.u32(52)?,
        }),
        HAVEN_FOUND => Obs::HavenFound(HavenFound {
            haven: r.u64(24)?,
            media: aux,
            dimension: r.u32(32)?,
            structure: r.u32(36)?,
            dock: r.ipos(40)?,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, time, o))
}

/// Appends one command record with `cmdSeq` `seq`.
pub fn encode_cmd(seq: u64, c: &Cmd, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    w.u64(8, seq);
    match c {
        Cmd::Materialize(m) => {
            w.u64(16, m.agent);
            w.u32(24, m.epoch);
            w.u32(28, m.body);
            w.u32(32, m.dimension);
            w.u32(36, m.flags);
            w.pos(40, m.pos);
            w.u8(64, medium_u8(m.medium));
            w.u8(65, m.role as u8);
            w.u8(66, m.phase);
            w.u8(67, m.platform);
            w.i32(68, m.cruise_y);
            w.u64(72, m.target);
            w.u32(80, m.crew);
            w.u32(84, m.hull_milli);
            w.u32(88, m.waypoints.len() as u32);
            w.u32(92, m.cargo.len() as u32);
            w.times(96, &m.times);
            w.u32(128, m.harbour_blocks);
            let mut off = 136;
            w.points(off, &m.waypoints);
            off += 8 * m.waypoints.len();
            w.items(off, &m.cargo);
            off += 16 * m.cargo.len();
            for (k, mb) in m.members.iter().enumerate() {
                w.u32(off + 8 * k, mb.body);
                w.u32(off + 8 * k + 4, mb.flags);
            }
            off += 8 * m.members.len();
            w.finish(SHIP_MATERIALIZE, m.members.len() as u32, off);
        }
        Cmd::Timetable(t) => {
            w.u64(16, t.agent);
            w.u32(24, t.epoch);
            w.u32(28, t.waypoints.len() as u32);
            w.times(32, &t.times);
            w.i32(64, t.cruise_y);
            w.i32(68, t.dock_y);
            w.u8(72, t.platform);
            w.u32(76, t.harbour_blocks);
            w.points(80, &t.waypoints);
            w.finish(SHIP_TIMETABLE, t.flags, 80 + 8 * t.waypoints.len());
        }
        Cmd::Steer(s) => {
            w.u64(16, s.agent);
            w.u32(24, s.epoch);
            w.f32(28, s.speed);
            w.pos(32, s.pos);
            w.u64(56, s.target);
            w.finish(SHIP_STEER, s.flags, 64);
        }
        Cmd::Depart(d) => {
            w.u64(16, d.agent);
            w.u32(24, d.epoch);
            w.u32(28, d.timeout_ticks);
            w.pos(32, d.pos);
            w.finish(SHIP_DEPART, d.reason, 56);
        }
        Cmd::SetCargo(s) => {
            w.u64(16, s.agent);
            w.u32(24, s.epoch);
            w.u64(32, s.op);
            w.items(40, &s.items);
            w.finish(SHIP_CARGO, s.items.len() as u32, 40 + 16 * s.items.len());
        }
        Cmd::Despawn(d) => {
            w.u64(16, d.agent);
            w.u32(24, d.epoch);
            w.finish(SHIP_DESPAWN, d.reason, 32);
        }
    }
}

/// Reads one command at the start of `b`: (record size, cmdSeq, record).
pub fn decode_cmd(b: &[u8]) -> Result<(usize, u64, Cmd), CodecError> {
    let r = R { b };
    let kind = r.u16(0)?;
    let size = usize::from(r.u16(2)?);
    let aux = r.u32(4)?;
    if size < 16 || size % 8 != 0 || size > b.len() {
        return Err(CodecError::BadSize);
    }
    let r = R { b: &b[..size] };
    let seq = r.u64(8)?;
    let c = match kind {
        SHIP_MATERIALIZE => {
            let nw = r.u32(88)? as usize;
            let nc = r.u32(92)? as usize;
            let nm = aux as usize;
            if 136 + 8 * nw + 16 * nc + 8 * nm > size {
                return Err(CodecError::BadSize);
            }
            let mut off = 136;
            let waypoints = r.points(off, nw)?;
            off += 8 * nw;
            let cargo = r.items(off, nc)?;
            off += 16 * nc;
            let members = (0..nm)
                .map(|k| {
                    Ok(Member {
                        body: r.u32(off + 8 * k)?,
                        flags: r.u32(off + 8 * k + 4)?,
                    })
                })
                .collect::<Result<Vec<_>, CodecError>>()?;
            Cmd::Materialize(Materialize {
                agent: r.u64(16)?,
                epoch: r.u32(24)?,
                body: r.u32(28)?,
                dimension: r.u32(32)?,
                flags: r.u32(36)?,
                pos: r.pos(40)?,
                medium: medium_of(u32::from(r.u8(64)?))?,
                role: Role::from_u8(r.u8(65)?).ok_or(CodecError::BadValue("role"))?,
                phase: r.u8(66)?,
                platform: r.u8(67)?,
                cruise_y: r.i32(68)?,
                target: r.u64(72)?,
                crew: r.u32(80)?,
                hull_milli: r.u32(84)?,
                times: r.times(96)?,
                harbour_blocks: r.u32(128)?,
                waypoints,
                cargo,
                members,
            })
        }
        SHIP_TIMETABLE => {
            let n = r.u32(28)? as usize;
            if 80 + 8 * n > size {
                return Err(CodecError::BadSize);
            }
            Cmd::Timetable(Timetable {
                agent: r.u64(16)?,
                epoch: r.u32(24)?,
                flags: aux,
                times: r.times(32)?,
                cruise_y: r.i32(64)?,
                dock_y: r.i32(68)?,
                platform: r.u8(72)?,
                harbour_blocks: r.u32(76)?,
                waypoints: r.points(80, n)?,
            })
        }
        SHIP_STEER => Cmd::Steer(Steer {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            flags: aux,
            speed: r.f32(28)?,
            pos: r.pos(32)?,
            target: r.u64(56)?,
        }),
        SHIP_DEPART => Cmd::Depart(Depart {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            reason: aux,
            timeout_ticks: r.u32(28)?,
            pos: r.pos(32)?,
        }),
        SHIP_CARGO => {
            let n = aux as usize;
            if 40 + 16 * n > size {
                return Err(CodecError::BadSize);
            }
            Cmd::SetCargo(SetCargo {
                agent: r.u64(16)?,
                epoch: r.u32(24)?,
                op: r.u64(32)?,
                items: r.items(40, n)?,
            })
        }
        SHIP_DESPAWN => Cmd::Despawn(Despawn {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            reason: aux,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, c))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One value of every command kind (the replay gate's test reads it).
    pub(crate) fn cmds() -> Vec<Cmd> {
        all_cmds(1)
    }

    /// One value of every observation kind.
    pub(crate) fn obs() -> Vec<Obs> {
        all_obs(1)
    }

    fn all_obs(n: usize) -> Vec<Obs> {
        let items: Vec<Item> = (0..n as u32)
            .map(|i| Item {
                item: i + 1,
                count: 3 * i as i32 - 5,
            })
            .collect();
        vec![
            Obs::State(State {
                agent: 0x53 << 56 | 9,
                epoch: 4,
                flags: STATE_TICKING | STATE_IN_COMBAT,
                dimension: 1,
                hull_milli: 640,
                crew: 3,
                pos: (1.5, 200.0, -3.25),
            }),
            Obs::Materialized(Materialized {
                agent: 7,
                epoch: 1,
                dimension: 1,
                entity: (11, 12),
            }),
            Obs::SpawnRejected(SpawnRejected {
                agent: 7,
                epoch: 2,
                reason: 9,
                cmd_seq: 44,
            }),
            Obs::Dematerialized(Dematerialized {
                agent: 7,
                epoch: 2,
                reason: 3,
                dimension: 1,
                hull_milli: 1000,
                crew: 4,
                pos: (9.0, 63.0, 10.0),
            }),
            Obs::Damaged(Damaged {
                agent: 7,
                epoch: 2,
                by: By::Ship,
                hull_milli: 500,
                crew_lost: 2,
                player: (0, 0),
                ship: 8,
            }),
            Obs::Sunk(Sunk {
                agent: 7,
                epoch: 2,
                by: By::Player,
                dimension: 1,
                player: (5, 6),
                ship: 0,
                op: 1 << 63 | 77,
                pos: (1.0, 2.0, 3.0),
            }),
            Obs::Boarded(Boarded {
                agent: 7,
                epoch: 2,
                by: By::Player,
                player: (5, 6),
                ship: 0,
            }),
            Obs::Captured(Captured {
                agent: 7,
                epoch: 2,
                by: By::Ship,
                crew_taken: 2,
                player: (0, 0),
                ship: 8,
                op: 1 << 63 | 78,
            }),
            Obs::CargoLooted(CargoLooted {
                agent: 7,
                epoch: 2,
                by: By::Player,
                player: (5, 6),
                ship: 0,
                op: 1 << 63 | 79,
                items,
            }),
            Obs::Fight(Fight {
                attacker: 8,
                defender: 7,
                outcome: 1,
                attacker_crew: (6, 4),
                defender_crew: (4, 0),
                attacker_hull_milli: 800,
                defender_hull_milli: 300,
            }),
            Obs::PortFound(PortFound {
                village: 0x1234,
                medium: Medium::Sea,
                dimension: 1,
                structure: 3,
                dock: (100, 63, -200),
                berths: 4,
            }),
            Obs::HavenFound(HavenFound {
                haven: 0x99,
                media: 3,
                dimension: 1,
                structure: 0,
                dock: (5, 70, 5),
            }),
        ]
    }

    fn all_cmds(n: usize) -> Vec<Cmd> {
        let pts: Vec<(i32, i32)> = (0..n as i32).map(|i| (i * 10, -i)).collect();
        vec![
            Cmd::Materialize(Materialize {
                agent: 0x53 << 56 | 3,
                epoch: 5,
                body: 2,
                dimension: 1,
                flags: MAT_CLAMP_ABOVE_SURFACE | MAT_HOSTILE,
                pos: (10.0, 216.0, 20.0),
                medium: Medium::Sky,
                role: Role::Pirate,
                phase: 4,
                platform: 2,
                cruise_y: 216,
                target: 0x53 << 56 | 4,
                crew: 5,
                hull_milli: 1000,
                times: [100, 200, 300, 400],
                harbour_blocks: 64,
                waypoints: pts.clone(),
                cargo: vec![Item { item: 3, count: 64 }; n],
                members: vec![Member { body: 2, flags: 0 }, Member { body: 5, flags: 1 }],
            }),
            Cmd::Timetable(Timetable {
                agent: 3,
                epoch: 5,
                flags: TT_DOCK_AT_END,
                times: [1, 2, 3, 4],
                cruise_y: 200,
                dock_y: 90,
                platform: 1,
                harbour_blocks: 32,
                waypoints: pts,
            }),
            Cmd::Steer(Steer {
                agent: 3,
                epoch: 5,
                flags: STEER_ATTACK,
                speed: 0.5,
                pos: (1.0, 2.0, 3.0),
                target: 9,
            }),
            Cmd::Depart(Depart {
                agent: 3,
                epoch: 5,
                reason: 1,
                timeout_ticks: 2400,
                pos: (4.0, 5.0, 6.0),
            }),
            Cmd::SetCargo(SetCargo {
                agent: 3,
                epoch: 5,
                op: 99,
                items: vec![Item { item: 1, count: 2 }; n],
            }),
            Cmd::Despawn(Despawn {
                agent: 3,
                epoch: 5,
                reason: 1,
            }),
        ]
    }

    #[test]
    fn every_record_round_trips_and_is_aligned() {
        for n in [0, 1, 7] {
            let mut buf = Vec::new();
            for (k, o) in all_obs(n).iter().enumerate() {
                encode_obs(100 + k as u64, 5000, o, &mut buf);
                assert_eq!(buf.len() % 8, 0);
            }
            let mut off = 0;
            for (k, o) in all_obs(n).iter().enumerate() {
                let (size, seq, time, got) = decode_obs(&buf[off..]).expect("decodes");
                assert_eq!(&got, o);
                if o.is_journaled() {
                    assert_eq!((seq, time), (100 + k as u64, 5000));
                }
                off += size;
            }
            assert_eq!(off, buf.len());
            let mut buf = Vec::new();
            for (k, c) in all_cmds(n).iter().enumerate() {
                encode_cmd(k as u64 + 1, c, &mut buf);
                assert_eq!(buf.len() % 8, 0);
            }
            let mut off = 0;
            for (k, c) in all_cmds(n).iter().enumerate() {
                let (size, seq, got) = decode_cmd(&buf[off..]).expect("decodes");
                assert_eq!(&got, c);
                assert_eq!(seq, k as u64 + 1);
                off += size;
            }
            assert_eq!(off, buf.len());
        }
    }

    #[test]
    fn kinds_stay_in_the_reserved_ranges() {
        for o in all_obs(1) {
            let k = o.kind();
            assert!(
                (0x0070..=0x007F).contains(&k) || (0x0600..=0x06FF).contains(&k),
                "{k:#x}"
            );
        }
        for c in all_cmds(1) {
            assert!((0x8600..=0x86FF).contains(&c.kind()));
        }
    }

    #[test]
    fn truncated_records_are_errors() {
        let mut buf = Vec::new();
        encode_cmd(1, &all_cmds(3)[0], &mut buf);
        assert!(decode_cmd(&buf[..buf.len() - 8]).is_err());
        let mut buf = Vec::new();
        encode_obs(1, 1, &all_obs(3)[8], &mut buf);
        assert!(decode_obs(&buf[..40]).is_err());
        buf[0] = 0x7F;
        buf[1] = 0x06;
        assert_eq!(decode_obs(&buf).unwrap_err(), CodecError::Unknown(0x067F));
    }
}
