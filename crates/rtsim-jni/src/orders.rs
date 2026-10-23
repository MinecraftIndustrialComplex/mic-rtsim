// SPDX-License-Identifier: AGPL-3.0-or-later
//! Orders: commands a test or `/rtsim` asks the engine to send, in place of a decision the sim
//! would make. They are what the `StubEngine` calls a script (`ScriptedEngine` on the Java side):
//! the engine writes them as commands, with its own sequences and dictionary, and keeps its
//! bookkeeping (an agent spawning, a transfer in the outbox) as if it had decided them itself.
//!
//! **Encoding** (`NativeEngine.nOrder`, little-endian). One call carries one or more orders and
//! they are one unit: written into the same step's command buffer, or all wait for a later one.
//! Each order is `u16 opcode, u16 reserved, u32 size` (the whole order, header included) and a
//! body; a string is `u32 byte length` and UTF-8.
//!
//! | Opcode | Order | Body |
//! |---|---|---|
//! | 1 | spawn a group | u64 agent, u32 epoch, str groupKind, str dimension, f64 x, f64 y, f64 z, u32 members, then per member: u32 index, f32 health, i32 bodyA, i32 bodyB, u32 items, then per item: u32 slot, i32 count, str item |
//! | 2 | despawn a group | u64 agent, u32 epoch, u32 reason |
//! | 3 | steer a group | u64 agent, u32 epoch, f32 speed, f64 x, f64 y, f64 z |
//! | 4 | transfer to a container | u64 opId (0: the engine allocates one), str dimension, str item, i32 x, i32 y, i32 z, i32 count |
//! | 5 | save at the end of the next step | |
//! | 6 | rewind the command sequence | u64 next |
//! | 7 | fail: the step that writes it panics (TEST mode only; refused otherwise) | |
//! | 8 | a threat for the industry engine (I19), in place of another system's attack | u64 id, u32 source (0 natural, 1 dragon, 2 bandit, 3 boss, 4 synthetic), u32 class (0 to 3), u32 count, u32 target kind (1 district, 2 train), f64 strength, u64 target, u64 at (game time; 0 or past: now) |
//! | 9 | an industry command (docs/industry-boundary.md §4), as if the industry engine had decided it: tests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the entries the record uses, so it never waits for Java's DICT); then the record exactly as `rtsim_industry::wire` encodes it (cmdSeq ignored), its dictionary ids Java's own (GOOD, NAME, BLOCK_STATE and dimension) |
//! | 10 to 15 | reserved | |
//! | 16 | MARKET | u64 village, u64 asOfObsSeq, str currency, i64 treasury, i64 treasuryTarget, i32 worstNum, i32 worstDen, u32 n, then per good: str item, i64 stock, i64 target, i64 anchor, i32 rNum, i32 rDen |
//! | 17 | MERCHANT_OFFERS | u64 agent, u32 epoch, u32 member, u32 n, then per offer: str costA, i32 costACount, str costB ("" none), i32 costBCount, str result, i32 resultCount, i32 maxUses, u32 flags |
//! | 18 | WRITE_GOSSIP | u64 opId, u64 village, u64 playerMost, u64 playerLeast, u32 gossipType, i32 amount, u32 report, u32 flags |
//! | 19 | SENTIMENT | u64 holder, i32 dislike, i32 friend, u32 flags, u32 n, then per entry: u64 most, u64 least, i32 sentiment |
//! | 20 | PLACE_HOUSE | u64 opId, u64 village, str dimension, str template, str processors ("" none), i32 x, i32 y, i32 z, u32 rotation, u32 mirror, u32 flags, u32 radius, u64 seed, i32 yOffset |
//! | 21 | SPAWN_RESIDENT | u64 opId, u64 village, str dimension, str body, str profession ("" none), u32 flags, f64 x, f64 y, f64 z, u64 house |
//! | 22 | a scripted village: the engine applies none of its observations and sends none of its own commands for it | u64 village |
//! | 23 | TEST: materialize the phase 1 caravan with wares nearest (searchX, searchZ) within radius blocks, as this group, at (x, y, z) | u64 agent, u32 epoch, str dimension, f64 x, f64 y, f64 z, u32 radius, f64 searchX, f64 searchZ |
//! | 24 | TEST: the phase 1 world runs this many more ticks ahead of game time | u64 ticks |
//! | 25 | TEST: a dragon command (docs/boundary.md §6.4), as if the dragons had decided it: the Ice and Fire plugin's GameTests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the species entries the record uses); then the record exactly as `rtsim_ecology::boundary` encodes it (cmdSeq ignored), its species ids Java's own (body dictionary) and its dimension Java's id (the dragons' frame) |
//! | 26 | TEST: a ship command (docs/boundary.md §7.4), as if the ships had decided it: a future ships plugin's GameTests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the body kinds and items the record uses); then the record exactly as `rtsim_ships::boundary` encodes it (cmdSeq ignored), its body and item ids Java's own (body and item dictionaries) and its dimension Java's id (the ships' frame); a SHIP_CARGO with op id 0 gets one from its identity |
//! | 27 | TEST: a herd command (docs/boundary.md §8.4), as if the herds had decided it: the vanilla herd adapter's GameTests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the species entries the record uses); then the record exactly as `rtsim_ecology::herds::boundary` encodes it (cmdSeq ignored), its species ids Java's own (body dictionary) and its dimension Java's id (the herds' frame) |
//! | 28 | TEST: found a herd (nothing in the game founds one yet: no habitat provider), applied when the engine takes the unit | str name, str species (`minecraft:sheep` and the rest), u32 dimension (Java's id: the herds' frame), i32 x, i32 z, u64 count, u64 seed (0: the herds' own) |
//! | 29 | TEST: a conflict command (docs/boundary.md §9.4), as if the conflict had decided it: a future conflict plugin's GameTests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the body kinds and items the record uses); then the record exactly as `rtsim_conflict::boundary` encodes it (cmdSeq ignored), its body and item ids Java's own (body and item dictionaries), its dimension Java's id (the conflict's frame) and its ticks game time |
//! | 30 to 39 | 30, 31, 32 and 34 are the works, tribute and nest test orders (see `Order`); the rest are free | |
//! | 40 | TEST: post a report to a phase 1 village's noticeboard (TALK, docs/boundary.md §13), as if the village had witnessed it: slice 2b's GameTest `talk_tells_the_village_news` puts news on a board with it; refused if the village is unknown or an actor does not resolve | u64 village, u16 kind (rtsim-knowledge's `ReportKind` id), u8 severity, u8 flags, then who and whom each as u8 class, u64 a, u64 b (the TALK_FACTS ref classes: 0 none, 1 player a b the UUID halves, 2 village and 3 base a Java's id, 4 caravan a its seed, 5 camp a its faction key, 6 and 7 a notable's seed, 8 lair a its key, 9 haven a its node key), then i32 x, i32 z (in the village's frame), u32 ticks ago |

use rtsim_industry::ids::{DistrictId, TrainId};
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::Tick;

use crate::wire::{f32_at, f64_at, i32_at, u16_at, u32_at, u64_at};

pub const OP_SPAWN_GROUP: u16 = 1;
pub const OP_DESPAWN_GROUP: u16 = 2;
pub const OP_STEER_GROUP: u16 = 3;
pub const OP_TRANSFER_TO_CONTAINER: u16 = 4;
pub const OP_SAVE_AT_NEXT_STEP: u16 = 5;
pub const OP_REWIND_CMD_SEQ: u16 = 6;
pub const OP_FAIL: u16 = 7;
pub const OP_INDUSTRY_THREAT: u16 = 8;
pub const OP_INDUSTRY_COMMAND: u16 = 9;

const SOURCES: [ThreatSource; 6] = [
    ThreatSource::Natural,
    ThreatSource::Dragon,
    ThreatSource::Bandit,
    ThreatSource::Boss,
    ThreatSource::Synthetic,
    ThreatSource::Nest,
];
pub const OP_MARKET: u16 = 16;
pub const OP_MERCHANT_OFFERS: u16 = 17;
pub const OP_WRITE_GOSSIP: u16 = 18;
pub const OP_SENTIMENT: u16 = 19;
pub const OP_PLACE_HOUSE: u16 = 20;
pub const OP_SPAWN_RESIDENT: u16 = 21;
pub const OP_SCRIPT_VILLAGE: u16 = 22;
pub const OP_P1_SPAWN_NEAR: u16 = 23;
pub const OP_P1_ADVANCE: u16 = 24;
pub const OP_DRAGON_COMMAND: u16 = 25;
pub const OP_SHIP_COMMAND: u16 = 26;
pub const OP_HERD_COMMAND: u16 = 27;
pub const OP_HERD_FOUND: u16 = 28;
pub const OP_CONFLICT_COMMAND: u16 = 29;
pub const OP_WORKS_GRANT: u16 = 30;
pub const OP_WORKS_RAID: u16 = 31;
pub const OP_TRIBUTE_START: u16 = 32;
pub const OP_NEST_DAY: u16 = 34;
/// TALK's test order (30 to 39 are left free for the orders other branches take).
pub const OP_TALK_REPORT: u16 = 40;

/// One good of a MARKET (docs/boundary.md §4.2), in rtsim-trade's units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketGood {
    pub item: String,
    pub stock: i64,
    pub target: i64,
    pub anchor: i64,
    pub r_num: i32,
    pub r_den: i32,
}

/// A MARKET command: a village market's snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketCmd {
    pub village: u64,
    pub as_of: u64,
    pub currency: String,
    pub treasury: i64,
    pub treasury_target: i64,
    pub worst_num: i32,
    pub worst_den: i32,
    pub goods: Vec<MarketGood>,
}

/// One offer of a MERCHANT_OFFERS. An empty `cost_b` is none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferCmd {
    pub cost_a: String,
    pub cost_a_count: i32,
    pub cost_b: String,
    pub cost_b_count: i32,
    pub result: String,
    pub result_count: i32,
    pub max_uses: i32,
    pub flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffersCmd {
    pub agent: u64,
    pub epoch: u32,
    pub member: u32,
    pub offers: Vec<OfferCmd>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GossipCmd {
    pub op: u64,
    pub village: u64,
    pub player: (u64, u64),
    pub gossip: u32,
    pub amount: i32,
    pub report: u32,
    pub flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentimentCmd {
    pub holder: u64,
    pub dislike: i32,
    pub friend: i32,
    pub flags: u32,
    pub entries: Vec<(u64, u64, i32)>,
}

/// A PLACE_HOUSE. An empty `processors` is none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceHouseCmd {
    pub op: u64,
    pub village: u64,
    pub dimension: String,
    pub template: String,
    pub processors: String,
    pub pos: [i32; 3],
    pub rotation: u32,
    pub mirror: u32,
    pub flags: u32,
    pub radius: u32,
    pub seed: u64,
    pub y_offset: i32,
}

/// A SPAWN_RESIDENT. An empty `profession` is the body's default.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnResidentCmd {
    pub op: u64,
    pub village: u64,
    pub dimension: String,
    pub body: String,
    pub profession: String,
    pub flags: u32,
    pub pos: [f64; 3],
    pub house: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemOrder {
    pub slot: u32,
    pub count: i32,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemberOrder {
    pub index: u32,
    pub health: f32,
    pub body_a: i32,
    pub body_b: i32,
    pub items: Vec<ItemOrder>,
}

/// An actor an order names, as a TALK_FACTS ref does: a class and the two ids of that class
/// (docs/boundary.md §13.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TalkActor {
    pub class: u8,
    pub a: u64,
    pub b: u64,
}

/// TEST: a report put on a village's noticeboard (opcode 40).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TalkReportCmd {
    /// The village (Java's id) whose board it goes on.
    pub village: u64,
    /// rtsim-knowledge's `ReportKind` id.
    pub kind: u16,
    pub severity: u8,
    pub flags: u8,
    pub who: TalkActor,
    pub whom: TalkActor,
    /// The place, in the village's frame.
    pub x: i32,
    pub z: i32,
    /// How long ago it happened, game ticks.
    pub ago: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Order {
    SpawnGroup {
        agent: u64,
        epoch: u32,
        group_kind: String,
        dimension: String,
        pos: [f64; 3],
        members: Vec<MemberOrder>,
    },
    DespawnGroup {
        agent: u64,
        epoch: u32,
        reason: u32,
    },
    SteerGroup {
        agent: u64,
        epoch: u32,
        speed: f32,
        pos: [f64; 3],
    },
    TransferToContainer {
        op: u64,
        dimension: String,
        item: String,
        pos: [i32; 3],
        count: i32,
    },
    SaveAtNextStep,
    RewindCmdSeq(u64),
    /// Tests: the step that writes it panics, to show what an engine failure does.
    Fail,
    Market(MarketCmd),
    MerchantOffers(OffersCmd),
    WriteGossip(GossipCmd),
    Sentiment(SentimentCmd),
    PlaceHouse(PlaceHouseCmd),
    SpawnResident(SpawnResidentCmd),
    /// A test scripts this village's phase 1 commands (applied at once, never written).
    ScriptVillage(u64),
    /// TEST: materialize the phase 1 caravan with wares nearest `search` (x, z) within `radius`
    /// blocks, as this group, at `pos` (applied at the next step, never written).
    Phase1SpawnNear {
        agent: u64,
        epoch: u32,
        dimension: String,
        pos: [f64; 3],
        radius: u32,
        search: [f64; 2],
    },
    /// TEST: the phase 1 world runs this many more ticks ahead of game time.
    Phase1Advance(u64),
    /// An attack for the industry engine (I19), applied at the start of the next step: what the
    /// dragons, conflict and kaiju modules will hand it.
    IndustryThreat(Threat),
    /// An industry command record (Java's dictionary ids), written as the industry engine's own:
    /// the link's queue, sequences, DICT records and outbox (tests; W17). `names` are the Java
    /// dictionary entries it uses: (kind, Java's id, name).
    IndustryCommand {
        names: Vec<(u32, u32, String)>,
        record: Vec<u8>,
    },
    /// TEST: a dragon command record (Java's species and dimension ids), queued as the dragons'
    /// own: their sequences and DICT records (the Ice and Fire plugin's GameTests). `names` are the
    /// Java dictionary entries for the species: (kind, Java's id, name).
    DragonCommand {
        names: Vec<(u32, u32, String)>,
        record: Vec<u8>,
    },
    /// TEST: a ship command record (Java's body, item and dimension ids), queued as the ships'
    /// own: their sequences and DICT records (a future ships plugin's GameTests). `names` are the
    /// Java dictionary entries it uses: (kind, Java's id, name).
    ShipCommand {
        names: Vec<(u32, u32, String)>,
        record: Vec<u8>,
    },
    /// TEST: a herd command record (Java's species and dimension ids), queued as the herds' own:
    /// their sequences and DICT records (the vanilla herd adapter's GameTests). `names` are the
    /// Java dictionary entries for the species: (kind, Java's id, name).
    HerdCommand {
        names: Vec<(u32, u32, String)>,
        record: Vec<u8>,
    },
    /// TEST: a herd founded at (x, z) in a dimension (Java's id, the herds' frame) when the
    /// engine takes the unit. Nothing in the game founds herds yet (no habitat provider).
    HerdFound {
        name: String,
        species: String,
        dimension: u32,
        x: i32,
        z: i32,
        count: u64,
        /// 0: drawn from the herds' own seed.
        seed: u64,
    },
    /// TEST: a conflict command record (Java's body, item and dimension ids, game time), queued
    /// as the conflict's own: its sequences and DICT records (a future conflict plugin's
    /// GameTests). `names` are the Java dictionary entries it uses: (kind, Java's id, name).
    ConflictCommand {
        names: Vec<(u32, u32, String)>,
        record: Vec<u8>,
    },
    /// TEST: coin (thousandths of the village's own currency) put straight into a phase 1
    /// village's works purse (SINK.md), ledgered as minted; applied at the next step.
    WorksGrant {
        village: u64,
        amount: i64,
    },
    /// TEST: a dragon raided a phase 1 village (SINK.md §4.6): golems first for a season, and
    /// with `tribute` its purse is the dragon's; applied at the next step.
    WorksRaid {
        village: u64,
        tribute: bool,
    },
    /// TEST: phase 1 village `village` becomes the tributary of the dragons' lair `lair` (keys),
    /// as if a dragon of the lair had raided it (SINK.md §4.6); applied at the next step.
    TributeStart {
        village: u64,
        lair: u64,
    },
    /// TEST: the oil nests' day (their watch period and the unit of their waits, OILGATE.md §4.6)
    /// is this many ticks from now on; 0 restores the tunable's. Applied at the next step.
    NestDay(u64),
    /// TEST: a report on a phase 1 village's noticeboard, as if the village had witnessed it
    /// (TALK; docs/boundary.md §13.4). Applied when the engine takes the unit.
    TalkReport(TalkReportCmd),
}

impl Order {
    /// Applied when the engine takes the unit (step start), never written as a command.
    pub fn is_directive(&self) -> bool {
        matches!(
            self,
            Order::SaveAtNextStep
                | Order::ScriptVillage(_)
                | Order::Phase1SpawnNear { .. }
                | Order::Phase1Advance(_)
                | Order::HerdFound { .. }
                | Order::WorksGrant { .. }
                | Order::WorksRaid { .. }
                | Order::TributeStart { .. }
                | Order::NestDay(_)
        )
    }

    /// Needs an engine opened in TEST mode.
    pub fn test_only(&self) -> bool {
        matches!(
            self,
            Order::Fail
                | Order::Phase1SpawnNear { .. }
                | Order::Phase1Advance(_)
                | Order::DragonCommand { .. }
                | Order::ShipCommand { .. }
                | Order::HerdCommand { .. }
                | Order::HerdFound { .. }
                | Order::ConflictCommand { .. }
                | Order::WorksGrant { .. }
                | Order::WorksRaid { .. }
                | Order::TributeStart { .. }
                | Order::NestDay(_)
                | Order::TalkReport(_)
        )
    }
}

/// A herd species by its vanilla entity id (docs/boundary.md §8.2).
pub fn herd_species(name: &str) -> Option<rtsim_ecology::herds::HerdSpecies> {
    rtsim_ecology::herds::HerdSpecies::ALL
        .into_iter()
        .find(|s| s.entity_id() == name)
}

/// The herd species' body ids a herd command order's names give (Java's dictionary); `None` when
/// they name no herd species. Species the names leave out get id 0, which matches nothing.
pub fn herd_species_dict(
    names: &[(u32, u32, String)],
) -> Option<rtsim_ecology::herds::boundary::HerdDict> {
    let mut d = rtsim_ecology::herds::boundary::HerdDict { body: [0; 5] };
    let mut any = false;
    for (kind, id, name) in names {
        if *kind == crate::wire::DICT_BODY
            && let Some(s) = herd_species(name)
        {
            d.body[s.index()] = *id;
            any = true;
        }
    }
    any.then_some(d)
}

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn need(&self, n: usize) -> Result<(), String> {
        if self.at + n > self.b.len() {
            Err(format!(
                "order truncated at byte {} (wanted {n} more)",
                self.at
            ))
        } else {
            Ok(())
        }
    }

    fn u8(&mut self) -> Result<u8, String> {
        self.need(1)?;
        let v = self.b[self.at];
        self.at += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, String> {
        self.need(2)?;
        let v = u16_at(self.b, self.at);
        self.at += 2;
        Ok(v)
    }

    fn talk_actor(&mut self) -> Result<TalkActor, String> {
        Ok(TalkActor {
            class: self.u8()?,
            a: self.u64()?,
            b: self.u64()?,
        })
    }

    fn u32(&mut self) -> Result<u32, String> {
        self.need(4)?;
        let v = u32_at(self.b, self.at);
        self.at += 4;
        Ok(v)
    }

    fn i32(&mut self) -> Result<i32, String> {
        self.need(4)?;
        let v = i32_at(self.b, self.at);
        self.at += 4;
        Ok(v)
    }

    fn u64(&mut self) -> Result<u64, String> {
        self.need(8)?;
        let v = u64_at(self.b, self.at);
        self.at += 8;
        Ok(v)
    }

    fn i64(&mut self) -> Result<i64, String> {
        Ok(self.u64()? as i64)
    }

    fn count(&mut self, max: usize, what: &str) -> Result<usize, String> {
        let n = self.u32()? as usize;
        if n > max {
            return Err(format!("{n} {what} in one order (at most {max})"));
        }
        Ok(n)
    }

    fn f32(&mut self) -> Result<f32, String> {
        self.need(4)?;
        let v = f32_at(self.b, self.at);
        self.at += 4;
        Ok(v)
    }

    fn f64(&mut self) -> Result<f64, String> {
        self.need(8)?;
        let v = f64_at(self.b, self.at);
        self.at += 8;
        Ok(v)
    }

    fn str(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        self.need(n)?;
        let s = std::str::from_utf8(&self.b[self.at..self.at + n])
            .map_err(|e| format!("order string: {e}"))?;
        self.at += n;
        Ok(s.to_string())
    }

    fn pos(&mut self) -> Result<[f64; 3], String> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }
}

/// Decodes one call's orders (one unit).
pub fn decode(bytes: &[u8]) -> Result<Vec<Order>, String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            return Err(format!("order header truncated at byte {at}"));
        }
        let opcode = u16_at(bytes, at);
        let size = u32_at(bytes, at + 4) as usize;
        if size < 8 || at + size > bytes.len() {
            return Err(format!("order {opcode} at byte {at} has a bad size {size}"));
        }
        let body = &bytes[at + 8..at + size];
        let mut c = Cursor { b: body, at: 0 };
        let order = match opcode {
            OP_SPAWN_GROUP => {
                let agent = c.u64()?;
                let epoch = c.u32()?;
                let group_kind = c.str()?;
                let dimension = c.str()?;
                let pos = c.pos()?;
                let n = c.u32()? as usize;
                if n > 4096 {
                    return Err(format!("a group of {n} members"));
                }
                let mut members = Vec::with_capacity(n);
                for _ in 0..n {
                    let index = c.u32()?;
                    let health = c.f32()?;
                    let body_a = c.i32()?;
                    let body_b = c.i32()?;
                    let k = c.u32()? as usize;
                    if k > 4096 {
                        return Err(format!("a member with {k} item stacks"));
                    }
                    let mut items = Vec::with_capacity(k);
                    for _ in 0..k {
                        let slot = c.u32()?;
                        let count = c.i32()?;
                        let item = c.str()?;
                        items.push(ItemOrder { slot, count, item });
                    }
                    members.push(MemberOrder {
                        index,
                        health,
                        body_a,
                        body_b,
                        items,
                    });
                }
                Order::SpawnGroup {
                    agent,
                    epoch,
                    group_kind,
                    dimension,
                    pos,
                    members,
                }
            }
            OP_DESPAWN_GROUP => Order::DespawnGroup {
                agent: c.u64()?,
                epoch: c.u32()?,
                reason: c.u32()?,
            },
            OP_STEER_GROUP => Order::SteerGroup {
                agent: c.u64()?,
                epoch: c.u32()?,
                speed: c.f32()?,
                pos: c.pos()?,
            },
            OP_TRANSFER_TO_CONTAINER => {
                let op = c.u64()?;
                let dimension = c.str()?;
                let item = c.str()?;
                let pos = [c.i32()?, c.i32()?, c.i32()?];
                let count = c.i32()?;
                Order::TransferToContainer {
                    op,
                    dimension,
                    item,
                    pos,
                    count,
                }
            }
            OP_SAVE_AT_NEXT_STEP => Order::SaveAtNextStep,
            OP_REWIND_CMD_SEQ => Order::RewindCmdSeq(c.u64()?),
            OP_FAIL => Order::Fail,
            OP_MARKET => {
                let village = c.u64()?;
                let as_of = c.u64()?;
                let currency = c.str()?;
                let treasury = c.i64()?;
                let treasury_target = c.i64()?;
                let worst_num = c.i32()?;
                let worst_den = c.i32()?;
                let n = c.count(1024, "goods")?;
                let mut goods = Vec::with_capacity(n);
                for _ in 0..n {
                    goods.push(MarketGood {
                        item: c.str()?,
                        stock: c.i64()?,
                        target: c.i64()?,
                        anchor: c.i64()?,
                        r_num: c.i32()?,
                        r_den: c.i32()?,
                    });
                }
                Order::Market(MarketCmd {
                    village,
                    as_of,
                    currency,
                    treasury,
                    treasury_target,
                    worst_num,
                    worst_den,
                    goods,
                })
            }
            OP_MERCHANT_OFFERS => {
                let agent = c.u64()?;
                let epoch = c.u32()?;
                let member = c.u32()?;
                let n = c.count(1024, "offers")?;
                let mut offers = Vec::with_capacity(n);
                for _ in 0..n {
                    offers.push(OfferCmd {
                        cost_a: c.str()?,
                        cost_a_count: c.i32()?,
                        cost_b: c.str()?,
                        cost_b_count: c.i32()?,
                        result: c.str()?,
                        result_count: c.i32()?,
                        max_uses: c.i32()?,
                        flags: c.u32()?,
                    });
                }
                Order::MerchantOffers(OffersCmd {
                    agent,
                    epoch,
                    member,
                    offers,
                })
            }
            OP_WRITE_GOSSIP => Order::WriteGossip(GossipCmd {
                op: c.u64()?,
                village: c.u64()?,
                player: (c.u64()?, c.u64()?),
                gossip: c.u32()?,
                amount: c.i32()?,
                report: c.u32()?,
                flags: c.u32()?,
            }),
            OP_SENTIMENT => {
                let holder = c.u64()?;
                let dislike = c.i32()?;
                let friend = c.i32()?;
                let flags = c.u32()?;
                let n = c.count(2048, "sentiment entries")?;
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    entries.push((c.u64()?, c.u64()?, c.i32()?));
                }
                Order::Sentiment(SentimentCmd {
                    holder,
                    dislike,
                    friend,
                    flags,
                    entries,
                })
            }
            OP_PLACE_HOUSE => Order::PlaceHouse(PlaceHouseCmd {
                op: c.u64()?,
                village: c.u64()?,
                dimension: c.str()?,
                template: c.str()?,
                processors: c.str()?,
                pos: [c.i32()?, c.i32()?, c.i32()?],
                rotation: c.u32()?,
                mirror: c.u32()?,
                flags: c.u32()?,
                radius: c.u32()?,
                seed: c.u64()?,
                y_offset: c.i32()?,
            }),
            OP_SPAWN_RESIDENT => Order::SpawnResident(SpawnResidentCmd {
                op: c.u64()?,
                village: c.u64()?,
                dimension: c.str()?,
                body: c.str()?,
                profession: c.str()?,
                flags: c.u32()?,
                pos: c.pos()?,
                house: c.u64()?,
            }),
            OP_SCRIPT_VILLAGE => Order::ScriptVillage(c.u64()?),
            OP_P1_SPAWN_NEAR => Order::Phase1SpawnNear {
                agent: c.u64()?,
                epoch: c.u32()?,
                dimension: c.str()?,
                pos: c.pos()?,
                radius: c.u32()?,
                search: [c.f64()?, c.f64()?],
            },
            OP_P1_ADVANCE => Order::Phase1Advance(c.u64()?),
            OP_WORKS_GRANT => Order::WorksGrant {
                village: c.u64()?,
                amount: c.u64()? as i64,
            },
            OP_WORKS_RAID => Order::WorksRaid {
                village: c.u64()?,
                tribute: c.u32()? != 0,
            },
            OP_TRIBUTE_START => Order::TributeStart {
                village: c.u64()?,
                lair: c.u64()?,
            },
            OP_NEST_DAY => Order::NestDay(c.u64()?),
            OP_TALK_REPORT => Order::TalkReport(TalkReportCmd {
                village: c.u64()?,
                kind: c.u16()?,
                severity: c.u8()?,
                flags: c.u8()?,
                who: c.talk_actor()?,
                whom: c.talk_actor()?,
                x: c.i32()?,
                z: c.i32()?,
                ago: c.u32()?,
            }),
            OP_INDUSTRY_THREAT => {
                let id = c.u64()?;
                let source = *SOURCES
                    .get(c.u32()? as usize)
                    .ok_or("a threat source out of range")?;
                let class = *ThreatClass::ALL
                    .get(c.u32()? as usize)
                    .ok_or("a threat class out of range")?;
                let count = c.u32()?;
                let target_kind = c.u32()?;
                let strength = c.f64()?;
                let target = c.u64()?;
                let at = Tick(c.u64()?);
                let target = match target_kind {
                    1 => ThreatTarget::District(DistrictId(target)),
                    2 => ThreatTarget::Train(TrainId(target)),
                    k => return Err(format!("threat target kind {k}")),
                };
                Order::IndustryThreat(Threat {
                    id,
                    source,
                    class,
                    count,
                    strength,
                    target,
                    loads: None,
                    at,
                })
            }
            OP_INDUSTRY_COMMAND => {
                let n = c.u32()? as usize;
                if n > 4096 {
                    return Err(format!("an industry command with {n} names"));
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    let kind = c.u32()?;
                    let id = c.u32()?;
                    names.push((kind, id, c.str()?));
                }
                let record = body[c.at..].to_vec();
                c.at = body.len();
                if rtsim_industry::wire::decode_cmd(&record)
                    .map_err(|e| format!("an industry command order: {e}"))?
                    .is_none()
                {
                    return Err("an industry command order holds no industry command".into());
                }
                Order::IndustryCommand { names, record }
            }
            OP_DRAGON_COMMAND => {
                let n = c.u32()? as usize;
                if n > 64 {
                    return Err(format!("a dragon command with {n} names"));
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    let kind = c.u32()?;
                    let id = c.u32()?;
                    names.push((kind, id, c.str()?));
                }
                let record = body[c.at..].to_vec();
                c.at = body.len();
                let kind = if record.len() >= 16 {
                    u16_at(&record, 0)
                } else {
                    0
                };
                if !(0x8500..=0x85FF).contains(&kind) {
                    return Err("a dragon command order holds no dragon command".into());
                }
                Order::DragonCommand { names, record }
            }
            OP_SHIP_COMMAND => {
                let n = c.u32()? as usize;
                if n > 1024 {
                    return Err(format!("a ship command with {n} names"));
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    let kind = c.u32()?;
                    let id = c.u32()?;
                    names.push((kind, id, c.str()?));
                }
                let record = body[c.at..].to_vec();
                c.at = body.len();
                if rtsim_ships::boundary::decode_cmd(&record).is_err() {
                    return Err("a ship command order holds no ship command".into());
                }
                Order::ShipCommand { names, record }
            }
            OP_HERD_COMMAND => {
                let n = c.u32()? as usize;
                if n > 64 {
                    return Err(format!("a herd command with {n} names"));
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    let kind = c.u32()?;
                    let id = c.u32()?;
                    names.push((kind, id, c.str()?));
                }
                let record = body[c.at..].to_vec();
                c.at = body.len();
                if herd_species_dict(&names).is_none_or(|d| {
                    rtsim_ecology::herds::boundary::decode_cmd(&record, &d).is_err()
                }) {
                    return Err("a herd command order holds no herd command".into());
                }
                Order::HerdCommand { names, record }
            }
            OP_HERD_FOUND => {
                let name = c.str()?;
                let species = c.str()?;
                if herd_species(&species).is_none() {
                    return Err(format!("a herd of {species}: not a herd species"));
                }
                let dimension = c.u32()?;
                let x = c.i32()?;
                let z = c.i32()?;
                let count = c.u64()?;
                if count == 0 || count > 1_000_000 {
                    return Err(format!("a herd of {count} animals"));
                }
                let seed = c.u64()?;
                Order::HerdFound {
                    name,
                    species,
                    dimension,
                    x,
                    z,
                    count,
                    seed,
                }
            }
            OP_CONFLICT_COMMAND => {
                let n = c.u32()? as usize;
                if n > 1024 {
                    return Err(format!("a conflict command with {n} names"));
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    let kind = c.u32()?;
                    let id = c.u32()?;
                    names.push((kind, id, c.str()?));
                }
                let record = body[c.at..].to_vec();
                c.at = body.len();
                if rtsim_conflict::boundary::decode_cmd(&record).is_err() {
                    return Err("a conflict command order holds no conflict command".into());
                }
                Order::ConflictCommand { names, record }
            }
            other => return Err(format!("unknown order opcode {other}")),
        };
        if c.at != body.len() {
            return Err(format!(
                "order {opcode} has {} trailing bytes",
                body.len() - c.at
            ));
        }
        out.push(order);
        at += size;
    }
    Ok(out)
}

/// The encoder, for the Rust tests (Java has its own).
#[derive(Default)]
pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    fn begin(&mut self, opcode: u16) -> usize {
        let at = self.buf.len();
        self.buf.extend_from_slice(&opcode.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u32.to_le_bytes());
        at
    }

    fn end(&mut self, at: usize) {
        let size = (self.buf.len() - at) as u32;
        self.buf[at + 4..at + 8].copy_from_slice(&size.to_le_bytes());
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn talk_actor(&mut self, a: &TalkActor) {
        self.u8(a.class);
        self.u64(a.a);
        self.u64(a.b);
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
    }

    pub fn order(&mut self, o: &Order) -> &mut Self {
        match o {
            Order::SpawnGroup {
                agent,
                epoch,
                group_kind,
                dimension,
                pos,
                members,
            } => {
                let at = self.begin(OP_SPAWN_GROUP);
                self.u64(*agent);
                self.u32(*epoch);
                self.str(group_kind);
                self.str(dimension);
                pos.iter().for_each(|v| self.f64(*v));
                self.u32(members.len() as u32);
                for m in members {
                    self.u32(m.index);
                    self.f32(m.health);
                    self.i32(m.body_a);
                    self.i32(m.body_b);
                    self.u32(m.items.len() as u32);
                    for it in &m.items {
                        self.u32(it.slot);
                        self.i32(it.count);
                        self.str(&it.item);
                    }
                }
                self.end(at);
            }
            Order::DespawnGroup {
                agent,
                epoch,
                reason,
            } => {
                let at = self.begin(OP_DESPAWN_GROUP);
                self.u64(*agent);
                self.u32(*epoch);
                self.u32(*reason);
                self.end(at);
            }
            Order::SteerGroup {
                agent,
                epoch,
                speed,
                pos,
            } => {
                let at = self.begin(OP_STEER_GROUP);
                self.u64(*agent);
                self.u32(*epoch);
                self.f32(*speed);
                pos.iter().for_each(|v| self.f64(*v));
                self.end(at);
            }
            Order::TransferToContainer {
                op,
                dimension,
                item,
                pos,
                count,
            } => {
                let at = self.begin(OP_TRANSFER_TO_CONTAINER);
                self.u64(*op);
                self.str(dimension);
                self.str(item);
                pos.iter().for_each(|v| self.i32(*v));
                self.i32(*count);
                self.end(at);
            }
            Order::SaveAtNextStep => {
                let at = self.begin(OP_SAVE_AT_NEXT_STEP);
                self.end(at);
            }
            Order::RewindCmdSeq(next) => {
                let at = self.begin(OP_REWIND_CMD_SEQ);
                self.u64(*next);
                self.end(at);
            }
            Order::Fail => {
                let at = self.begin(OP_FAIL);
                self.end(at);
            }
            Order::Market(m) => {
                let at = self.begin(OP_MARKET);
                self.u64(m.village);
                self.u64(m.as_of);
                self.str(&m.currency);
                self.u64(m.treasury as u64);
                self.u64(m.treasury_target as u64);
                self.i32(m.worst_num);
                self.i32(m.worst_den);
                self.u32(m.goods.len() as u32);
                for g in &m.goods {
                    self.str(&g.item);
                    self.u64(g.stock as u64);
                    self.u64(g.target as u64);
                    self.u64(g.anchor as u64);
                    self.i32(g.r_num);
                    self.i32(g.r_den);
                }
                self.end(at);
            }
            Order::MerchantOffers(o) => {
                let at = self.begin(OP_MERCHANT_OFFERS);
                self.u64(o.agent);
                self.u32(o.epoch);
                self.u32(o.member);
                self.u32(o.offers.len() as u32);
                for f in &o.offers {
                    self.str(&f.cost_a);
                    self.i32(f.cost_a_count);
                    self.str(&f.cost_b);
                    self.i32(f.cost_b_count);
                    self.str(&f.result);
                    self.i32(f.result_count);
                    self.i32(f.max_uses);
                    self.u32(f.flags);
                }
                self.end(at);
            }
            Order::WriteGossip(g) => {
                let at = self.begin(OP_WRITE_GOSSIP);
                self.u64(g.op);
                self.u64(g.village);
                self.u64(g.player.0);
                self.u64(g.player.1);
                self.u32(g.gossip);
                self.i32(g.amount);
                self.u32(g.report);
                self.u32(g.flags);
                self.end(at);
            }
            Order::Sentiment(s) => {
                let at = self.begin(OP_SENTIMENT);
                self.u64(s.holder);
                self.i32(s.dislike);
                self.i32(s.friend);
                self.u32(s.flags);
                self.u32(s.entries.len() as u32);
                for &(m, l, v) in &s.entries {
                    self.u64(m);
                    self.u64(l);
                    self.i32(v);
                }
                self.end(at);
            }
            Order::PlaceHouse(h) => {
                let at = self.begin(OP_PLACE_HOUSE);
                self.u64(h.op);
                self.u64(h.village);
                self.str(&h.dimension);
                self.str(&h.template);
                self.str(&h.processors);
                h.pos.iter().for_each(|v| self.i32(*v));
                self.u32(h.rotation);
                self.u32(h.mirror);
                self.u32(h.flags);
                self.u32(h.radius);
                self.u64(h.seed);
                self.i32(h.y_offset);
                self.end(at);
            }
            Order::SpawnResident(r) => {
                let at = self.begin(OP_SPAWN_RESIDENT);
                self.u64(r.op);
                self.u64(r.village);
                self.str(&r.dimension);
                self.str(&r.body);
                self.str(&r.profession);
                self.u32(r.flags);
                r.pos.iter().for_each(|v| self.f64(*v));
                self.u64(r.house);
                self.end(at);
            }
            Order::ScriptVillage(v) => {
                let at = self.begin(OP_SCRIPT_VILLAGE);
                self.u64(*v);
                self.end(at);
            }
            Order::Phase1SpawnNear {
                agent,
                epoch,
                dimension,
                pos,
                radius,
                search,
            } => {
                let at = self.begin(OP_P1_SPAWN_NEAR);
                self.u64(*agent);
                self.u32(*epoch);
                self.str(dimension);
                pos.iter().for_each(|v| self.f64(*v));
                self.u32(*radius);
                search.iter().for_each(|v| self.f64(*v));
                self.end(at);
            }
            Order::Phase1Advance(t) => {
                let at = self.begin(OP_P1_ADVANCE);
                self.u64(*t);
                self.end(at);
            }
            Order::WorksGrant { village, amount } => {
                let at = self.begin(OP_WORKS_GRANT);
                self.u64(*village);
                self.u64(*amount as u64);
                self.end(at);
            }
            Order::WorksRaid { village, tribute } => {
                let at = self.begin(OP_WORKS_RAID);
                self.u64(*village);
                self.u32(u32::from(*tribute));
                self.end(at);
            }
            Order::TributeStart { village, lair } => {
                let at = self.begin(OP_TRIBUTE_START);
                self.u64(*village);
                self.u64(*lair);
                self.end(at);
            }
            Order::NestDay(ticks) => {
                let at = self.begin(OP_NEST_DAY);
                self.u64(*ticks);
                self.end(at);
            }
            Order::TalkReport(r) => {
                let at = self.begin(OP_TALK_REPORT);
                self.u64(r.village);
                self.u16(r.kind);
                self.u8(r.severity);
                self.u8(r.flags);
                self.talk_actor(&r.who);
                self.talk_actor(&r.whom);
                self.i32(r.x);
                self.i32(r.z);
                self.u32(r.ago);
                self.end(at);
            }
            Order::IndustryThreat(t) => {
                let at = self.begin(OP_INDUSTRY_THREAT);
                self.u64(t.id);
                let source = SOURCES.iter().position(|s| *s == t.source).unwrap_or(4);
                self.u32(source as u32);
                self.u32(t.class.index() as u32);
                self.u32(t.count);
                let (kind, target) = match t.target {
                    ThreatTarget::District(d) => (1, d.0),
                    ThreatTarget::Train(tr) => (2, tr.0),
                };
                self.u32(kind);
                self.f64(t.strength);
                self.u64(target);
                self.u64(t.at.0);
                self.end(at);
            }
            Order::ShipCommand { names, record } => {
                let at = self.begin(OP_SHIP_COMMAND);
                self.u32(names.len() as u32);
                for (kind, id, name) in names {
                    self.u32(*kind);
                    self.u32(*id);
                    self.str(name);
                }
                self.buf.extend_from_slice(record);
                self.end(at);
            }
            Order::ConflictCommand { names, record } => {
                let at = self.begin(OP_CONFLICT_COMMAND);
                self.u32(names.len() as u32);
                for (kind, id, name) in names {
                    self.u32(*kind);
                    self.u32(*id);
                    self.str(name);
                }
                self.buf.extend_from_slice(record);
                self.end(at);
            }
            Order::HerdCommand { names, record } => {
                let at = self.begin(OP_HERD_COMMAND);
                self.u32(names.len() as u32);
                for (kind, id, name) in names {
                    self.u32(*kind);
                    self.u32(*id);
                    self.str(name);
                }
                self.buf.extend_from_slice(record);
                self.end(at);
            }
            Order::HerdFound {
                name,
                species,
                dimension,
                x,
                z,
                count,
                seed,
            } => {
                let at = self.begin(OP_HERD_FOUND);
                self.str(name);
                self.str(species);
                self.u32(*dimension);
                self.i32(*x);
                self.i32(*z);
                self.u64(*count);
                self.u64(*seed);
                self.end(at);
            }
            Order::DragonCommand { names, record } => {
                let at = self.begin(OP_DRAGON_COMMAND);
                self.u32(names.len() as u32);
                for (kind, id, name) in names {
                    self.u32(*kind);
                    self.u32(*id);
                    self.str(name);
                }
                self.buf.extend_from_slice(record);
                self.end(at);
            }
            Order::IndustryCommand { names, record } => {
                let at = self.begin(OP_INDUSTRY_COMMAND);
                self.u32(names.len() as u32);
                for (kind, id, name) in names {
                    self.u32(*kind);
                    self.u32(*id);
                    self.str(name);
                }
                self.buf.extend_from_slice(record);
                self.end(at);
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let orders = vec![
            Order::SpawnGroup {
                agent: 9,
                epoch: 2,
                group_kind: "mic_rtsim:human_caravan".into(),
                dimension: "minecraft:overworld".into(),
                pos: [1.5, 64.0, -3.5],
                members: vec![
                    MemberOrder {
                        index: 0,
                        health: 0.0,
                        body_a: -1,
                        body_b: -1,
                        items: vec![],
                    },
                    MemberOrder {
                        index: 1,
                        health: 10.0,
                        body_a: 2,
                        body_b: 3,
                        items: vec![ItemOrder {
                            slot: 0,
                            count: 12,
                            item: "minecraft:emerald".into(),
                        }],
                    },
                ],
            },
            Order::DespawnGroup {
                agent: 9,
                epoch: 2,
                reason: 1,
            },
            Order::SteerGroup {
                agent: 9,
                epoch: 2,
                speed: 1.25,
                pos: [0.0, 1.0, 2.0],
            },
            Order::TransferToContainer {
                op: 4242,
                dimension: "minecraft:overworld".into(),
                item: "minecraft:diamond".into(),
                pos: [1, 2, 3],
                count: 3,
            },
            Order::SaveAtNextStep,
            Order::RewindCmdSeq(17),
            Order::Fail,
            Order::Market(MarketCmd {
                village: 0x0F00_0000_0001,
                as_of: 77,
                currency: "minecraft:emerald".into(),
                treasury: 3_000,
                treasury_target: 20_000,
                worst_num: 1,
                worst_den: 2,
                goods: vec![MarketGood {
                    item: "minecraft:wheat".into(),
                    stock: 400_000,
                    target: 400_000,
                    anchor: 50_000,
                    r_num: 3,
                    r_den: 4,
                }],
            }),
            Order::MerchantOffers(OffersCmd {
                agent: 9,
                epoch: 2,
                member: 0,
                offers: vec![OfferCmd {
                    cost_a: "minecraft:emerald".into(),
                    cost_a_count: 1,
                    cost_b: String::new(),
                    cost_b_count: 0,
                    result: "minecraft:wheat".into(),
                    result_count: 10,
                    max_uses: 0,
                    flags: 1,
                }],
            }),
            Order::WriteGossip(GossipCmd {
                op: 5,
                village: 6,
                player: (7, 8),
                gossip: 2,
                amount: 25,
                report: 3,
                flags: 1,
            }),
            Order::Sentiment(SentimentCmd {
                holder: 6,
                dislike: -25,
                friend: 25,
                flags: 1,
                entries: vec![(1, 2, -30), (3, 4, 40)],
            }),
            Order::PlaceHouse(PlaceHouseCmd {
                op: 11,
                village: 6,
                dimension: "minecraft:overworld".into(),
                template: "ribbits:houses/small_house_red_1".into(),
                processors: "ribbits:main".into(),
                pos: [1, 64, -2],
                rotation: 3,
                mirror: 0,
                flags: 3,
                radius: 48,
                seed: 99,
                y_offset: -1,
            }),
            Order::SpawnResident(SpawnResidentCmd {
                op: 12,
                village: 6,
                dimension: "minecraft:overworld".into(),
                body: "ribbits:ribbit".into(),
                profession: String::new(),
                flags: 1,
                pos: [0.5, 64.0, 0.5],
                house: 11,
            }),
            Order::ScriptVillage(6),
            Order::Phase1SpawnNear {
                agent: 40,
                epoch: 1,
                dimension: "minecraft:overworld".into(),
                pos: [1.5, 64.0, 2.5],
                radius: 500,
                search: [1_000_000.5, -3.0],
            },
            Order::Phase1Advance(72_000),
            Order::WorksGrant {
                village: 6,
                amount: 25_000,
            },
            Order::WorksRaid {
                village: 6,
                tribute: true,
            },
            Order::TributeStart {
                village: 6,
                lair: 0x1d_0000_0003,
            },
            Order::NestDay(600),
            Order::TalkReport(TalkReportCmd {
                village: 6,
                kind: 13,
                severity: 150,
                flags: 1,
                who: TalkActor {
                    class: 5,
                    a: 0x1d_0000_0003,
                    b: 0,
                },
                whom: TalkActor {
                    class: 1,
                    a: 0x1111_2222_3333_4444,
                    b: 0x5555_6666_7777_8888,
                },
                x: -300,
                z: 4_000,
                ago: 24_000,
            }),
            Order::IndustryThreat(Threat {
                id: 5,
                source: ThreatSource::Bandit,
                class: ThreatClass::Siege,
                count: 9,
                strength: 9.5,
                target: ThreatTarget::District(DistrictId(3)),
                loads: None,
                at: Tick(24_000),
            }),
            Order::IndustryCommand {
                names: vec![(0x10, 2, "i/64/minecraft:iron_ingot".into())],
                record: {
                    let mut r = Vec::new();
                    rtsim_industry::wire::encode_cmd(
                        0,
                        &rtsim_industry::cmd::Cmd::DepositDelta(
                            rtsim_industry::cmd::DepositDelta {
                                op: rtsim_kernel::journal::OpId((0x49 << 56) | 3),
                                plugin: rtsim_industry::obs::DepositPluginId::DieselOil,
                                chunk: rtsim_industry::ids::ChunkPos { dim: 1, x: 4, z: 0 },
                                good: rtsim_industry::ids::Good(2),
                                delta: -9,
                            },
                        ),
                        &mut r,
                    )
                    .unwrap();
                    r
                },
            },
            Order::DragonCommand {
                names: vec![(3, 7, "iceandfire:fire_dragon".into())],
                record: {
                    let mut r = Vec::new();
                    rtsim_ecology::boundary::encode_cmd(
                        0,
                        &rtsim_ecology::boundary::Cmd::Steer(rtsim_ecology::boundary::Steer {
                            agent: 0x44 << 56 | 5,
                            epoch: 2,
                            flags: 1,
                            target: (1.0, 90.0, -4.0),
                        }),
                        &rtsim_ecology::boundary::SpeciesDict::default(),
                        &mut r,
                    );
                    r
                },
            },
            Order::HerdCommand {
                names: vec![(3, 9, "minecraft:sheep".into())],
                record: {
                    let mut r = Vec::new();
                    let mut dict = rtsim_ecology::herds::boundary::HerdDict { body: [0; 5] };
                    dict.body[0] = 9;
                    rtsim_ecology::herds::boundary::encode_cmd(
                        0,
                        &rtsim_ecology::herds::boundary::Cmd::Steer(
                            rtsim_ecology::herds::boundary::Steer {
                                agent: 0x48 << 56 | 5,
                                epoch: 2,
                                speed: 0.02,
                                target: (40, -12),
                                arrive: 90_000,
                            },
                        ),
                        &dict,
                        &mut r,
                    );
                    r
                },
            },
            Order::HerdFound {
                name: "Testdown flock".into(),
                species: "minecraft:goat".into(),
                dimension: 1,
                x: -40,
                z: 300,
                count: 12,
                seed: 77,
            },
            Order::ConflictCommand {
                names: vec![(3, 6, "mic_rtsim:conflict/goblin/warrior".into())],
                record: {
                    let mut r = Vec::new();
                    rtsim_conflict::boundary::encode_cmd(
                        0,
                        &rtsim_conflict::boundary::Cmd::Recall(
                            rtsim_conflict::boundary::BandRecall {
                                agent: 0x43 << 56 | 5,
                                epoch: 2,
                                reason: 1,
                                timeout_ticks: 1_200,
                                toward: (10.0, -40.0),
                            },
                        ),
                        &mut r,
                    );
                    r
                },
            },
            Order::ShipCommand {
                names: vec![(2, 4, "minecraft:wheat".into())],
                record: {
                    let mut r = Vec::new();
                    rtsim_ships::boundary::encode_cmd(
                        0,
                        &rtsim_ships::boundary::Cmd::SetCargo(rtsim_ships::boundary::SetCargo {
                            agent: 0x53 << 56 | 5,
                            epoch: 2,
                            op: 0,
                            items: vec![rtsim_ships::boundary::Item { item: 4, count: 12 }],
                        }),
                        &mut r,
                    );
                    r
                },
            },
        ];
        let mut e = Encoder::new();
        for o in &orders {
            e.order(o);
        }
        assert_eq!(decode(&e.into_bytes()).unwrap(), orders);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(decode(&[1, 0, 0, 0, 200, 0, 0, 0]).is_err());
        assert!(decode(&[9, 0, 0, 0, 8, 0, 0, 0]).is_err());
        assert!(decode(&[1, 0, 0]).is_err());
        let mut e = Encoder::new();
        e.order(&Order::RewindCmdSeq(1));
        let mut b = e.into_bytes();
        b.pop();
        assert!(decode(&b).is_err());
    }
}
