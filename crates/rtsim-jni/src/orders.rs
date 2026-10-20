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
//! | 8 to 15 | reserved (8: the industry engine's threats) | |
//! | 16 | MARKET | u64 village, u64 asOfObsSeq, str currency, i64 treasury, i64 treasuryTarget, i32 worstNum, i32 worstDen, u32 n, then per good: str item, i64 stock, i64 target, i64 anchor, i32 rNum, i32 rDen |
//! | 17 | MERCHANT_OFFERS | u64 agent, u32 epoch, u32 member, u32 n, then per offer: str costA, i32 costACount, str costB ("" none), i32 costBCount, str result, i32 resultCount, i32 maxUses, u32 flags |
//! | 18 | WRITE_GOSSIP | u64 opId, u64 village, u64 playerMost, u64 playerLeast, u32 gossipType, i32 amount, u32 report, u32 flags |
//! | 19 | SENTIMENT | u64 holder, i32 dislike, i32 friend, u32 flags, u32 n, then per entry: u64 most, u64 least, i32 sentiment |
//! | 20 | PLACE_HOUSE | u64 opId, u64 village, str dimension, str template, str processors ("" none), i32 x, i32 y, i32 z, u32 rotation, u32 mirror, u32 flags, u32 radius, u64 seed, i32 yOffset |
//! | 21 | SPAWN_RESIDENT | u64 opId, u64 village, str dimension, str body, str profession ("" none), u32 flags, f64 x, f64 y, f64 z, u64 house |
//! | 22 | a scripted village: the engine applies none of its observations and sends none of its own commands for it | u64 village |
//! | 23 | TEST: materialize the phase 1 caravan with wares nearest (searchX, searchZ) within radius blocks, as this group, at (x, y, z) | u64 agent, u32 epoch, str dimension, f64 x, f64 y, f64 z, u32 radius, f64 searchX, f64 searchZ |
//! | 24 | TEST: the phase 1 world runs this many more ticks ahead of game time | u64 ticks |

use crate::wire::{f32_at, f64_at, i32_at, u16_at, u32_at, u64_at};

pub const OP_SPAWN_GROUP: u16 = 1;
pub const OP_DESPAWN_GROUP: u16 = 2;
pub const OP_STEER_GROUP: u16 = 3;
pub const OP_TRANSFER_TO_CONTAINER: u16 = 4;
pub const OP_SAVE_AT_NEXT_STEP: u16 = 5;
pub const OP_REWIND_CMD_SEQ: u16 = 6;
pub const OP_FAIL: u16 = 7;
pub const OP_MARKET: u16 = 16;
pub const OP_MERCHANT_OFFERS: u16 = 17;
pub const OP_WRITE_GOSSIP: u16 = 18;
pub const OP_SENTIMENT: u16 = 19;
pub const OP_PLACE_HOUSE: u16 = 20;
pub const OP_SPAWN_RESIDENT: u16 = 21;
pub const OP_SCRIPT_VILLAGE: u16 = 22;
pub const OP_P1_SPAWN_NEAR: u16 = 23;
pub const OP_P1_ADVANCE: u16 = 24;

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
        )
    }

    /// Needs an engine opened in TEST mode.
    pub fn test_only(&self) -> bool {
        matches!(
            self,
            Order::Fail | Order::Phase1SpawnNear { .. } | Order::Phase1Advance(_)
        )
    }
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
