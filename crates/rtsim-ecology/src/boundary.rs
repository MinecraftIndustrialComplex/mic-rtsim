// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragon records between the engine and the Ice and Fire plugin (`mic_rtsim_iceandfire`),
//! and their reference codec. `docs/boundary.md` §6 is the contract; this file and that section
//! change together, as `rtsim-industry`'s `wire.rs` and `docs/industry-boundary.md` do.
//!
//! Everything in docs/boundary.md §1 and §2 holds: little-endian, 8-byte header (`kind` u16,
//! `size` u16, `aux` u32), records on 8-byte boundaries with sizes a multiple of 8, reserved
//! fields zero, unknown kinds skipped. Journaled observations carry `obsSeq` at 8 and `gameTime`
//! at 16; commands carry `cmdSeq` at 8.
//!
//! **Kind ranges** (reserved here, docs/boundary.md §6.1): 0x0060 to 0x006F latest-wins dragon
//! observations, 0x0500 to 0x05FF journaled dragon observations, 0x8500 to 0x85FF dragon commands.
//!
//! **Species** travel as body-kind dictionary ids (dictionary kind 3: `iceandfire:fire_dragon`
//! and the others); [`SpeciesDict`] maps them. **Structures** (LAIR_FOUND) use dictionary kind 4.

use crate::dragon::LairKind;
use crate::species::{Gender, Species};

pub const DRAGON_STATE: u16 = 0x0060;
/// A known lair's box is loaded and holds a chest: where its tribute goes (SINK.md §4.6).
pub const LAIR_CHEST: u16 = 0x0061;
pub const DRAGON_MATERIALIZED: u16 = 0x0500;
pub const DRAGON_SPAWN_REJECTED: u16 = 0x0501;
pub const DRAGON_DEMATERIALIZED: u16 = 0x0502;
pub const DRAGON_HURT: u16 = 0x0503;
pub const DRAGON_DIED: u16 = 0x0504;
pub const LAIR_FOUND: u16 = 0x0505;
pub const HOARD_ROBBED: u16 = 0x0506;
pub const DRAGON_UNTAGGED: u16 = 0x0507;
pub const DRAGON_ATE: u16 = 0x0508;
/// Oil chunks the plugin found (OILGATE.md §4.1, OG-2).
pub const OIL_CHUNKS: u16 = 0x0509;
/// An embodied dragon breathed fire (OILGATE.md §4.3, OG-4).
pub const DRAGON_BREATHS: u16 = 0x050A;
/// The oil gate's switches from the plugin's config (OILGATE.md §4.2, §4.8).
pub const OIL_CONFIG: u16 = 0x050B;

pub const DRAGON_MATERIALIZE: u16 = 0x8500;
pub const DRAGON_STEER: u16 = 0x8501;
pub const DRAGON_DEPART: u16 = 0x8502;
pub const DRAGON_ADOPT: u16 = 0x8503;
pub const DRAGON_DISCARD: u16 = 0x8504;
/// Place a fire roost near an oil field with no lair in reach (OILGATE.md §4.4, OG-7).
pub const PLACE_LAIR: u16 = 0x8505;
/// Sample oil chunks around a point, those in memory now and the rest as they load (OILGATE.md
/// §4.1, OG-2, OGC43).
pub const OIL_SURVEY: u16 = 0x8506;
// 0x8507 is reserved for the underground half (OILGATE.md §4.6, OG-9b): placing the deep layer's
// sculk on an oil chunk's first load. Nothing sends it yet.

pub type Uuid = (u64, u64);

/// DRAGON_STATE flags.
pub const STATE_SLEEPING: u32 = 1;
pub const STATE_FLYING: u32 = 2;
pub const STATE_HAS_TARGET: u32 = 4;
pub const STATE_TICKING: u32 = 8;

/// DRAGON_MATERIALIZE flags.
pub const MAT_CLAMP_ABOVE_SURFACE: u32 = 1;
pub const MAT_HAS_HOME: u32 = 2;
pub const MAT_SLEEPING: u32 = 4;
pub const MAT_HOSTILE: u32 = 8;
/// The dragon carries an oil reserve (`Materialize::oil`): the plugin counts its breaths and
/// cancels fire at zero (OILGATE.md §4.3, OG-4, OG-13). Without it the body breathes freely.
pub const MAT_OIL: u32 = 16;

/// OIL_CHUNKS sources.
pub const OIL_SOURCE_DEEP_TIME: u32 = 1;
pub const OIL_SOURCE_CDG: u32 = 2;
pub const OIL_SOURCE_DEPOSIT: u32 = 3;
/// An oil chunk whose amount never runs out (CDG's infinite deposits, a Deep Time trap's charge
/// flagged so).
pub const OIL_BOTTOMLESS: u32 = 1;
/// OIL_CONFIG flags: field flights run (§4.2; off until §4.8 passes).
pub const OIL_FIELD_FLIGHTS: u32 = 1;
/// OIL_CONFIG flags: a field with no fire lair in reach gets one placed (§4.4, OG-7).
pub const OIL_PLACE_LAIRS: u32 = 2;
/// OIL_CHUNKS entries per record at most (the plugin batches so).
pub const OIL_CHUNKS_MAX: usize = 1024;

/// DRAGON_UNTAGGED flags.
pub const UNTAGGED_TAMED: u32 = 1;
pub const UNTAGGED_HAS_HOME: u32 = 2;
pub const UNTAGGED_SLEEPING: u32 = 4;
pub const UNTAGGED_AGING_DISABLED: u32 = 8;

/// Who hurt or killed a dragon (as the bridge's HURT record).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackerKind {
    None = 0,
    Player = 1,
    Mob = 2,
    Other = 3,
}

impl AttackerKind {
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

/// How an untagged dragon came to be (from NeoForge's spawn type where the bridge sees one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    /// Placed by a lair's structure piece at world generation.
    Structure = 1,
    /// A natural spawn rule (vanilla biome spawns, or In Control's).
    Natural = 2,
    /// A spawner (In Control's `spawner.json`, a spawner block).
    Spawner = 3,
    SpawnEgg = 4,
    Command = 5,
    /// Hatched from an egg (tamed by Ice and Fire, so never adopted).
    Hatched = 6,
    Other = 7,
}

impl SpawnKind {
    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Structure,
            2 => Self::Natural,
            3 => Self::Spawner,
            4 => Self::SpawnEgg,
            5 => Self::Command,
            6 => Self::Hatched,
            7 => Self::Other,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub dimension: u32,
    pub health: f32,
    pub max_health: f32,
    pub hunger: i32,
    pub age_ticks: i32,
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
    /// docs/boundary.md §2.3's SPAWN_REJECTED codes, plus 9 unknown species.
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
    pub health: f32,
    pub max_health: f32,
    pub hunger: i32,
    pub age_ticks: i32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hurt {
    pub agent: u64,
    pub epoch: u32,
    pub attacker_kind: AttackerKind,
    pub attacker: Uuid,
    pub hits: u32,
    pub amount: f32,
    pub health_after: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Died {
    pub agent: u64,
    pub epoch: u32,
    pub killer_kind: AttackerKind,
    pub killer: Uuid,
    pub dimension: u32,
    pub pos: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LairFound {
    /// The structure start's id, packed as villages' are (docs/boundary.md §4).
    pub lair: u64,
    pub late: bool,
    pub dimension: u32,
    pub species: Species,
    pub kind: LairKind,
    pub min: (i32, i32, i32),
    pub max: (i32, i32, i32),
    /// Where a dragon of the lair roosts (the structure piece's home position).
    pub home: (i32, i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HoardRobbed {
    pub lair: u64,
    pub op: u64,
    pub player: Uuid,
    /// (item dictionary id, count) taken, summed over the tick.
    pub items: Vec<(u32, i32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Untagged {
    pub entity: Uuid,
    pub spawn: SpawnKind,
    pub species: Species,
    pub dimension: u32,
    pub flags: u32,
    pub age_ticks: i32,
    pub hunger: i32,
    pub gender: Gender,
    pub variant: u32,
    pub home: (i32, i32, i32),
    /// The lair whose box holds it (0: none).
    pub lair: u64,
    pub pos: (f64, f64, f64),
}

/// An embodied dragon killed and ate animals (Ice and Fire's own hunting, `dragon_targets`),
/// summed over a tick. Near a village, the animals were that village's herd: its market stock
/// falls by the same count (P28's recommendation: a watched dragon's consequences go through
/// stock, prices and caravans).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ate {
    pub agent: u64,
    pub epoch: u32,
    /// The prey's entity type (body dictionary, e.g. `minecraft:cow`).
    pub prey: u32,
    pub count: u32,
    /// The village whose bounds hold the kill (its id as VILLAGE_ADOPTED's; 0: none).
    pub village: u64,
    pub pos: (f64, f64, f64),
}

/// One oil chunk (OIL_CHUNKS): its chunk coordinates, the amount the source gives it (mB, CDG's
/// `getBaseOilAmount` or a Deep Time trap's charge) and flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OilChunkRec {
    pub cx: i32,
    pub cz: i32,
    pub amount: i32,
    pub flags: u32,
}

/// Oil chunks the plugin found in one dimension. Idempotent: a chunk sent twice is one chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct OilChunks {
    pub dimension: u32,
    /// `OIL_SOURCE_*`.
    pub source: u32,
    pub chunks: Vec<OilChunkRec>,
}

/// Fire an embodied dragon breathed since its last report (`DragonFireEvent`s, summed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breaths {
    pub agent: u64,
    pub epoch: u32,
    pub count: u32,
}

/// The oil gate's switches (`OIL_FIELD_FLIGHTS`, `OIL_PLACE_LAIRS`), sent at session start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OilConfig {
    pub flags: u32,
}

/// A known lair's chest (latest-wins, docs/boundary.md §6): the plugin found one in the lair's
/// box once its chunks loaded. Sent once a session per lair, and again if the chest is gone and
/// another is found. The engine puts the lair's tribute coin there (TRANSFER_TO_CONTAINER).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LairChest {
    /// The lair, as LAIR_FOUND's.
    pub lair: u64,
    pub dimension: u32,
    pub pos: (i32, i32, i32),
}

/// An observation from the plugin.
#[derive(Clone, Debug, PartialEq)]
pub enum Obs {
    State(State),
    Materialized(Materialized),
    SpawnRejected(SpawnRejected),
    Dematerialized(Dematerialized),
    Hurt(Hurt),
    Died(Died),
    LairFound(LairFound),
    HoardRobbed(HoardRobbed),
    Untagged(Untagged),
    Ate(Ate),
    OilChunks(OilChunks),
    Breaths(Breaths),
    OilConfig(OilConfig),
    LairChest(LairChest),
}

impl Obs {
    pub fn kind(&self) -> u16 {
        match self {
            Obs::State(_) => DRAGON_STATE,
            Obs::LairChest(_) => LAIR_CHEST,
            Obs::Materialized(_) => DRAGON_MATERIALIZED,
            Obs::SpawnRejected(_) => DRAGON_SPAWN_REJECTED,
            Obs::Dematerialized(_) => DRAGON_DEMATERIALIZED,
            Obs::Hurt(_) => DRAGON_HURT,
            Obs::Died(_) => DRAGON_DIED,
            Obs::LairFound(_) => LAIR_FOUND,
            Obs::HoardRobbed(_) => HOARD_ROBBED,
            Obs::Untagged(_) => DRAGON_UNTAGGED,
            Obs::Ate(_) => DRAGON_ATE,
            Obs::OilChunks(_) => OIL_CHUNKS,
            Obs::Breaths(_) => DRAGON_BREATHS,
            Obs::OilConfig(_) => OIL_CONFIG,
        }
    }

    pub fn is_journaled(&self) -> bool {
        self.kind() >= 0x0100
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Materialize {
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub species: Species,
    pub dimension: u32,
    /// Ice and Fire's `AgeTicks`. The plugin also sets `AgingDisabled`: the engine owns age.
    pub age_ticks: i32,
    pub pos: (f64, f64, f64),
    /// Ice and Fire's `Hunger`, 0 to 100.
    pub hunger: i32,
    pub gender: Gender,
    /// Colour index 0 to 3 within the species.
    pub variant: u32,
    /// Absolute health; 0 means full.
    pub health: f32,
    pub home: (i32, i32, i32),
    /// The first flight target (`IafDragonFlightManager.setFlightTarget`).
    pub target: (f64, f64, f64),
    /// A player it attacks on sight (`MAT_HOSTILE`); zero for none.
    pub target_player: Uuid,
    /// Its oil reserve, 0 to the capacity (`MAT_OIL`; 0 without the flag).
    pub oil: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    pub agent: u64,
    pub epoch: u32,
    /// 1: settle at the target.
    pub flags: u32,
    pub target: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Depart {
    pub agent: u64,
    pub epoch: u32,
    /// 1 bored, 2 hungry, 3 wounded, 4 passing through, 5 the engine needs it back.
    pub reason: u32,
    /// The plugin despawns it after this many ticks even if it is still in view.
    pub timeout_ticks: u32,
    pub target: (f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adopt {
    pub entity: Uuid,
    pub agent: u64,
    pub epoch: u32,
    pub flags: u32,
    pub home: (i32, i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Discard {
    pub entity: Uuid,
    /// 1 the sim owns the population (a natural or spawner dragon), 2 its lair already has its
    /// dragon.
    pub reason: u32,
}

/// Place a lair structure near an oil field (`/place structure`'s code path, every biome allowed):
/// on the first load of a free site within `radius` blocks of (x, z). At most one per field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceLair {
    /// The engine's field id, echoed in the LAIR_FOUND the placement makes (logs only).
    pub field: u64,
    pub dimension: u32,
    pub species: Species,
    pub kind: LairKind,
    pub x: i32,
    pub z: i32,
    pub radius: u32,
}

/// Sample CDG's oil for the chunks within `radius` blocks of (x, z) under the plugin's per-tick
/// budget, and answer with OIL_CHUNKS (source 2): chunks in memory now, the others the first time
/// they load this session (a chunk not in memory costs 79 ms to sample; OGC43).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OilSurvey {
    pub dimension: u32,
    pub x: i32,
    pub z: i32,
    pub radius: u32,
}

/// A command to the plugin.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Materialize(Materialize),
    Steer(Steer),
    Depart(Depart),
    Adopt(Adopt),
    Discard(Discard),
    PlaceLair(PlaceLair),
    OilSurvey(OilSurvey),
}

impl Cmd {
    pub fn kind(&self) -> u16 {
        match self {
            Cmd::Materialize(_) => DRAGON_MATERIALIZE,
            Cmd::Steer(_) => DRAGON_STEER,
            Cmd::Depart(_) => DRAGON_DEPART,
            Cmd::Adopt(_) => DRAGON_ADOPT,
            Cmd::Discard(_) => DRAGON_DISCARD,
            Cmd::PlaceLair(_) => PLACE_LAIR,
            Cmd::OilSurvey(_) => OIL_SURVEY,
        }
    }

    pub fn agent(&self) -> Option<u64> {
        match self {
            Cmd::Materialize(m) => Some(m.agent),
            Cmd::Steer(s) => Some(s.agent),
            Cmd::Depart(d) => Some(d.agent),
            Cmd::Adopt(a) => Some(a.agent),
            Cmd::Discard(_) | Cmd::PlaceLair(_) | Cmd::OilSurvey(_) => None,
        }
    }

    /// The record's size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Cmd::Materialize(_) => 136,
            Cmd::Steer(_) | Cmd::Depart(_) => 56,
            Cmd::Adopt(_) => 64,
            Cmd::Discard(_) | Cmd::OilSurvey(_) => 32,
            Cmd::PlaceLair(_) => 48,
        }
    }

    /// Where the record holds a dimension id (the engine's frame, which the link maps to its
    /// own dictionary id), and the frame.
    pub fn dimension(&self) -> Option<(usize, u32)> {
        match self {
            Cmd::Materialize(m) => Some((32, m.dimension)),
            Cmd::PlaceLair(p) => Some((24, p.dimension)),
            Cmd::OilSurvey(s) => Some((16, s.dimension)),
            _ => None,
        }
    }
}

/// Dictionary ids for the three species' entity types and the six lair structures, as the two
/// sides' DICT records define them in a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeciesDict {
    pub body: [u32; 3],
    /// [species][roost, cave].
    pub structure: [[u32; 2]; 3],
}

impl Default for SpeciesDict {
    fn default() -> Self {
        SpeciesDict {
            body: [1, 2, 3],
            structure: [[1, 2], [3, 4], [5, 6]],
        }
    }
}

impl SpeciesDict {
    pub fn body_of(&self, s: Species) -> u32 {
        self.body[s.index()]
    }

    pub fn species_of(&self, id: u32) -> Option<Species> {
        self.body
            .iter()
            .position(|&b| b == id)
            .map(|i| Species::ALL[i])
    }

    pub fn structure_of(&self, s: Species, k: LairKind) -> u32 {
        self.structure[s.index()][matches!(k, LairKind::Cave) as usize]
    }

    pub fn lair_of(&self, id: u32) -> Option<(Species, LairKind)> {
        for (i, pair) in self.structure.iter().enumerate() {
            for (j, &v) in pair.iter().enumerate() {
                if v == id {
                    return Some((
                        Species::ALL[i],
                        if j == 0 {
                            LairKind::Roost
                        } else {
                            LairKind::Cave
                        },
                    ));
                }
            }
        }
        None
    }

    /// The structure ids (verified: `iceandfire:{fire,ice,lightning}_dragon_{roost,cave}`).
    pub fn structure_name(s: Species, k: LairKind) -> String {
        format!(
            "iceandfire:{}_dragon_{}",
            s.name(),
            match k {
                LairKind::Roost => "roost",
                LairKind::Cave => "cave",
            }
        )
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
}

fn gender_u32(g: Gender) -> u32 {
    matches!(g, Gender::Male) as u32
}

fn gender_of(v: u32) -> Result<Gender, CodecError> {
    match v {
        0 => Ok(Gender::Female),
        1 => Ok(Gender::Male),
        _ => Err(CodecError::BadValue("gender")),
    }
}

/// Appends one observation record. `seq` and `game_time` fill the journaled prefix (ignored for
/// latest-wins kinds).
pub fn encode_obs(seq: u64, game_time: i64, o: &Obs, dict: &SpeciesDict, buf: &mut Vec<u8>) {
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
            w.f32(24, s.health);
            w.f32(28, s.max_health);
            w.i32(32, s.hunger);
            w.i32(36, s.age_ticks);
            w.pos(40, s.pos);
            w.finish(DRAGON_STATE, s.flags, 64);
        }
        Obs::Materialized(m) => {
            w.u64(24, m.agent);
            w.u32(32, m.epoch);
            w.u32(36, m.dimension);
            w.uuid(40, m.entity);
            w.finish(DRAGON_MATERIALIZED, 0, 56);
        }
        Obs::SpawnRejected(r) => {
            w.u64(24, r.agent);
            w.u32(32, r.epoch);
            w.u64(40, r.cmd_seq);
            w.finish(DRAGON_SPAWN_REJECTED, r.reason, 48);
        }
        Obs::Dematerialized(d) => {
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.dimension);
            w.f32(40, d.health);
            w.f32(44, d.max_health);
            w.i32(48, d.hunger);
            w.i32(52, d.age_ticks);
            w.pos(56, d.pos);
            w.finish(DRAGON_DEMATERIALIZED, d.reason, 80);
        }
        Obs::Hurt(h) => {
            w.u64(24, h.agent);
            w.u32(32, h.epoch);
            w.u32(36, h.hits);
            w.f32(40, h.amount);
            w.f32(44, h.health_after);
            w.uuid(48, h.attacker);
            w.finish(DRAGON_HURT, h.attacker_kind as u32, 64);
        }
        Obs::Died(d) => {
            w.u64(24, d.agent);
            w.u32(32, d.epoch);
            w.u32(36, d.dimension);
            w.uuid(40, d.killer);
            w.pos(56, d.pos);
            w.finish(DRAGON_DIED, d.killer_kind as u32, 80);
        }
        Obs::LairFound(l) => {
            w.u64(24, l.lair);
            w.u32(32, l.dimension);
            w.u32(36, dict.structure_of(l.species, l.kind));
            w.ipos(40, l.min);
            w.ipos(52, l.max);
            w.ipos(64, l.home);
            w.finish(LAIR_FOUND, l.late as u32, 80);
        }
        Obs::HoardRobbed(h) => {
            w.u64(24, h.lair);
            w.u64(32, h.op);
            w.uuid(40, h.player);
            for (k, &(item, count)) in h.items.iter().enumerate() {
                w.u32(56 + 16 * k, item);
                w.i32(60 + 16 * k, count);
            }
            w.finish(HOARD_ROBBED, h.items.len() as u32, 56 + 16 * h.items.len());
        }
        Obs::Untagged(u) => {
            w.uuid(24, u.entity);
            w.u32(40, dict.body_of(u.species));
            w.u32(44, u.dimension);
            w.u32(48, u.flags);
            w.i32(52, u.age_ticks);
            w.i32(56, u.hunger);
            w.u32(60, gender_u32(u.gender));
            w.u32(64, u.variant);
            w.ipos(68, u.home);
            w.u64(80, u.lair);
            w.pos(88, u.pos);
            w.finish(DRAGON_UNTAGGED, u.spawn as u32, 112);
        }
        Obs::Ate(a) => {
            w.u64(24, a.agent);
            w.u32(32, a.epoch);
            w.u32(36, a.prey);
            w.u64(40, a.village);
            w.pos(48, a.pos);
            w.finish(DRAGON_ATE, a.count, 72);
        }
        Obs::OilChunks(o) => {
            w.u32(24, o.dimension);
            w.u32(28, o.source);
            for (k, c) in o.chunks.iter().enumerate() {
                w.i32(32 + 16 * k, c.cx);
                w.i32(36 + 16 * k, c.cz);
                w.i32(40 + 16 * k, c.amount);
                w.u32(44 + 16 * k, c.flags);
            }
            w.finish(OIL_CHUNKS, o.chunks.len() as u32, 32 + 16 * o.chunks.len());
        }
        Obs::Breaths(b) => {
            w.u64(24, b.agent);
            w.u32(32, b.epoch);
            w.finish(DRAGON_BREATHS, b.count, 40);
        }
        Obs::OilConfig(c) => {
            w.finish(OIL_CONFIG, c.flags, 24);
        }
        Obs::LairChest(c) => {
            w.u64(8, c.lair);
            w.u32(16, c.dimension);
            w.ipos(20, c.pos);
            w.finish(LAIR_CHEST, 0, 32);
        }
    }
}

/// Reads one observation at the start of `b`: (record size, obsSeq, gameTime, record). Seq and
/// time are 0 for latest-wins kinds.
pub fn decode_obs(b: &[u8], dict: &SpeciesDict) -> Result<(usize, u64, i64, Obs), CodecError> {
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
        DRAGON_STATE => Obs::State(State {
            agent: r.u64(8)?,
            epoch: r.u32(16)?,
            flags: aux,
            dimension: r.u32(20)?,
            health: r.f32(24)?,
            max_health: r.f32(28)?,
            hunger: r.i32(32)?,
            age_ticks: r.i32(36)?,
            pos: r.pos(40)?,
        }),
        DRAGON_MATERIALIZED => Obs::Materialized(Materialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            dimension: r.u32(36)?,
            entity: r.uuid(40)?,
        }),
        DRAGON_SPAWN_REJECTED => Obs::SpawnRejected(SpawnRejected {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            reason: aux,
            cmd_seq: r.u64(40)?,
        }),
        DRAGON_DEMATERIALIZED => Obs::Dematerialized(Dematerialized {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            reason: aux,
            dimension: r.u32(36)?,
            health: r.f32(40)?,
            max_health: r.f32(44)?,
            hunger: r.i32(48)?,
            age_ticks: r.i32(52)?,
            pos: r.pos(56)?,
        }),
        DRAGON_HURT => Obs::Hurt(Hurt {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            attacker_kind: AttackerKind::from_u32(aux).ok_or(CodecError::BadValue("attacker"))?,
            attacker: r.uuid(48)?,
            hits: r.u32(36)?,
            amount: r.f32(40)?,
            health_after: r.f32(44)?,
        }),
        DRAGON_DIED => Obs::Died(Died {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            killer_kind: AttackerKind::from_u32(aux).ok_or(CodecError::BadValue("killer"))?,
            killer: r.uuid(40)?,
            dimension: r.u32(36)?,
            pos: r.pos(56)?,
        }),
        LAIR_FOUND => {
            let (species, kind) = dict
                .lair_of(r.u32(36)?)
                .ok_or(CodecError::BadValue("structure"))?;
            Obs::LairFound(LairFound {
                lair: r.u64(24)?,
                late: aux & 1 != 0,
                dimension: r.u32(32)?,
                species,
                kind,
                min: r.ipos(40)?,
                max: r.ipos(52)?,
                home: r.ipos(64)?,
            })
        }
        HOARD_ROBBED => {
            let n = aux as usize;
            if 56 + 16 * n > size {
                return Err(CodecError::BadSize);
            }
            let mut items = Vec::with_capacity(n);
            for k in 0..n {
                items.push((r.u32(56 + 16 * k)?, r.i32(60 + 16 * k)?));
            }
            Obs::HoardRobbed(HoardRobbed {
                lair: r.u64(24)?,
                op: r.u64(32)?,
                player: r.uuid(40)?,
                items,
            })
        }
        DRAGON_UNTAGGED => Obs::Untagged(Untagged {
            entity: r.uuid(24)?,
            spawn: SpawnKind::from_u32(aux).ok_or(CodecError::BadValue("spawn"))?,
            species: dict
                .species_of(r.u32(40)?)
                .ok_or(CodecError::BadValue("species"))?,
            dimension: r.u32(44)?,
            flags: r.u32(48)?,
            age_ticks: r.i32(52)?,
            hunger: r.i32(56)?,
            gender: gender_of(r.u32(60)?)?,
            variant: r.u32(64)?,
            home: r.ipos(68)?,
            lair: r.u64(80)?,
            pos: r.pos(88)?,
        }),
        DRAGON_ATE => Obs::Ate(Ate {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            prey: r.u32(36)?,
            count: aux,
            village: r.u64(40)?,
            pos: r.pos(48)?,
        }),
        OIL_CHUNKS => {
            let n = aux as usize;
            if 32 + 16 * n > size {
                return Err(CodecError::BadSize);
            }
            let mut chunks = Vec::with_capacity(n);
            for k in 0..n {
                chunks.push(OilChunkRec {
                    cx: r.i32(32 + 16 * k)?,
                    cz: r.i32(36 + 16 * k)?,
                    amount: r.i32(40 + 16 * k)?,
                    flags: r.u32(44 + 16 * k)?,
                });
            }
            Obs::OilChunks(OilChunks {
                dimension: r.u32(24)?,
                source: r.u32(28)?,
                chunks,
            })
        }
        DRAGON_BREATHS => Obs::Breaths(Breaths {
            agent: r.u64(24)?,
            epoch: r.u32(32)?,
            count: aux,
        }),
        OIL_CONFIG => {
            if size < 24 {
                return Err(CodecError::BadSize);
            }
            Obs::OilConfig(OilConfig { flags: aux })
        }
        LAIR_CHEST => Obs::LairChest(LairChest {
            lair: r.u64(8)?,
            dimension: r.u32(16)?,
            pos: r.ipos(20)?,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, time, o))
}

/// Appends one command record with sequence `seq`.
pub fn encode_cmd(seq: u64, c: &Cmd, dict: &SpeciesDict, buf: &mut Vec<u8>) {
    let start = buf.len();
    let mut w = W { b: buf, start };
    w.u64(8, seq);
    match c {
        Cmd::Materialize(m) => {
            w.u64(16, m.agent);
            w.u32(24, m.epoch);
            w.u32(28, dict.body_of(m.species));
            w.u32(32, m.dimension);
            w.i32(36, m.age_ticks);
            w.pos(40, m.pos);
            w.i32(64, m.hunger);
            w.u32(68, gender_u32(m.gender));
            w.u32(72, m.variant);
            w.f32(76, m.health);
            w.ipos(80, m.home);
            w.pos(96, m.target);
            w.i32(92, m.oil);
            w.uuid(120, m.target_player);
            w.finish(DRAGON_MATERIALIZE, m.flags, 136);
        }
        Cmd::Steer(s) => {
            w.u64(16, s.agent);
            w.u32(24, s.epoch);
            w.pos(32, s.target);
            w.finish(DRAGON_STEER, s.flags, 56);
        }
        Cmd::Depart(d) => {
            w.u64(16, d.agent);
            w.u32(24, d.epoch);
            w.u32(28, d.timeout_ticks);
            w.pos(32, d.target);
            w.finish(DRAGON_DEPART, d.reason, 56);
        }
        Cmd::Adopt(a) => {
            w.uuid(16, a.entity);
            w.u64(32, a.agent);
            w.u32(40, a.epoch);
            w.u32(44, a.flags);
            w.ipos(48, a.home);
            w.finish(DRAGON_ADOPT, 0, 64);
        }
        Cmd::Discard(d) => {
            w.uuid(16, d.entity);
            w.finish(DRAGON_DISCARD, d.reason, 32);
        }
        Cmd::PlaceLair(p) => {
            w.u64(16, p.field);
            w.u32(24, p.dimension);
            w.u32(28, dict.structure_of(p.species, p.kind));
            w.i32(32, p.x);
            w.i32(36, p.z);
            w.u32(40, p.radius);
            w.finish(PLACE_LAIR, 0, 48);
        }
        Cmd::OilSurvey(o) => {
            w.u32(16, o.dimension);
            w.i32(20, o.x);
            w.i32(24, o.z);
            w.u32(28, o.radius);
            w.finish(OIL_SURVEY, 0, 32);
        }
    }
}

/// Reads one command at the start of `b`: (record size, cmdSeq, command).
pub fn decode_cmd(b: &[u8], dict: &SpeciesDict) -> Result<(usize, u64, Cmd), CodecError> {
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
        DRAGON_MATERIALIZE => Cmd::Materialize(Materialize {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            flags: aux,
            species: dict
                .species_of(r.u32(28)?)
                .ok_or(CodecError::BadValue("species"))?,
            dimension: r.u32(32)?,
            age_ticks: r.i32(36)?,
            pos: r.pos(40)?,
            hunger: r.i32(64)?,
            gender: gender_of(r.u32(68)?)?,
            variant: r.u32(72)?,
            health: r.f32(76)?,
            home: r.ipos(80)?,
            target: r.pos(96)?,
            target_player: r.uuid(120)?,
            oil: r.i32(92)?,
        }),
        DRAGON_STEER => Cmd::Steer(Steer {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            flags: aux,
            target: r.pos(32)?,
        }),
        DRAGON_DEPART => Cmd::Depart(Depart {
            agent: r.u64(16)?,
            epoch: r.u32(24)?,
            reason: aux,
            timeout_ticks: r.u32(28)?,
            target: r.pos(32)?,
        }),
        DRAGON_ADOPT => Cmd::Adopt(Adopt {
            entity: r.uuid(16)?,
            agent: r.u64(32)?,
            epoch: r.u32(40)?,
            flags: r.u32(44)?,
            home: r.ipos(48)?,
        }),
        DRAGON_DISCARD => Cmd::Discard(Discard {
            entity: r.uuid(16)?,
            reason: aux,
        }),
        PLACE_LAIR => {
            let (species, kind) = dict
                .lair_of(r.u32(28)?)
                .ok_or(CodecError::BadValue("structure"))?;
            Cmd::PlaceLair(PlaceLair {
                field: r.u64(16)?,
                dimension: r.u32(24)?,
                species,
                kind,
                x: r.i32(32)?,
                z: r.i32(36)?,
                radius: r.u32(40)?,
            })
        }
        OIL_SURVEY => Cmd::OilSurvey(OilSurvey {
            dimension: r.u32(16)?,
            x: r.i32(20)?,
            z: r.i32(24)?,
            radius: r.u32(28)?,
        }),
        k => return Err(CodecError::Unknown(k)),
    };
    Ok((size, seq, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn all_obs(n: u32) -> Vec<Obs> {
        let p = (1.5, 180.0, -3.25);
        vec![
            Obs::State(State {
                agent: 0x44 << 56 | 7,
                epoch: 3,
                flags: STATE_FLYING | STATE_TICKING,
                dimension: 1,
                health: 120.5,
                max_health: 330.0,
                hunger: 44,
                age_ticks: 1_800_000,
                pos: p,
            }),
            Obs::Materialized(Materialized {
                agent: 9,
                epoch: 1,
                dimension: 1,
                entity: (u64::MAX, 5),
            }),
            Obs::SpawnRejected(SpawnRejected {
                agent: 9,
                epoch: 1,
                reason: 9,
                cmd_seq: 77,
            }),
            Obs::Dematerialized(Dematerialized {
                agent: 9,
                epoch: 2,
                reason: 3,
                dimension: 1,
                health: 99.0,
                max_health: 330.0,
                hunger: 12,
                age_ticks: 3,
                pos: p,
            }),
            Obs::Hurt(Hurt {
                agent: 9,
                epoch: 2,
                attacker_kind: AttackerKind::Player,
                attacker: (1, 2),
                hits: 4,
                amount: 31.0,
                health_after: 68.0,
            }),
            Obs::Died(Died {
                agent: 9,
                epoch: 2,
                killer_kind: AttackerKind::Player,
                killer: (1, 2),
                dimension: 1,
                pos: p,
            }),
            Obs::LairFound(LairFound {
                lair: 1 << 40 | 99,
                late: true,
                dimension: 1,
                species: Species::Ice,
                kind: LairKind::Cave,
                min: (-10, 20, 30),
                max: (40, 60, 80),
                home: (15, 33, 55),
            }),
            Obs::HoardRobbed(HoardRobbed {
                lair: 5,
                op: 1 << 63 | 8,
                player: (3, 4),
                items: (0..n).map(|i| (i + 1, i as i32 * 10 + 1)).collect(),
            }),
            Obs::Untagged(Untagged {
                entity: (8, 9),
                spawn: SpawnKind::Structure,
                species: Species::Lightning,
                dimension: 1,
                flags: UNTAGGED_SLEEPING | UNTAGGED_AGING_DISABLED,
                age_ticks: 960_000,
                hunger: 50,
                gender: Gender::Male,
                variant: 2,
                home: (1, 64, -1),
                lair: 5,
                pos: p,
            }),
            Obs::Ate(Ate {
                agent: 9,
                epoch: 2,
                prey: 40,
                count: 3,
                village: 1 << 50 | 12,
                pos: p,
            }),
            Obs::OilChunks(OilChunks {
                dimension: 1,
                source: OIL_SOURCE_CDG,
                chunks: (0..n)
                    .map(|i| OilChunkRec {
                        cx: i as i32 - 7,
                        cz: -(i as i32),
                        amount: 1_000 * i as i32 + 5,
                        flags: i % 2,
                    })
                    .collect(),
            }),
            Obs::Breaths(Breaths {
                agent: 9,
                epoch: 2,
                count: 17,
            }),
            Obs::OilConfig(OilConfig {
                flags: OIL_FIELD_FLIGHTS | OIL_PLACE_LAIRS,
            }),
            Obs::LairChest(LairChest {
                lair: 1 << 40 | 99,
                dimension: 1,
                pos: (-17, 70, 2_000_001),
            }),
        ]
    }

    fn all_cmds() -> Vec<Cmd> {
        vec![
            Cmd::Materialize(Materialize {
                agent: 9,
                epoch: 4,
                flags: MAT_CLAMP_ABOVE_SURFACE | MAT_HAS_HOME | MAT_HOSTILE,
                species: Species::Fire,
                dimension: 1,
                age_ticks: 2_000_000,
                pos: (100.0, 180.0, -50.0),
                hunger: 40,
                gender: Gender::Female,
                variant: 3,
                health: 0.0,
                home: (5, 70, 9),
                target: (300.0, 120.0, 0.0),
                target_player: (11, 12),
                oil: 63,
            }),
            Cmd::Steer(Steer {
                agent: 9,
                epoch: 4,
                flags: 1,
                target: (1.0, 2.0, 3.0),
            }),
            Cmd::Depart(Depart {
                agent: 9,
                epoch: 4,
                reason: 1,
                timeout_ticks: 1_200,
                target: (5.0, 180.0, 9.0),
            }),
            Cmd::Adopt(Adopt {
                entity: (8, 9),
                agent: 10,
                epoch: 1,
                flags: 1,
                home: (1, 64, -1),
            }),
            Cmd::Discard(Discard {
                entity: (8, 9),
                reason: 1,
            }),
            Cmd::PlaceLair(PlaceLair {
                field: 42,
                dimension: 1,
                species: Species::Fire,
                kind: LairKind::Roost,
                x: -900,
                z: 4_000,
                radius: 100,
            }),
            Cmd::OilSurvey(OilSurvey {
                dimension: 1,
                x: 12,
                z: -34,
                radius: 1_024,
            }),
        ]
    }

    #[test]
    fn every_record_round_trips_and_is_aligned() {
        let dict = SpeciesDict::default();
        let mut buf = Vec::new();
        let obs = all_obs(3);
        for (i, o) in obs.iter().enumerate() {
            encode_obs(100 + i as u64, 5_000, o, &dict, &mut buf);
            assert_eq!(buf.len() % 8, 0);
        }
        let mut at = 0;
        for o in &obs {
            let (size, seq, time, back) = decode_obs(&buf[at..], &dict).unwrap();
            assert_eq!(&back, o);
            if o.is_journaled() {
                assert!(seq >= 100);
                assert_eq!(time, 5_000);
            }
            at += size;
        }
        assert_eq!(at, buf.len());
        let mut buf = Vec::new();
        for (i, c) in all_cmds().iter().enumerate() {
            let before = buf.len();
            encode_cmd(i as u64 + 1, c, &dict, &mut buf);
            assert_eq!(buf.len() % 8, 0);
            assert_eq!(buf.len() - before, c.size(), "{c:?}");
        }
        let mut at = 0;
        for (i, c) in all_cmds().iter().enumerate() {
            let (size, seq, back) = decode_cmd(&buf[at..], &dict).unwrap();
            assert_eq!(&back, c);
            assert_eq!(seq, i as u64 + 1);
            at += size;
        }
        assert_eq!(at, buf.len());
    }

    #[test]
    fn kinds_stay_in_the_reserved_ranges() {
        for o in all_obs(1) {
            let k = o.kind();
            assert!((0x0060..=0x006F).contains(&k) || (0x0500..=0x05FF).contains(&k));
        }
        for c in all_cmds() {
            assert!((0x8500..=0x85FF).contains(&c.kind()));
        }
        assert_eq!(
            SpeciesDict::structure_name(Species::Ice, LairKind::Roost),
            "iceandfire:ice_dragon_roost"
        );
    }

    proptest! {
        #[test]
        fn hoard_records_of_any_length_round_trip(n in 0u32..200) {
            let dict = SpeciesDict::default();
            let o = all_obs(n).remove(7);
            let mut buf = Vec::new();
            encode_obs(1, 2, &o, &dict, &mut buf);
            prop_assert!(buf.len() <= 65_528);
            let (_, _, _, back) = decode_obs(&buf, &dict).unwrap();
            prop_assert_eq!(back, o);
        }

        #[test]
        fn oil_chunk_records_of_any_length_round_trip(n in 0u32..=OIL_CHUNKS_MAX as u32) {
            let dict = SpeciesDict::default();
            let o = all_obs(n).remove(10);
            let mut buf = Vec::new();
            encode_obs(1, 2, &o, &dict, &mut buf);
            prop_assert!(buf.len() <= 65_528);
            let (_, _, _, back) = decode_obs(&buf, &dict).unwrap();
            prop_assert_eq!(back, o);
        }
    }
}
