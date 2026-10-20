// SPDX-License-Identifier: AGPL-3.0-or-later
//! The minimal engine world: the puppet agents the bridge's core records talk about, a ledger of
//! cross-store transfers, and the engine operations Minecraft reported done. It decides nothing
//! yet (orders stand in for decisions); it keeps the books so that every record the bridge sends
//! lands somewhere and a repeat of an event changes nothing.
//!
//! Phase 1 replaces it: the agents become the sim's caravans and the ledger its markets. What stays
//! is the contract it implements (docs/boundary.md): an embodied agent is abstract again at every
//! session start, a replayed MATERIALIZED never embodies, and TAKEN, DIED and COMMAND_RESULT apply
//! at most once, by operation id while the kernel's `Replay` remembers it and by state after that.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};

use crate::wire::{self, obs};

/// Where an agent is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    /// Simulated only.
    Abstract = 0,
    /// A SPAWN_GROUP is out; MATERIALIZED or SPAWN_REJECTED answers it.
    Spawning = 1,
    /// Puppets exist in Minecraft.
    Embodied = 2,
    /// A DESPAWN_GROUP is out; DEMATERIALIZED answers it.
    Despawning = 3,
}

impl AgentState {
    fn from_u8(v: u8) -> codec::Result<Self> {
        Ok(match v {
            0 => AgentState::Abstract,
            1 => AgentState::Spawning,
            2 => AgentState::Embodied,
            3 => AgentState::Despawning,
            _ => return Err(CodecError::Invalid(format!("agent state {v}"))),
        })
    }
}

/// How a member left the sim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gone {
    /// TAKEN, with how (lead, taming, ...) and the cargo's op id.
    Taken { how: u32, op: u64 },
    /// DIED, with the dropped cargo's op id.
    Died { op: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub slot: u32,
    pub item: String,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub index: u32,
    pub body_kind: String,
    pub health: f32,
    pub max_health: f32,
    pub body_a: i32,
    pub body_b: i32,
    pub flags: u32,
    pub pos: [f64; 3],
    pub items: Vec<Item>,
    pub gone: Option<Gone>,
}

impl Member {
    fn new(index: u32) -> Self {
        Member {
            index,
            body_kind: String::new(),
            health: 0.0,
            max_health: 0.0,
            body_a: -1,
            body_b: -1,
            flags: 0,
            pos: [0.0; 3],
            items: Vec::new(),
            gone: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    pub id: u64,
    pub epoch: u32,
    pub state: AgentState,
    pub group_kind: String,
    pub dimension: String,
    pub pos: [f64; 3],
    pub members: Vec<Member>,
}

impl Agent {
    fn new(id: u64) -> Self {
        Agent {
            id,
            epoch: 0,
            state: AgentState::Abstract,
            group_kind: String::new(),
            dimension: String::new(),
            pos: [0.0; 3],
            members: Vec::new(),
        }
    }

    fn member_mut(&mut self, index: u32) -> &mut Member {
        if let Some(i) = self.members.iter().position(|m| m.index == index) {
            return &mut self.members[i];
        }
        self.members.push(Member::new(index));
        self.members.sort_by_key(|m| m.index);
        let i = self
            .members
            .iter()
            .position(|m| m.index == index)
            .expect("just pushed");
        &mut self.members[i]
    }
}

/// A MEMBER_STATE waiting for the event that captured it (the next journaled record).
#[derive(Clone, Debug)]
struct Captured {
    agent: u64,
    epoch: u32,
    member: Member,
}

/// What the world counted, for logs and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldCounters {
    pub replayed_materialized: u64,
    pub repeated_events: u64,
    pub unknown_agent_events: u64,
}

#[derive(Clone, Debug, Default)]
pub struct World {
    pub agents: BTreeMap<u64, Agent>,
    /// Store reference to balance, from TRANSFER observations (two-sided: `to` gains, `from` loses).
    pub ledger: BTreeMap<u64, i64>,
    /// Engine op ids Minecraft reported applied (COMMAND_RESULT applied or duplicate).
    pub done_ops: BTreeSet<u64>,
    /// Engine op ids Minecraft reported failed.
    pub failed_ops: BTreeSet<u64>,
    // ---- not saved: per session ----
    /// Java's dictionary for this session (Java resends it at every session start): (kind, id) to name.
    pub java_dict: HashMap<(u32, u32), String>,
    pub players: HashMap<(u64, u64), (u32, [f64; 3])>,
    pub module_mask: u32,
    pub game_time: i64,
    captured: Vec<Captured>,
    pub counters: WorldCounters,
}

impl World {
    pub fn embodied(&self) -> usize {
        self.agents
            .values()
            .filter(|a| a.state == AgentState::Embodied)
            .count()
    }

    fn name(&self, kind: u32, id: u32) -> String {
        self.java_dict.get(&(kind, id)).cloned().unwrap_or_default()
    }

    /// SESSION_START: puppets never outlive a session, so every agent the engine thought
    /// embodied (or about to be, or about to stop being) is abstract again, with the last state it
    /// knew. The captures of a clean shutdown arrive later, as journal replay.
    pub fn session_start(&mut self) {
        for a in self.agents.values_mut() {
            a.state = AgentState::Abstract;
        }
        // Captures stay: the event they wait for replays in this session.
        self.java_dict.clear();
        self.players.clear();
    }

    /// A latest-wins record (not journaled).
    pub fn observe_state(&mut self, rec: &wire::Rec<'_>) {
        let b = rec.bytes;
        match rec.kind {
            obs::DICT if rec.size() >= obs::dict::NAME => {
                let id = wire::u32_at(b, obs::dict::ID);
                let len = wire::u32_at(b, obs::dict::NAME_LEN) as usize;
                if obs::dict::NAME + len <= b.len() {
                    let name = String::from_utf8_lossy(&b[obs::dict::NAME..obs::dict::NAME + len])
                        .into_owned();
                    self.java_dict.insert((rec.aux, id), name);
                }
            }
            obs::TICK if rec.size() >= obs::tick::SIZE => {
                self.game_time = wire::i64_at(b, obs::tick::GAME_TIME)
            }
            obs::MODULES if rec.size() >= obs::modules::SIZE => {
                self.module_mask = wire::u32_at(b, obs::modules::MASK)
            }
            obs::PLAYER if rec.size() >= obs::player::SIZE => {
                let key = (
                    wire::u64_at(b, obs::player::UUID_MOST),
                    wire::u64_at(b, obs::player::UUID_LEAST),
                );
                let pos = [
                    wire::f64_at(b, obs::player::X),
                    wire::f64_at(b, obs::player::Y),
                    wire::f64_at(b, obs::player::Z),
                ];
                self.players.insert(key, (rec.aux, pos));
            }
            obs::PUPPET_STATE if rec.size() >= obs::puppet_state::SIZE => {
                let id = wire::u64_at(b, obs::puppet_state::AGENT);
                let epoch = wire::u32_at(b, obs::puppet_state::EPOCH);
                if let Some(a) = self.agents.get_mut(&id)
                    && a.epoch == epoch
                    && a.state == AgentState::Embodied
                {
                    let m = a.member_mut(rec.aux);
                    m.health = wire::f32_at(b, obs::puppet_state::HEALTH);
                    m.flags = wire::u32_at(b, obs::puppet_state::FLAGS);
                    m.pos = [
                        wire::f64_at(b, obs::puppet_state::X),
                        wire::f64_at(b, obs::puppet_state::Y),
                        wire::f64_at(b, obs::puppet_state::Z),
                    ];
                    if rec.aux == 0 {
                        a.pos = m.pos;
                    }
                }
            }
            _ => {}
        }
    }

    /// A journaled record the kernel's `Replay` admitted (new). `replayed` says it is from before
    /// this session (its sequence is below the session's first).
    pub fn apply_journaled(&mut self, rec: &wire::Rec<'_>, replayed: bool) {
        let b = rec.bytes;
        if rec.kind != obs::MEMBER_STATE {
            // A capture is always followed by the event that made it; anything else ends it.
            let captured = std::mem::take(&mut self.captured);
            self.apply_event(rec, replayed, captured);
            return;
        }
        if rec.size() < obs::member_state::ITEMS {
            return;
        }
        let n = wire::u32_at(b, obs::member_state::ITEM_COUNT) as usize;
        if obs::member_state::size(n) > rec.size() {
            return;
        }
        let mut m = Member::new(rec.aux);
        m.body_kind = self.name(
            wire::DICT_BODY,
            wire::u32_at(b, obs::member_state::BODY_KIND),
        );
        m.flags = wire::u32_at(b, obs::member_state::FLAGS);
        m.health = wire::f32_at(b, obs::member_state::HEALTH);
        m.max_health = wire::f32_at(b, obs::member_state::MAX_HEALTH);
        m.pos = [
            wire::f64_at(b, obs::member_state::X),
            wire::f64_at(b, obs::member_state::Y),
            wire::f64_at(b, obs::member_state::Z),
        ];
        m.body_a = wire::i32_at(b, obs::member_state::BODY_A);
        m.body_b = wire::i32_at(b, obs::member_state::BODY_B);
        for i in 0..n {
            let at = obs::member_state::ITEMS + i * obs::item::SIZE;
            m.items.push(Item {
                slot: wire::u32_at(b, at + obs::item::SLOT),
                item: self.name(wire::DICT_ITEM, wire::u32_at(b, at + obs::item::ITEM)),
                count: wire::i32_at(b, at + obs::item::COUNT),
            });
        }
        self.captured.push(Captured {
            agent: wire::u64_at(b, obs::member_state::AGENT),
            epoch: wire::u32_at(b, obs::member_state::EPOCH),
            member: m,
        });
    }

    /// The `Replay` refused a journaled record (already applied, or its op id was): a capture
    /// waiting for it is dropped with it.
    pub fn refused_journaled(&mut self, rec: &wire::Rec<'_>) {
        if rec.kind != obs::MEMBER_STATE {
            self.captured.clear();
        }
    }

    fn store_captured(agent: &mut Agent, captured: &[Captured], only: Option<u32>) {
        for c in captured {
            if c.agent != agent.id
                || c.epoch != agent.epoch
                || only.is_some_and(|i| i != c.member.index)
            {
                continue;
            }
            let gone = agent.member_mut(c.member.index).gone;
            *agent.member_mut(c.member.index) = Member {
                gone,
                ..c.member.clone()
            };
        }
    }

    fn apply_event(&mut self, rec: &wire::Rec<'_>, replayed: bool, captured: Vec<Captured>) {
        let b = rec.bytes;
        let need = match rec.kind {
            obs::MATERIALIZED => obs::materialized::SIZE,
            obs::SPAWN_REJECTED => obs::spawn_rejected::SIZE,
            obs::DEMATERIALIZED => obs::dematerialized::SIZE,
            obs::TAKEN => obs::taken::SIZE,
            obs::DIED => obs::died::SIZE,
            obs::TRANSFER => obs::transfer::SIZE,
            obs::COMMAND_RESULT => obs::command_result::SIZE,
            _ => return, // a journaled kind this engine does not know yet: skipped
        };
        if rec.size() < need {
            return;
        }
        match rec.kind {
            obs::MATERIALIZED => {
                let id = wire::u64_at(b, obs::materialized::AGENT);
                let epoch = wire::u32_at(b, obs::materialized::EPOCH);
                if replayed {
                    // From before this session: those puppets are gone (docs/boundary.md §2.3).
                    self.counters.replayed_materialized += 1;
                    return;
                }
                let dim = self.name(
                    wire::DICT_DIMENSION,
                    wire::u32_at(b, obs::materialized::DIM),
                );
                let a = self.agents.entry(id).or_insert_with(|| Agent::new(id));
                if a.state == AgentState::Spawning && a.epoch != epoch {
                    self.counters.unknown_agent_events += 1;
                    return;
                }
                a.epoch = epoch;
                a.state = AgentState::Embodied;
                if !dim.is_empty() {
                    a.dimension = dim;
                }
            }
            obs::SPAWN_REJECTED => {
                let id = wire::u64_at(b, obs::spawn_rejected::AGENT);
                let epoch = wire::u32_at(b, obs::spawn_rejected::EPOCH);
                match self.agents.get_mut(&id) {
                    Some(a) if a.epoch == epoch && a.state != AgentState::Embodied => {
                        a.state = AgentState::Abstract
                    }
                    _ => self.counters.unknown_agent_events += 1,
                }
            }
            obs::DEMATERIALIZED => {
                let id = wire::u64_at(b, obs::dematerialized::AGENT);
                let epoch = wire::u32_at(b, obs::dematerialized::EPOCH);
                match self.agents.get_mut(&id) {
                    Some(a) if a.epoch == epoch => {
                        Self::store_captured(a, &captured, None);
                        if let Some(c) = captured
                            .iter()
                            .find(|c| c.agent == id && c.member.index == 0)
                        {
                            a.pos = c.member.pos;
                        }
                        a.state = AgentState::Abstract;
                    }
                    _ => self.counters.unknown_agent_events += 1,
                }
            }
            obs::TAKEN | obs::DIED => {
                let (id, epoch, index, gone) = if rec.kind == obs::TAKEN {
                    (
                        wire::u64_at(b, obs::taken::AGENT),
                        wire::u32_at(b, obs::taken::EPOCH),
                        wire::u32_at(b, obs::taken::MEMBER),
                        Gone::Taken {
                            how: rec.aux,
                            op: wire::u64_at(b, obs::taken::OP_ID),
                        },
                    )
                } else {
                    (
                        wire::u64_at(b, obs::died::AGENT),
                        wire::u32_at(b, obs::died::EPOCH),
                        rec.aux,
                        Gone::Died {
                            op: wire::u64_at(b, obs::died::OP_ID),
                        },
                    )
                };
                let Some(a) = self.agents.get_mut(&id) else {
                    self.counters.unknown_agent_events += 1;
                    return;
                };
                if a.epoch != epoch && !replayed {
                    self.counters.unknown_agent_events += 1;
                }
                // By state: a member already gone stays gone, once (docs/boundary.md "Idempotent events").
                if a.members
                    .iter()
                    .any(|m| m.index == index && m.gone.is_some())
                {
                    self.counters.repeated_events += 1;
                    return;
                }
                if a.epoch == epoch {
                    Self::store_captured(a, &captured, Some(index));
                }
                a.member_mut(index).gone = Some(gone);
            }
            obs::TRANSFER => {
                let from = wire::u64_at(b, obs::transfer::FROM);
                let to = wire::u64_at(b, obs::transfer::TO);
                let amount = wire::i64_at(b, obs::transfer::AMOUNT);
                *self.ledger.entry(to).or_insert(0) += amount;
                *self.ledger.entry(from).or_insert(0) -= amount;
            }
            obs::COMMAND_RESULT => {
                let op = wire::u64_at(b, obs::command_result::OP_ID);
                match rec.aux {
                    obs::command_result::APPLIED | obs::command_result::DUPLICATE_OP => {
                        // By state: an op already done stays done, once.
                        if !self.done_ops.insert(op) {
                            self.counters.repeated_events += 1;
                        }
                        self.failed_ops.remove(&op);
                    }
                    obs::command_result::FAILED if !self.done_ops.contains(&op) => {
                        self.failed_ops.insert(op);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// The engine sent SPAWN_GROUP for `agent`: it is spawning, with these members.
    pub fn spawning(
        &mut self,
        id: u64,
        epoch: u32,
        group_kind: &str,
        dimension: &str,
        pos: [f64; 3],
        members: &[Member],
    ) {
        let a = self.agents.entry(id).or_insert_with(|| Agent::new(id));
        a.epoch = epoch;
        a.state = AgentState::Spawning;
        a.group_kind = group_kind.to_string();
        a.dimension = dimension.to_string();
        a.pos = pos;
        for m in members {
            let gone = a.member_mut(m.index).gone;
            *a.member_mut(m.index) = Member { gone, ..m.clone() };
        }
    }

    /// The engine sent DESPAWN_GROUP.
    pub fn despawning(&mut self, id: u64, epoch: u32) {
        if let Some(a) = self.agents.get_mut(&id)
            && a.epoch == epoch
            && a.state == AgentState::Embodied
        {
            a.state = AgentState::Despawning;
        }
    }
}

// ---- pages ---------------------------------------------------------------------------------------

fn put_str(w: &mut Writer, s: &str) {
    w.bytes(s.as_bytes());
}

fn get_str(r: &mut Reader<'_>) -> codec::Result<String> {
    let b = r.bytes()?;
    String::from_utf8(b.to_vec()).map_err(|e| CodecError::Invalid(format!("utf-8: {e}")))
}

fn put_pos(w: &mut Writer, p: &[f64; 3]) {
    p.iter().for_each(|v| w.f64(*v));
}

fn get_pos(r: &mut Reader<'_>) -> codec::Result<[f64; 3]> {
    Ok([r.f64()?, r.f64()?, r.f64()?])
}

/// 2: captures waiting for their event are saved. Before release a version change wipes the pages
/// (PLAN.md §4.9): `start` refuses an older page with its reason.
const WORLD_VERSION: u16 = 2;

fn put_member(w: &mut Writer, m: &Member) {
    w.u32(m.index);
    put_str(w, &m.body_kind);
    w.u32(m.health.to_bits());
    w.u32(m.max_health.to_bits());
    w.var_i64(i64::from(m.body_a));
    w.var_i64(i64::from(m.body_b));
    w.u32(m.flags);
    put_pos(w, &m.pos);
    w.var_u64(m.items.len() as u64);
    for it in &m.items {
        w.u32(it.slot);
        put_str(w, &it.item);
        w.var_i64(i64::from(it.count));
    }
    match m.gone {
        None => w.u8(0),
        Some(Gone::Taken { how, op }) => {
            w.u8(1);
            w.u32(how);
            w.u64(op);
        }
        Some(Gone::Died { op }) => {
            w.u8(2);
            w.u64(op);
        }
    }
}

fn get_member(r: &mut Reader<'_>) -> codec::Result<Member> {
    let mut m = Member::new(r.u32()?);
    m.body_kind = get_str(r)?;
    m.health = f32::from_bits(r.u32()?);
    m.max_health = f32::from_bits(r.u32()?);
    m.body_a = i32_of(r.var_i64()?)?;
    m.body_b = i32_of(r.var_i64()?)?;
    m.flags = r.u32()?;
    m.pos = get_pos(r)?;
    for _ in 0..r.var_u64()? {
        let slot = r.u32()?;
        let item = get_str(r)?;
        let count = i32_of(r.var_i64()?)?;
        m.items.push(Item { slot, item, count });
    }
    m.gone = match r.u8()? {
        0 => None,
        1 => Some(Gone::Taken {
            how: r.u32()?,
            op: r.u64()?,
        }),
        2 => Some(Gone::Died { op: r.u64()? }),
        g => return Err(CodecError::Invalid(format!("member gone tag {g}"))),
    };
    Ok(m)
}

impl Encode for World {
    fn encode(&self, w: &mut Writer) {
        w.u16(WORLD_VERSION);
        w.var_u64(self.agents.len() as u64);
        for a in self.agents.values() {
            w.u64(a.id);
            w.u32(a.epoch);
            w.u8(a.state as u8);
            put_str(w, &a.group_kind);
            put_str(w, &a.dimension);
            put_pos(w, &a.pos);
            w.var_u64(a.members.len() as u64);
            for m in &a.members {
                put_member(w, m);
            }
        }
        // A capture waiting for its event: a snapshot can fall between a MEMBER_STATE and the
        // TAKEN, DIED or DEMATERIALIZED it belongs to (review finding 3).
        w.var_u64(self.captured.len() as u64);
        for c in &self.captured {
            w.u64(c.agent);
            w.u32(c.epoch);
            put_member(w, &c.member);
        }
        w.var_u64(self.ledger.len() as u64);
        for (k, v) in &self.ledger {
            w.u64(*k);
            w.var_i64(*v);
        }
        w.var_u64(self.done_ops.len() as u64);
        self.done_ops.iter().for_each(|op| w.u64(*op));
        w.var_u64(self.failed_ops.len() as u64);
        self.failed_ops.iter().for_each(|op| w.u64(*op));
    }
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

impl Decode for World {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let version = r.u16()?;
        if version != WORLD_VERSION {
            return Err(CodecError::Invalid(format!(
                "world page version {version}, this build reads {WORLD_VERSION}"
            )));
        }
        let mut world = World::default();
        for _ in 0..r.var_u64()? {
            let mut a = Agent::new(r.u64()?);
            a.epoch = r.u32()?;
            a.state = AgentState::from_u8(r.u8()?)?;
            a.group_kind = get_str(r)?;
            a.dimension = get_str(r)?;
            a.pos = get_pos(r)?;
            for _ in 0..r.var_u64()? {
                a.members.push(get_member(r)?);
            }
            world.agents.insert(a.id, a);
        }
        for _ in 0..r.var_u64()? {
            let agent = r.u64()?;
            let epoch = r.u32()?;
            let member = get_member(r)?;
            world.captured.push(Captured {
                agent,
                epoch,
                member,
            });
        }
        for _ in 0..r.var_u64()? {
            let k = r.u64()?;
            world.ledger.insert(k, r.var_i64()?);
        }
        for _ in 0..r.var_u64()? {
            world.done_ops.insert(r.u64()?);
        }
        for _ in 0..r.var_u64()? {
            world.failed_ops.insert(r.u64()?);
        }
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{RecordWriter, Records};

    fn records(build: impl FnOnce(&mut RecordWriter<'_>)) -> Vec<u8> {
        let mut area = vec![0u8; 4096];
        let used = {
            let mut w = RecordWriter::new(&mut area);
            build(&mut w);
            w.used()
        };
        area.truncate(used);
        area
    }

    #[test]
    fn a_taken_member_is_taken_once_and_the_world_round_trips() {
        let mut world = World::default();
        world.spawning(
            5,
            1,
            "kind",
            "dim",
            [1.0, 2.0, 3.0],
            &[Member::new(0), Member::new(1)],
        );
        let area = records(|w| {
            let r = w
                .reserve(obs::MATERIALIZED, obs::materialized::SIZE, 2)
                .unwrap();
            wire::put_u64(r, obs::materialized::AGENT, 5);
            wire::put_u32(r, obs::materialized::EPOCH, 1);
            let r = w
                .reserve(obs::MEMBER_STATE, obs::member_state::size(1), 1)
                .unwrap();
            wire::put_u64(r, obs::member_state::AGENT, 5);
            wire::put_u32(r, obs::member_state::EPOCH, 1);
            wire::put_f32(r, obs::member_state::HEALTH, 7.5);
            wire::put_u32(r, obs::member_state::ITEM_COUNT, 1);
            wire::put_i32(r, obs::member_state::ITEMS + obs::item::COUNT, 12);
            for _ in 0..2 {
                let r = w.reserve(obs::TAKEN, obs::taken::SIZE, 2).unwrap();
                wire::put_u64(r, obs::taken::AGENT, 5);
                wire::put_u32(r, obs::taken::EPOCH, 1);
                wire::put_u32(r, obs::taken::MEMBER, 1);
                wire::put_u64(r, obs::taken::OP_ID, wire::JAVA_OP_BIT | 77);
            }
        });
        for rec in Records::new(&area) {
            world.apply_journaled(&rec, false);
        }
        let a = &world.agents[&5];
        assert_eq!(a.state, AgentState::Embodied);
        assert_eq!(
            a.members[1].gone,
            Some(Gone::Taken {
                how: 2,
                op: wire::JAVA_OP_BIT | 77
            })
        );
        assert_eq!(a.members[1].health, 7.5);
        assert_eq!(a.members[1].items.len(), 1);
        assert_eq!(
            world.counters.repeated_events, 1,
            "the second TAKEN changed nothing"
        );

        world.ledger.insert(3, -4);
        world.done_ops.insert(4242);
        let back = World::from_bytes(&world.to_bytes()).unwrap();
        assert_eq!(back.agents, world.agents);
        assert_eq!(back.ledger, world.ledger);
        assert_eq!(back.done_ops, world.done_ops);

        world.session_start();
        assert_eq!(world.agents[&5].state, AgentState::Abstract);
    }

    #[test]
    fn a_capture_waiting_for_its_event_survives_a_snapshot() {
        let mut world = World::default();
        world.spawning(
            6,
            1,
            "kind",
            "dim",
            [0.0; 3],
            &[Member::new(0), Member::new(1)],
        );
        let state = records(|w| {
            let r = w
                .reserve(obs::MEMBER_STATE, obs::member_state::size(0), 1)
                .unwrap();
            wire::put_u64(r, obs::member_state::AGENT, 6);
            wire::put_u32(r, obs::member_state::EPOCH, 1);
            wire::put_f32(r, obs::member_state::HEALTH, 3.5);
        });
        let taken = records(|w| {
            let r = w.reserve(obs::TAKEN, obs::taken::SIZE, 1).unwrap();
            wire::put_u64(r, obs::taken::AGENT, 6);
            wire::put_u32(r, obs::taken::EPOCH, 1);
            wire::put_u32(r, obs::taken::MEMBER, 1);
        });
        world.apply_journaled(&Records::new(&state).next().unwrap(), false);
        // The snapshot falls here; a new session starts from it.
        let mut back = World::from_bytes(&world.to_bytes()).unwrap();
        back.session_start();
        back.apply_journaled(&Records::new(&taken).next().unwrap(), true);
        let m = &back.agents[&6].members[1];
        assert_eq!(
            m.health, 3.5,
            "the capture reached its event across the snapshot"
        );
        assert!(m.gone.is_some());
    }

    #[test]
    fn a_replayed_materialized_never_embodies() {
        let mut world = World::default();
        let area = records(|w| {
            let r = w
                .reserve(obs::MATERIALIZED, obs::materialized::SIZE, 1)
                .unwrap();
            wire::put_u64(r, obs::materialized::AGENT, 9);
        });
        let rec = Records::new(&area).next().unwrap();
        world.apply_journaled(&rec, true);
        assert!(
            world
                .agents
                .get(&9)
                .is_none_or(|a| a.state != AgentState::Embodied)
        );
        assert_eq!(world.counters.replayed_materialized, 1);
    }
}
