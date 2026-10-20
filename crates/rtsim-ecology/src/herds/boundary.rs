// SPDX-License-Identifier: AGPL-3.0-or-later
//! The herd records between the engine and a herd plugin, and their reference codec.
//! `docs/boundary.md` §8 is the contract; this file, that section and the core's
//! `herd/HerdRecords.java` (the vanilla adapter that speaks these for `minecraft:sheep` and the rest)
//! change together. `herds::synth::FakeHerds` plays the adapter in tests and the CLI.
//!
//! Everything in docs/boundary.md §1 and §2 holds: little-endian, 8-byte header (`kind` u16,
//! `size` u16, `aux` u32), records on 8-byte boundaries with sizes a multiple of 8, reserved fields
//! zero, unknown kinds skipped. Journaled observations carry `obsSeq` at 8 and `gameTime` at 16;
//! commands carry `cmdSeq` at 8.
//!
//! **Kind ranges** (reserved here, docs/boundary.md §8.1): 0x0080 to 0x008F latest-wins herd
//! observations, 0x0700 to 0x07FF journaled herd observations, 0x8700 to 0x87FF herd commands.
//!
//! **Species** travel as body-kind dictionary ids (dictionary kind 3: `minecraft:sheep` and the
//! others); [`HerdDict`] maps them.

use super::species::{Cohort, HerdSpecies};

pub const HERD_GROUP_STATE: u16 = 0x0080;
pub const HERD_MATERIALIZED: u16 = 0x0700;
pub const HERD_SPAWN_REJECTED: u16 = 0x0701;
pub const HERD_DEMATERIALIZED: u16 = 0x0702;
pub const HERD_KILLED: u16 = 0x0703;
pub const HERD_SHEARED: u16 = 0x0704;
pub const HERD_TAKEN: u16 = 0x0705;
pub const HERD_OBSTACLE: u16 = 0x0706;

pub const HERD_MATERIALIZE: u16 = 0x8700;
pub const HERD_STEER: u16 = 0x8701;
pub const HERD_DESPAWN: u16 = 0x8702;

pub type Uuid = (u64, u64);

/// HERD_GROUP_STATE flags.
pub const STATE_TICKING: u32 = 1;

/// HERD_MATERIALIZE flags.
pub const MAT_SURFACE: u32 = 1;

/// Member flags (HERD_MATERIALIZE member blocks and HERD_DEMATERIALIZED captures).
pub const MEMBER_SHEARED: u32 = 1;

/// Who killed an animal (as the bridge's HURT/DIED kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillerKind {
    None = 0,
    Player = 1,
    Mob = 2,
    Other = 3,
}

impl KillerKind {
    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::None,
            1 => Self::Player,
            2 => Self::Mob,
            3 => Self::Other,
            _ => return None,
        })
    }
}

/// How an animal was taken out of the sim (A10; the bridge's TAKEN codes, §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakenHow {
    Lead = 1,
    Taming = 2,
    Pickup = 3,
    OtherMod = 4,
    Plugin = 5,
}

impl TakenHow {
    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Lead,
            2 => Self::Taming,
            3 => Self::Pickup,
            4 => Self::OtherMod,
            5 => Self::Plugin,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupState {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub dimension: u32,
    /// Members still present.
    pub present: u32,
    /// The group's centroid.
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Materialized {
    pub agent: u64,
    pub epoch: u32,
    pub dimension: u32,
    pub spawned: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnRejected {
    pub agent: u64,
    pub epoch: u32,
    /// docs/boundary.md §2.3's SPAWN_REJECTED codes, plus 9 unknown species.
    pub reason: u32,
    pub cmd_seq: u64,
}

/// One member as the plugin captured it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capture {
    pub member: u64,
    pub flags: u32,
    pub health: f32,
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dematerialized {
    pub agent: u64,
    pub epoch: u32,
    /// docs/boundary.md §2.3's DEMATERIALIZED codes.
    pub reason: u32,
    pub members: Vec<Capture>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Killed {
    pub agent: u64,
    pub epoch: u32,
    pub member: u64,
    pub by: KillerKind,
    pub killer: Uuid,
    pub op: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheared {
    pub agent: u64,
    pub epoch: u32,
    pub member: u64,
    pub player: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Taken {
    pub agent: u64,
    pub epoch: u32,
    pub member: u64,
    pub how: TakenHow,
    pub player: Uuid,
    pub op: u64,
}

/// The group's members keep failing to pass a place: they are stuck at `stuck` while steered
/// toward `target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub agent: u64,
    pub epoch: u32,
    /// Members that failed to pass.
    pub failures: u32,
    pub present: u32,
    pub stuck: (i32, i32),
    pub target: (i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Obs {
    GroupState(GroupState),
    Materialized(Materialized),
    SpawnRejected(SpawnRejected),
    Dematerialized(Dematerialized),
    Killed(Killed),
    Sheared(Sheared),
    Taken(Taken),
    Obstacle(Obstacle),
}

impl Obs {
    pub fn kind(&self) -> u16 {
        match self {
            Obs::GroupState(_) => HERD_GROUP_STATE,
            Obs::Materialized(_) => HERD_MATERIALIZED,
            Obs::SpawnRejected(_) => HERD_SPAWN_REJECTED,
            Obs::Dematerialized(_) => HERD_DEMATERIALIZED,
            Obs::Killed(_) => HERD_KILLED,
            Obs::Sheared(_) => HERD_SHEARED,
            Obs::Taken(_) => HERD_TAKEN,
            Obs::Obstacle(_) => HERD_OBSTACLE,
        }
    }

    pub fn is_journaled(&self) -> bool {
        self.kind() >= 0x0100
    }

    pub fn agent(&self) -> u64 {
        match self {
            Obs::GroupState(x) => x.agent,
            Obs::Materialized(x) => x.agent,
            Obs::SpawnRejected(x) => x.agent,
            Obs::Dematerialized(x) => x.agent,
            Obs::Killed(x) => x.agent,
            Obs::Sheared(x) => x.agent,
            Obs::Taken(x) => x.agent,
            Obs::Obstacle(x) => x.agent,
        }
    }
}

/// One member to materialize: its identity, look and age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberSpawn {
    pub member: u64,
    /// Its identity seed: the plugin seeds the body's own random with it (a horse's health,
    /// speed and jump), so the same horse comes back with the same stats.
    pub seed: u64,
    /// A sheep's `Color` dye id; a horse's `Variant` (colour | markings << 8); 1 for a screaming
    /// goat; else 0.
    pub variant: u32,
    /// Vanilla's `Age`: negative for a baby (young members), 0 otherwise.
    pub vanilla_age: i32,
    pub age_days: u32,
    pub cohort: Cohort,
    /// [`MEMBER_SHEARED`].
    pub flags: u32,
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Materialize {
    pub agent: u64,
    pub epoch: u32,
    pub species: HerdSpecies,
    pub dimension: u32,
    pub flags: u32,
    /// The herd's centre and where it is heading.
    pub centre: (i32, i32),
    pub target: (i32, i32),
    /// Walking speed, blocks per tick.
    pub speed: f32,
    /// The footprint's radius, blocks.
    pub radius: u32,
    pub members: Vec<MemberSpawn>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    pub agent: u64,
    pub epoch: u32,
    pub speed: f32,
    pub target: (i32, i32),
    /// When the herd reaches it.
    pub arrive: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Despawn {
    pub agent: u64,
    pub epoch: u32,
    /// 1 left view, 2 the player left, 3 recalled by the engine.
    pub reason: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Materialize(Materialize),
    Steer(Steer),
    Despawn(Despawn),
}

impl Cmd {
    pub fn kind(&self) -> u16 {
        match self {
            Cmd::Materialize(_) => HERD_MATERIALIZE,
            Cmd::Steer(_) => HERD_STEER,
            Cmd::Despawn(_) => HERD_DESPAWN,
        }
    }

    pub fn agent(&self) -> u64 {
        match self {
            Cmd::Materialize(m) => m.agent,
            Cmd::Steer(s) => s.agent,
            Cmd::Despawn(d) => d.agent,
        }
    }

    pub fn epoch(&self) -> u32 {
        match self {
            Cmd::Materialize(m) => m.epoch,
            Cmd::Steer(s) => s.epoch,
            Cmd::Despawn(d) => d.epoch,
        }
    }
}

/// Body-kind dictionary ids of the five species, as the two sides' DICT records define them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdDict {
    pub body: [u32; HerdSpecies::COUNT],
}

impl Default for HerdDict {
    fn default() -> Self {
        HerdDict {
            body: [11, 12, 13, 14, 15],
        }
    }
}

impl HerdDict {
    pub fn body_of(&self, s: HerdSpecies) -> u32 {
        self.body[s.index()]
    }

    pub fn species_of(&self, id: u32) -> Option<HerdSpecies> {
        self.body
            .iter()
            .position(|&b| b == id)
            .map(|i| HerdSpecies::ALL[i])
    }
}

// ---- codec ------------------------------------------------------------------------------------

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
    fn uuid(&mut self, off: usize, u: Uuid) {
        self.u64(off, u.0);
        self.u64(off + 8, u.1);
    }
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
    fn uuid(&self, off: usize) -> Result<Uuid, CodecError> {
        Ok((self.u64(off)?, self.u64(off + 8)?))
    }
}

const CAPTURE_BYTES: usize = 24;
const MEMBER_BYTES: usize = 40;

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
        Obs::GroupState(s) => {
            w.u64(8, s.agent);
            w.u32(16, s.epoch);
            w.u32(20, s.dimension);
            w.u32(24, s.present);
            w.f64(32, s.pos.0);
            w.f64(40, s.pos.1);
            w.f64(48, s.pos.2);
            w.finish(HERD_GROUP_STATE, s.flags, 56);
        }
        Obs::Materialized(m) => {
            w.u64(24, m.agent);
            w.u32(32, m.epoch);
            w.u32(36, m.dimension);
            w.finish(HERD_MATERIALIZED, m.spawned, 40);
        }
        Obs::SpawnRejected(r) => {
            w.u64(24, r.agent);
            w.u32(32, r.epoch);
            w.u64(40, r.cmd_seq);
            w.finish(HERD_SPAWN_REJECTED, r.reason, 48);
        }
        Obs::Dematerialized(d) => {
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.members.len() as u32);
            for (k, c) in d.members.iter().enumerate() {
                let o = 40 + CAPTURE_BYTES * k;
                w.u64(o, c.member);
                w.u32(o + 8, c.flags);
                w.f32(o + 12, c.health);
                w.i32(o + 16, c.x);
                w.i32(o + 20, c.z);
            }
            w.finish(
                HERD_DEMATERIALIZED,
                d.reason,
                40 + CAPTURE_BYTES * d.members.len(),
            );
        }
        Obs::Killed(k) => {
            w.u64(24, k.agent);
            w.u32(32, k.epoch);
            w.u64(40, k.member);
            w.uuid(48, k.killer);
            w.u64(64, k.op);
            w.finish(HERD_KILLED, k.by as u32, 72);
        }
        Obs::Sheared(s) => {
            w.u64(24, s.agent);
            w.u32(32, s.epoch);
            w.u64(40, s.member);
            w.uuid(48, s.player);
            w.finish(HERD_SHEARED, 0, 64);
        }
        Obs::Taken(t) => {
            w.u64(24, t.agent);
            w.u32(32, t.epoch);
            w.u64(40, t.member);
            w.uuid(48, t.player);
            w.u64(64, t.op);
            w.finish(HERD_TAKEN, t.how as u32, 72);
        }
        Obs::Obstacle(o) => {
            w.u64(24, o.agent);
            w.u32(32, o.epoch);
            w.u32(36, o.present);
            w.i32(40, o.stuck.0);
            w.i32(44, o.stuck.1);
            w.i32(48, o.target.0);
            w.i32(52, o.target.1);
            w.finish(HERD_OBSTACLE, o.failures, 56);
        }
    }
}

/// Reads one observation at the start of `b`: (record size, obsSeq, gameTime, record).
pub fn decode_obs(b: &[u8]) -> Result<(usize, u64, i64, Obs), CodecError> {
    let r = R { b };
    let kind = r.u16(0)?;
    let size = usize::from(r.u16(2)?);
    let aux = r.u32(4)?;
    if size < 8 || size % 8 != 0 || size > b.len() {
        return Err(CodecError::BadSize);
    }
    let r = R { b: &b[..size] };
    let (seq, time) = if kind >= 0x0100 {
        (r.u64(8)?, r.i64(16)?)
    } else {
        (0, 0)
    };
    let o = match kind {
        HERD_GROUP_STATE => Obs::GroupState(GroupState {
            agent: r.u64(8)?,
            epoch: r.u32(16)?,
            flags: aux,
            dimension: r.u32(20)?,
            present: r.u32(24)?,
            pos: (r.f64(32)?, r.f64(40)?, r.f64(48)?),
        }),
        HERD_MATERIALIZED => Obs::Materialized(Materialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            dimension: r.u32(36)?,
            spawned: aux,
        }),
        HERD_SPAWN_REJECTED => Obs::SpawnRejected(SpawnRejected {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            reason: aux,
            cmd_seq: r.u64(40)?,
        }),
        HERD_DEMATERIALIZED => {
            let n = r.u32(36)? as usize;
            if 40 + CAPTURE_BYTES * n > size {
                return Err(CodecError::BadSize);
            }
            let mut members = Vec::with_capacity(n);
            for k in 0..n {
                let o = 40 + CAPTURE_BYTES * k;
                members.push(Capture {
                    member: r.u64(o)?,
                    flags: r.u32(o + 8)?,
                    health: r.f32(o + 12)?,
                    x: r.i32(o + 16)?,
                    z: r.i32(o + 20)?,
                });
            }
            Obs::Dematerialized(Dematerialized {
                agent: r.u64(24)?,
                epoch: r.u32(32)?,
                reason: aux,
                members,
            })
        }
        HERD_KILLED => Obs::Killed(Killed {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            member: r.u64(40)?,
            by: KillerKind::from_u32(aux).ok_or(CodecError::BadValue("killer kind"))?,
            killer: r.uuid(48)?,
            op: r.u64(64)?,
        }),
        HERD_SHEARED => Obs::Sheared(Sheared {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            member: r.u64(40)?,
            player: r.uuid(48)?,
        }),
        HERD_TAKEN => Obs::Taken(Taken {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            member: r.u64(40)?,
            how: TakenHow::from_u32(aux).ok_or(CodecError::BadValue("taken how"))?,
            player: r.uuid(48)?,
            op: r.u64(64)?,
        }),
        HERD_OBSTACLE => Obs::Obstacle(Obstacle {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            failures: aux,
            present: r.u32(36)?,
            stuck: (r.i32(40)?, r.i32(44)?),
            target: (r.i32(48)?, r.i32(52)?),
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, time, o))
}

fn cohort_u8(c: Cohort) -> u8 {
    c.index() as u8
}

fn cohort_of(v: u8) -> Result<Cohort, CodecError> {
    Cohort::ALL
        .get(usize::from(v))
        .copied()
        .ok_or(CodecError::BadValue("cohort"))
}

/// Appends one command record.
pub fn encode_cmd(seq: u64, c: &Cmd, dict: &HerdDict, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    w.u64(8, seq);
    match c {
        Cmd::Materialize(m) => {
            w.u64(16, m.agent);
            w.u32(24, m.epoch);
            w.u32(28, dict.body_of(m.species));
            w.u32(32, m.dimension);
            w.u32(36, m.flags);
            w.i32(40, m.centre.0);
            w.i32(44, m.centre.1);
            w.i32(48, m.target.0);
            w.i32(52, m.target.1);
            w.f32(56, m.speed);
            w.u32(60, m.radius);
            for (k, x) in m.members.iter().enumerate() {
                let o = 64 + MEMBER_BYTES * k;
                w.u64(o, x.member);
                w.u64(o + 8, x.seed);
                w.u32(o + 16, x.variant);
                w.i32(o + 20, x.vanilla_age);
                w.u32(o + 24, x.age_days);
                w.u8(o + 28, cohort_u8(x.cohort));
                w.u8(o + 29, x.flags as u8);
                w.i32(o + 32, x.x);
                w.i32(o + 36, x.z);
            }
            w.finish(
                HERD_MATERIALIZE,
                m.members.len() as u32,
                64 + MEMBER_BYTES * m.members.len(),
            );
        }
        Cmd::Steer(s) => {
            w.u64(16, s.agent);
            w.u32(24, s.epoch);
            w.f32(28, s.speed);
            w.i32(32, s.target.0);
            w.i32(36, s.target.1);
            w.i64(40, s.arrive);
            w.finish(HERD_STEER, 0, 48);
        }
        Cmd::Despawn(d) => {
            w.u64(16, d.agent);
            w.u32(24, d.epoch);
            w.finish(HERD_DESPAWN, d.reason, 32);
        }
    }
}

/// Reads one command at the start of `b`: (record size, cmdSeq, record).
pub fn decode_cmd(b: &[u8], dict: &HerdDict) -> Result<(usize, u64, Cmd), CodecError> {
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
        HERD_MATERIALIZE => {
            let n = aux as usize;
            if 64 + MEMBER_BYTES * n > size {
                return Err(CodecError::BadSize);
            }
            let mut members = Vec::with_capacity(n);
            for k in 0..n {
                let o = 64 + MEMBER_BYTES * k;
                members.push(MemberSpawn {
                    member: r.u64(o)?,
                    seed: r.u64(o + 8)?,
                    variant: r.u32(o + 16)?,
                    vanilla_age: r.i32(o + 20)?,
                    age_days: r.u32(o + 24)?,
                    cohort: cohort_of(r.u8(o + 28)?)?,
                    flags: u32::from(r.u8(o + 29)?),
                    x: r.i32(o + 32)?,
                    z: r.i32(o + 36)?,
                });
            }
            Cmd::Materialize(Materialize {
                agent: r.u64(16)?,
                epoch: r.u32(24)?,
                species: dict
                    .species_of(r.u32(28)?)
                    .ok_or(CodecError::BadValue("species"))?,
                dimension: r.u32(32)?,
                flags: r.u32(36)?,
                centre: (r.i32(40)?, r.i32(44)?),
                target: (r.i32(48)?, r.i32(52)?),
                speed: r.f32(56)?,
                radius: r.u32(60)?,
                members,
            })
        }
        HERD_STEER => Cmd::Steer(Steer {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            speed: r.f32(28)?,
            target: (r.i32(32)?, r.i32(36)?),
            arrive: r.i64(40)?,
        }),
        HERD_DESPAWN => Cmd::Despawn(Despawn {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            reason: aux,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs() -> Vec<Obs> {
        let a = 0x48 << 56 | 7;
        vec![
            Obs::GroupState(GroupState {
                agent: a,
                epoch: 3,
                flags: STATE_TICKING,
                dimension: 1,
                present: 30,
                pos: (1.5, 64.0, -9.25),
            }),
            Obs::Materialized(Materialized {
                agent: a,
                epoch: 3,
                dimension: 1,
                spawned: 30,
            }),
            Obs::SpawnRejected(SpawnRejected {
                agent: a,
                epoch: 4,
                reason: 1,
                cmd_seq: 99,
            }),
            Obs::Dematerialized(Dematerialized {
                agent: a,
                epoch: 3,
                reason: 1,
                members: vec![
                    Capture {
                        member: 17,
                        flags: MEMBER_SHEARED,
                        health: 8.0,
                        x: 10,
                        z: -20,
                    },
                    Capture {
                        member: 1 << 40,
                        flags: 0,
                        health: 4.5,
                        x: -3,
                        z: 7,
                    },
                ],
            }),
            Obs::Killed(Killed {
                agent: a,
                epoch: 3,
                member: 17,
                by: KillerKind::Player,
                killer: (1, 2),
                op: 1 << 63 | 5,
            }),
            Obs::Sheared(Sheared {
                agent: a,
                epoch: 3,
                member: 18,
                player: (1, 2),
            }),
            Obs::Taken(Taken {
                agent: a,
                epoch: 3,
                member: 19,
                how: TakenHow::Lead,
                player: (1, 2),
                op: 1 << 63 | 6,
            }),
            Obs::Obstacle(Obstacle {
                agent: a,
                epoch: 3,
                failures: 12,
                present: 30,
                stuck: (100, -450),
                target: (64, -320),
            }),
        ]
    }

    #[test]
    fn observations_round_trip() {
        let mut buf = Vec::new();
        for (i, o) in obs().iter().enumerate() {
            encode_obs(i as u64 + 1, 1_000 + i as i64, o, &mut buf);
            assert_eq!(buf.len() % 8, 0);
        }
        let mut b = &buf[..];
        for (i, o) in obs().into_iter().enumerate() {
            let (size, seq, time, got) = decode_obs(b).unwrap();
            assert_eq!(got, o);
            if o.is_journaled() {
                assert_eq!((seq, time), (i as u64 + 1, 1_000 + i as i64));
            }
            b = &b[size..];
        }
        assert!(b.is_empty());
    }

    #[test]
    fn commands_round_trip() {
        let dict = HerdDict::default();
        let cmds = vec![
            Cmd::Materialize(Materialize {
                agent: 0x48 << 56 | 7,
                epoch: 3,
                species: HerdSpecies::Horse,
                dimension: 1,
                flags: MAT_SURFACE,
                centre: (10, 20),
                target: (300, -40),
                speed: 0.02,
                radius: 90,
                members: (0..5)
                    .map(|k| MemberSpawn {
                        member: k * 3,
                        seed: k.wrapping_mul(0x9E37_79B9),
                        variant: 4 | 2 << 8,
                        vanilla_age: if k == 0 { -12_000 } else { 0 },
                        age_days: k as u32 * 50,
                        cohort: Cohort::ALL[(k % 3) as usize],
                        flags: (k % 2) as u32,
                        x: k as i32,
                        z: -(k as i32),
                    })
                    .collect(),
            }),
            Cmd::Steer(Steer {
                agent: 0x48 << 56 | 7,
                epoch: 3,
                speed: 0.02,
                target: (5, 6),
                arrive: 123_456,
            }),
            Cmd::Despawn(Despawn {
                agent: 0x48 << 56 | 7,
                epoch: 3,
                reason: 1,
            }),
        ];
        let mut buf = Vec::new();
        for (i, c) in cmds.iter().enumerate() {
            encode_cmd(i as u64 + 10, c, &dict, &mut buf);
        }
        let mut b = &buf[..];
        for (i, c) in cmds.into_iter().enumerate() {
            let (size, seq, got) = decode_cmd(b, &dict).unwrap();
            assert_eq!((seq, got), (i as u64 + 10, c));
            b = &b[size..];
        }
    }

    #[test]
    fn short_and_unknown_records_are_errors() {
        let mut buf = Vec::new();
        encode_obs(1, 2, &obs()[4], &mut buf);
        assert_eq!(decode_obs(&buf[..20]), Err(CodecError::BadSize));
        buf[0] = 0x99;
        assert!(matches!(decode_obs(&buf), Err(CodecError::Unknown(_))));
    }
}
