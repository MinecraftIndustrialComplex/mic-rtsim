// SPDX-License-Identifier: AGPL-3.0-or-later
//! TALK's three records (docs/boundary.md §13, TALK.md §9.2): the reference codec. The core's
//! `talk/TalkRecords.java` mirrors it, and the bytes of one ask, one facts and one talked record
//! are pinned on both sides (`tests` here, `TalkRecordsTest` there).
//!
//! - **TALK_ASK** (0x00C0, Java to the engine, latest wins, `aux` = n): Java's open questions,
//!   `n` asks of [`ask::SIZE`] bytes from offset 8, sent each tick until answered.
//! - **TALK_FACTS** (0x8A00, engine to Java, `aux` = news n): one answer, per session, with no op
//!   id: 48 bytes of facts and then `n` news items of [`news::SIZE`] bytes, each with three refs of
//!   [`reference::SIZE`] bytes (TK30).
//! - **TALKED** (0x0A00, Java to the engine, journaled): a talk the village credits, 64 bytes
//!   ([`talked`]), exactly once by its op id (docs/boundary.md §13.4).
//!
//! The reasons for the layout are in TALK.md §9; this file is the layout. Every value is
//! little-endian.

use crate::wire::{self, put_i32, put_i64, put_u16, put_u32, put_u64};

/// TALK_ASK: latest wins, Java to the engine (TK10: TALK's range is 0x00C0 to 0x00CF).
pub const TALK_ASK: u16 = 0x00C0;
/// TALK_FACTS: a command, engine to Java (TK10: 0x8A00 to 0x8AFF).
pub const TALK_FACTS: u16 = 0x8A00;
/// TALKED: journaled, Java to the engine (TK10: 0x0A00 is TALK's journaled kind).
pub const TALKED: u16 = 0x0A00;

/// One ask (TALK.md §9.2). The record is the 8-byte header and `n` of these.
pub mod ask {
    /// Where the first ask starts.
    pub const FIRST: usize = 8;
    pub const SIZE: usize = 56;
    /// u64, Java's, per session, never 0.
    pub const ID: usize = 0;
    pub const PLAYER_MOST: usize = 8;
    pub const PLAYER_LEAST: usize = 16;
    /// u32: [`holder`] kinds.
    pub const HOLDER_KIND: usize = 24;
    /// u32, dictionary 1.
    pub const DIMENSION: usize = 28;
    /// u64: a village's Java id, a puppet's agent, a camp's key.
    pub const HOLDER: usize = 32;
    /// i32, i32: the talker's block position.
    pub const X: usize = 40;
    pub const Z: usize = 44;
    /// u32: [`NEWS`], [`FACTION`].
    pub const FLAGS: usize = 48;
    pub const RESERVED: usize = 52;

    /// Flag 1: the answer carries news.
    pub const NEWS: u32 = 1;
    /// Flag 2: the answer carries the faction block (slice 4).
    pub const FACTION: u32 = 2;

    /// The holder kinds of an ask.
    pub mod holder {
        pub const VILLAGE: u32 = 1;
        pub const CARAVAN: u32 = 2;
        pub const FACTION: u32 = 3;
        pub const SHIP: u32 = 4;
    }

    /// The size of a whole TALK_ASK record of `n` asks.
    pub const fn record_size(n: usize) -> usize {
        FIRST + n * SIZE
    }
}

/// TALKED (docs/boundary.md §13.4): a ribbit talked with a player and its village credits it. The
/// journaled prefix (8 obsSeq, 16 gameTime) comes first, as for every journaled kind.
pub mod talked {
    /// u64, Java's: the kernel's `Replay` applies a record of this op id once.
    pub const OP_ID: usize = 24;
    /// u64: the village's Java id.
    pub const VILLAGE: usize = 32;
    pub const PLAYER_MOST: usize = 40;
    pub const PLAYER_LEAST: usize = 48;
    /// u32: the ribbit's body, dictionary 3 (0 unknown).
    pub const BODY: usize = 56;
    pub const RESERVED: usize = 60;
    pub const SIZE: usize = 64;
}

/// The answer (TALK.md §9.2).
pub mod facts {
    /// u64 at 8.
    pub const CMD_SEQ: usize = 8;
    pub const ASK_ID: usize = 16;
    /// u32: [`HOLDER`], [`PORT_CLOSED`], [`BLOCKADED`], [`FACTION`].
    pub const FLAGS: usize = 24;
    /// i32: the holder's sentiment toward the player, rtsim-knowledge's scale.
    pub const SENTIMENT: usize = 28;
    /// u32, u32: the scarcest good (dictionary 2, 0 none) and its stock per target, thousandths.
    pub const SCARCEST: usize = 32;
    pub const SCARCEST_RATIO: usize = 36;
    /// u32, u32: the most glutted good and its stock per target, thousandths.
    pub const GLUTTED: usize = 40;
    pub const GLUTTED_RATIO: usize = 44;
    /// Where the news starts; also the size of a record with none.
    pub const NEWS: usize = 48;

    /// The engine knows the holder.
    pub const HOLDER: u32 = 1;
    pub const PORT_CLOSED: u32 = 2;
    pub const BLOCKADED: u32 = 4;
    /// A faction block follows (slice 4; never set yet).
    pub const FACTION: u32 = 8;

    /// News items in one answer, at most (TK20).
    pub const MAX_NEWS: usize = 4;

    /// The size of a whole TALK_FACTS record of `n` news items.
    pub const fn record_size(n: usize) -> usize {
        NEWS + n * super::news::SIZE
    }
}

/// One news item (TALK.md §9.2).
pub mod news {
    pub const SIZE: usize = 120;
    /// u16: the report kind (rtsim-knowledge's `ReportKind` id).
    pub const KIND: usize = 0;
    /// u8, u8: the report's severity and flags.
    pub const SEVERITY: usize = 2;
    pub const REPORT_FLAGS: usize = 3;
    /// u32: the talk flags below.
    pub const TALK_FLAGS: usize = 4;
    /// u32: ticks since the report, saturating.
    pub const AGO: usize = 8;
    /// i32, i32: the place from the talker, the short way round.
    pub const DX: usize = 12;
    pub const DZ: usize = 16;
    pub const RESERVED: usize = 20;
    /// The refs: who, whom and the settlement nearest the place.
    pub const WHO: usize = 24;
    pub const WHOM: usize = 56;
    pub const NEAR: usize = 88;

    /// Who is the player who asked.
    pub const YOU_DID: u32 = 1;
    /// Whom is the player who asked.
    pub const TO_YOU: u32 = 2;
    /// Whom is the holder.
    pub const TO_US: u32 = 4;
    /// Who is the holder.
    pub const BY_US: u32 = 8;
    /// The place is within 96 blocks of the holder (TK14).
    pub const SEEN: u32 = 16;
}

/// A ref: an actor as the engine describes it (TALK.md §9.2, TK30).
pub mod reference {
    pub const SIZE: usize = 32;
    /// u8: [`class`].
    pub const CLASS: usize = 0;
    /// u8: [`DEAD`], [`POS`].
    pub const FLAGS: usize = 1;
    /// u32: dictionary 5 (culture) for settlements, camps and chiefs, dictionary 3 (body) for
    /// dragons, lairs and ships; 0 when not known.
    pub const WHAT: usize = 4;
    pub const A: usize = 8;
    pub const B: usize = 16;
    /// i32, i32: where it is from the talker, the short way round, when [`POS`].
    pub const DX: usize = 24;
    pub const DZ: usize = 28;

    pub const DEAD: u8 = 1;
    pub const POS: u8 = 2;

    /// The classes of a ref.
    pub mod class {
        pub const UNKNOWN: u8 = 0;
        /// a, b the UUID halves.
        pub const PLAYER: u8 = 1;
        /// a Java's village id.
        pub const VILLAGE: u8 = 2;
        /// a the base's id, b its owner.
        pub const BASE: u8 = 3;
        /// a its seed, b its home village's id (0 none).
        pub const CARAVAN: u8 = 4;
        /// a the faction key.
        pub const CAMP: u8 = 5;
        /// a its seed, b its camp's faction key (or its home's key).
        pub const CHIEF: u8 = 6;
        /// a its seed, b its lair's key (0 none).
        pub const DRAGON: u8 = 7;
        /// a the lair's key.
        pub const LAIR: u8 = 8;
        /// a the haven's key.
        pub const HAVEN: u8 = 9;
        pub const SHIP: u8 = 10;
    }
}

/// One ask, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ask {
    pub id: u64,
    pub player: (u64, u64),
    pub holder_kind: u32,
    pub dimension: u32,
    pub holder: u64,
    pub x: i32,
    pub z: i32,
    pub flags: u32,
}

impl Ask {
    pub fn decode(b: &[u8]) -> Ask {
        debug_assert!(b.len() >= ask::SIZE);
        Ask {
            id: wire::u64_at(b, ask::ID),
            player: (
                wire::u64_at(b, ask::PLAYER_MOST),
                wire::u64_at(b, ask::PLAYER_LEAST),
            ),
            holder_kind: wire::u32_at(b, ask::HOLDER_KIND),
            dimension: wire::u32_at(b, ask::DIMENSION),
            holder: wire::u64_at(b, ask::HOLDER),
            x: wire::i32_at(b, ask::X),
            z: wire::i32_at(b, ask::Z),
            flags: wire::u32_at(b, ask::FLAGS),
        }
    }

    pub fn encode(&self, b: &mut [u8]) {
        put_u64(b, ask::ID, self.id);
        put_u64(b, ask::PLAYER_MOST, self.player.0);
        put_u64(b, ask::PLAYER_LEAST, self.player.1);
        put_u32(b, ask::HOLDER_KIND, self.holder_kind);
        put_u32(b, ask::DIMENSION, self.dimension);
        put_u64(b, ask::HOLDER, self.holder);
        put_i32(b, ask::X, self.x);
        put_i32(b, ask::Z, self.z);
        put_u32(b, ask::FLAGS, self.flags);
        put_u32(b, ask::RESERVED, 0);
    }
}

/// Why a TALK_ASK record was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AskError {
    /// The record is not a TALK_ASK.
    Kind(u16),
    /// The record is shorter than its `n` asks need: (size, needed).
    Short(usize, usize),
}

impl std::fmt::Display for AskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AskError::Kind(k) => write!(f, "kind {k:#06x} is not a TALK_ASK"),
            AskError::Short(size, need) => {
                write!(
                    f,
                    "a TALK_ASK of {size} bytes holds fewer asks than its n ({need} needed)"
                )
            }
        }
    }
}

/// Decodes a whole TALK_ASK record (header included) into its asks.
pub fn decode_asks(rec: &[u8]) -> Result<Vec<Ask>, AskError> {
    if rec.len() < wire::RECORD_HEADER {
        return Err(AskError::Short(rec.len(), wire::RECORD_HEADER));
    }
    let kind = wire::u16_at(rec, wire::R_KIND);
    if kind != TALK_ASK {
        return Err(AskError::Kind(kind));
    }
    let n = wire::u32_at(rec, wire::R_AUX) as usize;
    let need = n
        .checked_mul(ask::SIZE)
        .and_then(|v| v.checked_add(ask::FIRST))
        .unwrap_or(usize::MAX);
    if rec.len() < need {
        return Err(AskError::Short(rec.len(), need));
    }
    Ok((0..n)
        .map(|i| Ask::decode(&rec[ask::FIRST + i * ask::SIZE..]))
        .collect())
}

/// A whole TALK_ASK record of these asks (what Java writes; the engine only decodes).
pub fn encode_asks(asks: &[Ask]) -> Vec<u8> {
    let mut rec = vec![0u8; wire::align8(ask::record_size(asks.len()))];
    put_u16(&mut rec, wire::R_KIND, TALK_ASK);
    let size = rec.len() as u16;
    put_u16(&mut rec, wire::R_SIZE, size);
    put_u32(&mut rec, wire::R_AUX, asks.len() as u32);
    for (i, a) in asks.iter().enumerate() {
        a.encode(&mut rec[ask::FIRST + i * ask::SIZE..]);
    }
    rec
}

/// A TALKED record, decoded: a talk of a player with one of a village's ribbits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Talked {
    pub obs_seq: u64,
    pub game_time: i64,
    pub op_id: u64,
    pub village: u64,
    pub player: (u64, u64),
    pub body: u32,
}

/// Why a TALKED record was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TalkedError {
    /// The record is not a TALKED.
    Kind(u16),
    /// The record is not exactly [`talked::SIZE`] bytes.
    Size(usize),
    /// The player is the nil UUID: nobody to credit.
    NilPlayer,
}

impl std::fmt::Display for TalkedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TalkedError::Kind(k) => write!(f, "kind {k:#06x} is not a TALKED"),
            TalkedError::Size(n) => {
                write!(
                    f,
                    "a TALKED of {n} bytes (exactly {} expected)",
                    talked::SIZE
                )
            }
            TalkedError::NilPlayer => write!(f, "a TALKED by the nil player"),
        }
    }
}

impl Talked {
    /// Decodes a whole TALKED record (header included). Strict: a record of any other size is
    /// refused, so a layout change cannot be half read.
    pub fn decode(rec: &[u8]) -> Result<Talked, TalkedError> {
        if rec.len() < wire::RECORD_HEADER {
            return Err(TalkedError::Size(rec.len()));
        }
        let kind = wire::u16_at(rec, wire::R_KIND);
        if kind != TALKED {
            return Err(TalkedError::Kind(kind));
        }
        if rec.len() != talked::SIZE || usize::from(wire::u16_at(rec, wire::R_SIZE)) != talked::SIZE
        {
            return Err(TalkedError::Size(rec.len()));
        }
        let player = (
            wire::u64_at(rec, talked::PLAYER_MOST),
            wire::u64_at(rec, talked::PLAYER_LEAST),
        );
        if player == (0, 0) {
            return Err(TalkedError::NilPlayer);
        }
        Ok(Talked {
            obs_seq: wire::u64_at(rec, wire::obs::OBS_SEQ),
            game_time: wire::i64_at(rec, wire::obs::GAME_TIME),
            op_id: wire::u64_at(rec, talked::OP_ID),
            village: wire::u64_at(rec, talked::VILLAGE),
            player,
            body: wire::u32_at(rec, talked::BODY),
        })
    }

    /// The whole record (what Java writes; the engine only decodes).
    pub fn encode(&self) -> Vec<u8> {
        let mut rec = vec![0u8; talked::SIZE];
        put_u16(&mut rec, wire::R_KIND, TALKED);
        put_u16(&mut rec, wire::R_SIZE, talked::SIZE as u16);
        put_u32(&mut rec, wire::R_AUX, 0);
        put_u64(&mut rec, wire::obs::OBS_SEQ, self.obs_seq);
        put_i64(&mut rec, wire::obs::GAME_TIME, self.game_time);
        put_u64(&mut rec, talked::OP_ID, self.op_id);
        put_u64(&mut rec, talked::VILLAGE, self.village);
        put_u64(&mut rec, talked::PLAYER_MOST, self.player.0);
        put_u64(&mut rec, talked::PLAYER_LEAST, self.player.1);
        put_u32(&mut rec, talked::BODY, self.body);
        put_u32(&mut rec, talked::RESERVED, 0);
        rec
    }
}

/// A ref, decoded: see [`reference`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ref {
    pub class: u8,
    pub flags: u8,
    pub what: u32,
    pub a: u64,
    pub b: u64,
    pub dx: i32,
    pub dz: i32,
}

impl Ref {
    pub fn decode(b: &[u8]) -> Ref {
        Ref {
            class: b[reference::CLASS],
            flags: b[reference::FLAGS],
            what: wire::u32_at(b, reference::WHAT),
            a: wire::u64_at(b, reference::A),
            b: wire::u64_at(b, reference::B),
            dx: wire::i32_at(b, reference::DX),
            dz: wire::i32_at(b, reference::DZ),
        }
    }

    pub fn encode(&self, b: &mut [u8]) {
        b[reference::CLASS] = self.class;
        b[reference::FLAGS] = self.flags;
        put_u32(b, reference::WHAT, self.what);
        put_u64(b, reference::A, self.a);
        put_u64(b, reference::B, self.b);
        put_i32(b, reference::DX, self.dx);
        put_i32(b, reference::DZ, self.dz);
    }

    /// Whether the ref carries a place.
    pub fn has_pos(&self) -> bool {
        self.flags & reference::POS != 0
    }
}

/// One news item, decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct News {
    pub kind: u16,
    pub severity: u8,
    pub report_flags: u8,
    pub talk_flags: u32,
    pub ago: u32,
    pub dx: i32,
    pub dz: i32,
    pub who: Ref,
    pub whom: Ref,
    pub near: Ref,
}

impl News {
    pub fn decode(b: &[u8]) -> News {
        News {
            kind: wire::u16_at(b, news::KIND),
            severity: b[news::SEVERITY],
            report_flags: b[news::REPORT_FLAGS],
            talk_flags: wire::u32_at(b, news::TALK_FLAGS),
            ago: wire::u32_at(b, news::AGO),
            dx: wire::i32_at(b, news::DX),
            dz: wire::i32_at(b, news::DZ),
            who: Ref::decode(&b[news::WHO..]),
            whom: Ref::decode(&b[news::WHOM..]),
            near: Ref::decode(&b[news::NEAR..]),
        }
    }

    pub fn encode(&self, b: &mut [u8]) {
        put_u16(b, news::KIND, self.kind);
        b[news::SEVERITY] = self.severity;
        b[news::REPORT_FLAGS] = self.report_flags;
        put_u32(b, news::TALK_FLAGS, self.talk_flags);
        put_u32(b, news::AGO, self.ago);
        put_i32(b, news::DX, self.dx);
        put_i32(b, news::DZ, self.dz);
        put_u32(b, news::RESERVED, 0);
        self.who.encode(&mut b[news::WHO..]);
        self.whom.encode(&mut b[news::WHOM..]);
        self.near.encode(&mut b[news::NEAR..]);
    }
}

/// An answer, decoded. The good ids and every ref's `what` are the engine's dictionary ids of
/// this session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub ask_id: u64,
    pub flags: u32,
    pub sentiment: i32,
    pub scarcest: u32,
    pub scarcest_ratio: u32,
    pub glutted: u32,
    pub glutted_ratio: u32,
    pub news: Vec<News>,
}

impl Facts {
    /// The size of the record this answer encodes to.
    pub fn size(&self) -> usize {
        facts::record_size(self.news.len().min(facts::MAX_NEWS))
    }

    /// The whole TALK_FACTS record with command sequence `seq`. At most [`facts::MAX_NEWS`] news
    /// items are written.
    pub fn encode(&self, seq: u64) -> Vec<u8> {
        let n = self.news.len().min(facts::MAX_NEWS);
        let mut rec = vec![0u8; facts::record_size(n)];
        put_u16(&mut rec, wire::R_KIND, TALK_FACTS);
        let size = rec.len() as u16;
        put_u16(&mut rec, wire::R_SIZE, size);
        put_u32(&mut rec, wire::R_AUX, n as u32);
        put_u64(&mut rec, facts::CMD_SEQ, seq);
        put_u64(&mut rec, facts::ASK_ID, self.ask_id);
        put_u32(&mut rec, facts::FLAGS, self.flags);
        put_i32(&mut rec, facts::SENTIMENT, self.sentiment);
        put_u32(&mut rec, facts::SCARCEST, self.scarcest);
        put_u32(&mut rec, facts::SCARCEST_RATIO, self.scarcest_ratio);
        put_u32(&mut rec, facts::GLUTTED, self.glutted);
        put_u32(&mut rec, facts::GLUTTED_RATIO, self.glutted_ratio);
        for (i, n) in self.news.iter().take(n).enumerate() {
            n.encode(&mut rec[facts::NEWS + i * news::SIZE..]);
        }
        rec
    }

    /// Decodes a whole TALK_FACTS record: its command sequence and the answer (Java's job; here
    /// for tests and tools).
    pub fn decode(rec: &[u8]) -> Result<(u64, Facts), String> {
        if rec.len() < facts::NEWS {
            return Err(format!("a TALK_FACTS of {} bytes", rec.len()));
        }
        let kind = wire::u16_at(rec, wire::R_KIND);
        if kind != TALK_FACTS {
            return Err(format!("kind {kind:#06x} is not a TALK_FACTS"));
        }
        let n = wire::u32_at(rec, wire::R_AUX) as usize;
        if n > facts::MAX_NEWS {
            return Err(format!("{n} news items (at most {})", facts::MAX_NEWS));
        }
        if rec.len() < facts::record_size(n) {
            return Err(format!(
                "a TALK_FACTS of {} bytes holds fewer items than its n = {n}",
                rec.len()
            ));
        }
        let f = Facts {
            ask_id: wire::u64_at(rec, facts::ASK_ID),
            flags: wire::u32_at(rec, facts::FLAGS),
            sentiment: wire::i32_at(rec, facts::SENTIMENT),
            scarcest: wire::u32_at(rec, facts::SCARCEST),
            scarcest_ratio: wire::u32_at(rec, facts::SCARCEST_RATIO),
            glutted: wire::u32_at(rec, facts::GLUTTED),
            glutted_ratio: wire::u32_at(rec, facts::GLUTTED_RATIO),
            news: (0..n)
                .map(|i| News::decode(&rec[facts::NEWS + i * news::SIZE..]))
                .collect(),
        };
        Ok((wire::u64_at(rec, facts::CMD_SEQ), f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hex of a byte string, 32 bytes to a line, for the test's failure message and for
    /// pasting the pinned bytes into the Java side's test.
    fn hex(b: &[u8]) -> String {
        b.iter().map(|v| format!("{v:02x}")).collect()
    }

    /// The pinned ask (Java's `TalkRecordsTest` pins the same bytes): ask 7 by the player
    /// (0x1111222233334444, 0x5555666677778888) to the village 0x0102030405060708 of dimension 3,
    /// the talker at (-100, 2000), wanting news. Built at the literal offsets of TALK.md §9.2, not
    /// through this file's constants, so a moved constant cannot move the pin with it.
    #[test]
    fn the_ask_bytes_are_pinned() {
        let ask = Ask {
            id: 7,
            player: (0x1111_2222_3333_4444, 0x5555_6666_7777_8888),
            holder_kind: ask::holder::VILLAGE,
            dimension: 3,
            holder: 0x0102_0304_0506_0708,
            x: -100,
            z: 2000,
            flags: ask::NEWS,
        };
        let rec = encode_asks(&[ask]);
        let mut want = vec![0u8; 64];
        want[0..2].copy_from_slice(&0x00C0u16.to_le_bytes());
        want[2..4].copy_from_slice(&64u16.to_le_bytes());
        want[4..8].copy_from_slice(&1u32.to_le_bytes());
        want[8..16].copy_from_slice(&7u64.to_le_bytes());
        want[16..24].copy_from_slice(&0x1111_2222_3333_4444u64.to_le_bytes());
        want[24..32].copy_from_slice(&0x5555_6666_7777_8888u64.to_le_bytes());
        want[32..36].copy_from_slice(&1u32.to_le_bytes());
        want[36..40].copy_from_slice(&3u32.to_le_bytes());
        want[40..48].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        want[48..52].copy_from_slice(&(-100i32).to_le_bytes());
        want[52..56].copy_from_slice(&2000i32.to_le_bytes());
        want[56..60].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(rec, want, "got {}", hex(&rec));
        assert_eq!(
            hex(&rec),
            PINNED_ASK.replace(char::is_whitespace, ""),
            "the pinned ask"
        );
        assert_eq!(decode_asks(&rec).unwrap(), vec![ask]);
    }

    /// The pinned answer: ask 7 answered with a known holder, a closed port, sentiment -30, wheat
    /// (id 2) scarce at 400 per mille and gold (id 5) glutted at 3,500, and one item of news: a
    /// Raid (kind 13, severity 150, report flags 1) 24,000 ticks ago, 300 east and 40 north of the
    /// talker (dx 300, dz -40), seen, done to us by a camp (class 5, key 99, culture id 4) to the
    /// village 0x0102030405060708 (class 2, culture id 4, 10 blocks west of the talker), near no
    /// settlement.
    #[test]
    fn the_facts_bytes_are_pinned() {
        let village = Ref {
            class: reference::class::VILLAGE,
            flags: reference::POS,
            what: 4,
            a: 0x0102_0304_0506_0708,
            b: 0,
            dx: -10,
            dz: 5,
        };
        let camp = Ref {
            class: reference::class::CAMP,
            flags: reference::POS,
            what: 4,
            a: 99,
            b: 0,
            dx: 280,
            dz: -50,
        };
        let f = Facts {
            ask_id: 7,
            flags: facts::HOLDER | facts::PORT_CLOSED,
            sentiment: -30,
            scarcest: 2,
            scarcest_ratio: 400,
            glutted: 5,
            glutted_ratio: 3500,
            news: vec![News {
                kind: 13,
                severity: 150,
                report_flags: 1,
                talk_flags: news::TO_US | news::SEEN,
                ago: 24_000,
                dx: 300,
                dz: -40,
                who: camp,
                whom: village,
                near: Ref::default(),
            }],
        };
        let rec = f.encode(0x1234);
        assert_eq!(rec.len(), 48 + 120);
        // The same bytes, by literal offset (TALK.md §9.2).
        let mut want = vec![0u8; 168];
        want[0..2].copy_from_slice(&0x8A00u16.to_le_bytes());
        want[2..4].copy_from_slice(&168u16.to_le_bytes());
        want[4..8].copy_from_slice(&1u32.to_le_bytes());
        want[8..16].copy_from_slice(&0x1234u64.to_le_bytes());
        want[16..24].copy_from_slice(&7u64.to_le_bytes());
        want[24..28].copy_from_slice(&3u32.to_le_bytes());
        want[28..32].copy_from_slice(&(-30i32).to_le_bytes());
        want[32..36].copy_from_slice(&2u32.to_le_bytes());
        want[36..40].copy_from_slice(&400u32.to_le_bytes());
        want[40..44].copy_from_slice(&5u32.to_le_bytes());
        want[44..48].copy_from_slice(&3500u32.to_le_bytes());
        let n = 48;
        want[n..n + 2].copy_from_slice(&13u16.to_le_bytes());
        want[n + 2] = 150;
        want[n + 3] = 1;
        want[n + 4..n + 8].copy_from_slice(&(4u32 | 16).to_le_bytes());
        want[n + 8..n + 12].copy_from_slice(&24_000u32.to_le_bytes());
        want[n + 12..n + 16].copy_from_slice(&300i32.to_le_bytes());
        want[n + 16..n + 20].copy_from_slice(&(-40i32).to_le_bytes());
        // who at 24: the camp
        let w = n + 24;
        want[w] = 5;
        want[w + 1] = 2;
        want[w + 4..w + 8].copy_from_slice(&4u32.to_le_bytes());
        want[w + 8..w + 16].copy_from_slice(&99u64.to_le_bytes());
        want[w + 24..w + 28].copy_from_slice(&280i32.to_le_bytes());
        want[w + 28..w + 32].copy_from_slice(&(-50i32).to_le_bytes());
        // whom at 56: the village
        let m = n + 56;
        want[m] = 2;
        want[m + 1] = 2;
        want[m + 4..m + 8].copy_from_slice(&4u32.to_le_bytes());
        want[m + 8..m + 16].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        want[m + 24..m + 28].copy_from_slice(&(-10i32).to_le_bytes());
        want[m + 28..m + 32].copy_from_slice(&5i32.to_le_bytes());
        // near at 88: nothing.
        assert_eq!(rec, want, "got {}", hex(&rec));
        assert_eq!(
            hex(&rec),
            PINNED_FACTS.replace(char::is_whitespace, ""),
            "the pinned answer"
        );
        let (seq, back) = Facts::decode(&rec).unwrap();
        assert_eq!((seq, &back), (0x1234, &f));
    }

    /// The hex of [`the_ask_bytes_are_pinned`]'s record, for the Java side's test to copy.
    const PINNED_ASK: &str = "\
        c000400001000000070000000000000044443333222211118888777766665555\
        010000000300000008070605040302019cffffffd00700000100000000000000";

    /// The hex of [`the_facts_bytes_are_pinned`]'s record, for the Java side's test to copy.
    const PINNED_FACTS: &str = "\
        008aa800010000003412000000000000070000000000000003000000e2ffffff\
        020000009001000005000000ac0d00000d00960114000000c05d00002c010000\
        d8ffffff00000000050200000400000063000000000000000000000000000000\
        18010000ceffffff020200000400000008070605040302010000000000000000\
        f6ffffff05000000000000000000000000000000000000000000000000000000\
        0000000000000000";

    /// The pinned talked record (Java's `TalkRecordsTest` pins the same bytes): observation 0x1234
    /// at game time 123,456, op id 0x0A0B0C0D0E0F1011, by the player (0x1111222233334444,
    /// 0x5555666677778888) to a ribbit of village 0x0102030405060708 whose body is dictionary id 3.
    /// Built at the literal offsets of docs/boundary.md §13.4, not through this file's constants.
    #[test]
    fn the_talked_bytes_are_pinned() {
        let t = Talked {
            obs_seq: 0x1234,
            game_time: 123_456,
            op_id: 0x0A0B_0C0D_0E0F_1011,
            village: 0x0102_0304_0506_0708,
            player: (0x1111_2222_3333_4444, 0x5555_6666_7777_8888),
            body: 3,
        };
        let rec = t.encode();
        let mut want = vec![0u8; 64];
        want[0..2].copy_from_slice(&0x0A00u16.to_le_bytes());
        want[2..4].copy_from_slice(&64u16.to_le_bytes());
        want[8..16].copy_from_slice(&0x1234u64.to_le_bytes());
        want[16..24].copy_from_slice(&123_456i64.to_le_bytes());
        want[24..32].copy_from_slice(&0x0A0B_0C0D_0E0F_1011u64.to_le_bytes());
        want[32..40].copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        want[40..48].copy_from_slice(&0x1111_2222_3333_4444u64.to_le_bytes());
        want[48..56].copy_from_slice(&0x5555_6666_7777_8888u64.to_le_bytes());
        want[56..60].copy_from_slice(&3u32.to_le_bytes());
        assert_eq!(rec, want, "got {}", hex(&rec));
        assert_eq!(
            hex(&rec),
            PINNED_TALKED.replace(char::is_whitespace, ""),
            "the pinned talked"
        );
        assert_eq!(Talked::decode(&rec), Ok(t));
        assert_eq!(talked::SIZE, 64);
    }

    /// The hex of [`the_talked_bytes_are_pinned`]'s record, for the Java side's test to copy.
    const PINNED_TALKED: &str = "\
        000a400000000000341200000000000040e201000000000011100f0e0d0c0b0a\
        0807060504030201444433332222111188887777666655550300000000000000";

    #[test]
    fn a_talked_that_does_not_fit_is_an_error() {
        let good = Talked {
            obs_seq: 1,
            game_time: 1,
            op_id: 9,
            village: 7,
            player: (1, 2),
            body: 0,
        }
        .encode();
        assert!(Talked::decode(&good).is_ok());
        // Truncated: a record shorter than its layout, with the header still claiming 64.
        assert_eq!(Talked::decode(&good[..56]), Err(TalkedError::Size(56)));
        assert_eq!(Talked::decode(&good[..4]), Err(TalkedError::Size(4)));
        // Oversized: a longer record whose header says so.
        let mut big = good.clone();
        big.extend_from_slice(&[0; 8]);
        put_u16(&mut big, wire::R_SIZE, 72);
        assert_eq!(Talked::decode(&big), Err(TalkedError::Size(72)));
        // The header lying about the size of a record that is 64 bytes.
        let mut lie = good.clone();
        put_u16(&mut lie, wire::R_SIZE, 72);
        assert_eq!(Talked::decode(&lie), Err(TalkedError::Size(64)));
        // Another kind.
        let mut other = good.clone();
        put_u16(&mut other, wire::R_KIND, 0x0A01);
        assert_eq!(Talked::decode(&other), Err(TalkedError::Kind(0x0A01)));
        // The nil player.
        let mut nil = good;
        put_u64(&mut nil, talked::PLAYER_MOST, 0);
        put_u64(&mut nil, talked::PLAYER_LEAST, 0);
        assert_eq!(Talked::decode(&nil), Err(TalkedError::NilPlayer));
    }

    #[test]
    fn asks_that_do_not_fit_are_errors() {
        let mut rec = encode_asks(&[Ask {
            id: 1,
            player: (0, 0),
            holder_kind: 1,
            dimension: 1,
            holder: 1,
            x: 0,
            z: 0,
            flags: 0,
        }]);
        // n claims two asks in a record that holds one.
        put_u32(&mut rec, wire::R_AUX, 2);
        assert!(matches!(
            decode_asks(&rec),
            Err(AskError::Short(64, need)) if need == 120
        ));
        // A huge n cannot overflow the size arithmetic.
        put_u32(&mut rec, wire::R_AUX, u32::MAX);
        assert!(decode_asks(&rec).is_err());
        assert!(decode_asks(&rec[..4]).is_err());
        put_u16(&mut rec, wire::R_KIND, 0x00C1);
        put_u32(&mut rec, wire::R_AUX, 1);
        assert_eq!(decode_asks(&rec), Err(AskError::Kind(0x00C1)));
    }

    #[test]
    fn the_sizes_are_what_the_spec_says() {
        assert_eq!(ask::SIZE, 56);
        assert_eq!(ask::record_size(1), 64);
        assert_eq!(news::SIZE, 120);
        assert_eq!(reference::SIZE, 32);
        assert_eq!(facts::record_size(0), 48);
        assert_eq!(facts::record_size(4), 528);
        assert_eq!(news::NEAR + reference::SIZE, news::SIZE);
        // Every record size is a multiple of the record alignment.
        for n in 0..=4 {
            assert_eq!(facts::record_size(n) % wire::RECORD_ALIGN, 0);
        }
    }

    #[test]
    fn at_most_four_news_items_are_written() {
        let f = Facts {
            news: vec![News::default(); 6],
            ..Facts::default()
        };
        let rec = f.encode(1);
        assert_eq!(rec.len(), facts::record_size(4));
        assert_eq!(Facts::decode(&rec).unwrap().1.news.len(), 4);
    }
}
