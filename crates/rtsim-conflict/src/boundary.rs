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

pub const TERRITORY: u16 = 0x8800;
pub const FACTION_NOTICE: u16 = 0x8801;
pub const ENVOY_MATERIALIZE: u16 = 0x8802;
pub const BAND_MATERIALIZE: u16 = 0x8803;
pub const BAND_STEER: u16 = 0x8804;
pub const BAND_RECALL: u16 = 0x8805;
pub const DEMAND_SETTLED: u16 = 0x8806;

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
/// BAND_MATERIALIZE member flags.
pub const MEMBER_CHIEF: u32 = 1;
/// BAND_STEER flags.
pub const STEER_ATTACK: u32 = 1;

/// Who did something: the CASUALTIES and CHIEF_KILLED killer kinds.
pub const BY_NONE: u32 = 0;
pub const BY_PLAYER: u32 = 1;
pub const BY_MOB: u32 = 2;
pub const BY_OTHER: u32 = 3;

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
}

impl Obs {
    pub fn kind(&self) -> u16 {
        match self {
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

    /// The plugin's op id, for the records that move items or kill a chief (applied once).
    pub fn op(&self) -> Option<u64> {
        match self {
            Obs::ChiefKilled(c) => Some(c.op),
            Obs::TributePaid(t) => Some(t.op),
            Obs::GiftGiven(g) => Some(g.op),
            Obs::Looted(l) => Some(l.op),
            _ => None,
        }
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

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Territory(Territory),
    Notice(Notice),
    Envoy(EnvoyMaterialize),
    Materialize(BandMaterialize),
    Steer(BandSteer),
    Recall(BandRecall),
    Settled(DemandSettled),
}

impl Cmd {
    pub fn kind(&self) -> u16 {
        match self {
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
        }
    }

    /// The agent a puppet command addresses.
    pub fn agent(&self) -> Option<u64> {
        match self {
            Cmd::Envoy(c) => Some(c.agent),
            Cmd::Materialize(c) => Some(c.agent),
            Cmd::Steer(c) => Some(c.agent),
            Cmd::Recall(c) => Some(c.agent),
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
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.survivors.len() as u32);
            w.pairs(40, &d.survivors);
            w.finish(BAND_DEMATERIALIZED, d.reason, 40 + 8 * d.survivors.len());
        }
        Obs::Casualties(c) => {
            w.u64(24, c.agent);
            w.u32(32, c.epoch);
            w.u32(36, c.body);
            w.u32(40, c.count);
            w.uuid(48, c.killer);
            w.finish(BAND_CASUALTIES, c.by, 64);
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
            w.items(72, &t.items);
            w.finish(TRIBUTE_PAID, t.items.len() as u32, 72 + 16 * t.items.len());
        }
        Obs::GiftGiven(g) => {
            w.u64(24, g.faction);
            w.uuid(32, g.player);
            w.u64(48, g.op);
            w.items(56, &g.items);
            w.finish(GIFT_GIVEN, g.items.len() as u32, 56 + 16 * g.items.len());
        }
        Obs::Looted(l) => {
            w.u64(24, l.agent);
            w.u32(32, l.epoch);
            w.u64(40, l.op);
            w.items(48, &l.items);
            w.finish(BAND_LOOTED, l.items.len() as u32, 48 + 16 * l.items.len());
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
        }),
        BAND_SPAWN_REJECTED => Obs::SpawnRejected(BandSpawnRejected {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            wave: r.u32(36)?,
            reason: aux,
            cmd_seq: r.u64(40)?,
        }),
        BAND_DEMATERIALIZED => {
            let n = r.u32(36)? as usize;
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
            items: r.items(72, aux as usize)?,
        }),
        GIFT_GIVEN => Obs::GiftGiven(GiftGiven {
            faction: r.u64(24)?,
            player: r.uuid(32)?,
            op: r.u64(48)?,
            items: r.items(56, aux as usize)?,
        }),
        BAND_LOOTED => Obs::Looted(BandLooted {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            op: r.u64(40)?,
            items: r.items(48, aux as usize)?,
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
            w.pairs(104, &e.members);
            w.finish(
                ENVOY_MATERIALIZE,
                e.members.len() as u32,
                104 + 8 * e.members.len(),
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
            for (k, s) in m.members.iter().enumerate() {
                w.u32(120 + 16 * k, s.body);
                w.u32(124 + 16 * k, s.count);
                w.u32(128 + 16 * k, s.flags);
            }
            w.finish(
                BAND_MATERIALIZE,
                m.members.len() as u32,
                120 + 16 * m.members.len(),
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
            members: r.pairs(104, aux as usize)?,
        }),
        BAND_MATERIALIZE => {
            let n = aux as usize;
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
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((seq, c, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs() -> Vec<Obs> {
        let items = vec![
            Item { item: 3, count: 12 },
            Item {
                item: 9,
                count: -1,
            },
        ];
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
                items,
            }),
        ]
    }

    fn cmds() -> Vec<Cmd> {
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
                        flags: MEMBER_CHIEF,
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
                outcome: SETTLED_PAID,
                agent: 6,
                player: (11, 22),
                signal: 0,
            }),
        ]
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
        let mut b = vec![0u8; 16];
        b[0..2].copy_from_slice(&0x08FFu16.to_le_bytes());
        b[2..4].copy_from_slice(&16u16.to_le_bytes());
        assert_eq!(decode_obs(&b), Err(CodecError::Unknown(0x08FF)));
        b[2..4].copy_from_slice(&12u16.to_le_bytes());
        assert_eq!(decode_obs(&b), Err(CodecError::BadSize));
    }
}
