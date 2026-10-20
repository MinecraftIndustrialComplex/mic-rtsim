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
//! | 9 | an industry command (docs/industry-boundary.md §4), as if the industry engine had decided it: tests drive the Java side with it | u32 names, then per name: u32 dictionary kind, u32 Java's id, str name (the entries the record uses, so it never waits for Java's DICT); then the record exactly as `rtsim_industry::wire` encodes it (cmdSeq ignored), its dictionary ids Java's own (GOOD, NAME, BLOCK_STATE and dimension) |
//! | 8 | a threat for the industry engine (I19), in place of another system's attack | u64 id, u32 source (0 natural, 1 dragon, 2 bandit, 3 boss, 4 synthetic), u32 class (0 to 3), u32 count, u32 target kind (1 district, 2 train), f64 strength, u64 target, u64 at (game time; 0 or past: now) |

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

const SOURCES: [ThreatSource; 5] = [
    ThreatSource::Natural,
    ThreatSource::Dragon,
    ThreatSource::Bandit,
    ThreatSource::Boss,
    ThreatSource::Synthetic,
];

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
            Order::IndustryThreat(Threat {
                id: 5,
                source: ThreatSource::Bandit,
                class: ThreatClass::Siege,
                count: 9,
                strength: 9.5,
                target: ThreatTarget::District(DistrictId(3)),
                at: Tick(24_000),
            }),
            Order::IndustryCommand {
                names: vec![(0x10, 2, "i/64/minecraft:iron_ingot".into())],
                record: {
                    let mut r = Vec::new();
                    rtsim_industry::wire::encode_cmd(
                        0,
                        &rtsim_industry::cmd::Cmd::ChannelDelta(
                            rtsim_industry::cmd::ChannelDelta {
                                op: rtsim_kernel::journal::OpId((0x49 << 56) | 3),
                                channel: rtsim_industry::ids::ChannelId(4),
                                goods: rtsim_industry::ids::GoodVec::from_pairs([(
                                    rtsim_industry::ids::Good(2),
                                    -9,
                                )]),
                            },
                        ),
                        &mut r,
                    )
                    .unwrap();
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
