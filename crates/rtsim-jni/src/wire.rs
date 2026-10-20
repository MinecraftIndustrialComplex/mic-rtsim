// SPDX-License-Identifier: AGPL-3.0-or-later
//! The binary layout of the boundary (docs/boundary.md), mirrored from the bridge's Java constants:
//! `neoforge/core/.../boundary/Layout.java`, `Obs.java` and `Cmd.java`. Those files, this one and
//! docs/boundary.md change together; there is no format version before release.
//!
//! Every multi-byte value is little-endian. Offsets are from the start of a buffer (header fields)
//! or of a record (record fields, its 8-byte header included).

/// Alignment of every buffer's base address.
pub const BUFFER_ALIGN: usize = 64;
/// The buffer header; records start right after it.
pub const HEADER_BYTES: usize = 64;

/// u64, release/acquire. The hand-over whose records the buffer holds; 0 = never.
pub const H_PUBLISHED: usize = 0;
/// u64, release/acquire. The last hand-over the consumer finished with.
pub const H_CONSUMED: usize = 8;
/// u32. Bytes in the record area, written once by the allocator (the engine).
pub const H_CAPACITY: usize = 16;
/// u32. Bytes of records, from [`HEADER_BYTES`].
pub const H_USED: usize = 20;
/// u32. Number of records.
pub const H_COUNT: usize = 24;
/// u32. Direction-specific flags.
pub const H_FLAGS: usize = 28;
/// u64. Observations: `lastAppliedCmdSeq`. Commands: `ackedObsSeq`.
pub const H_WORD_A: usize = 32;
/// i64. Observations: game time at the hand-over. Commands: game time the step reached.
pub const H_WORD_B: usize = 40;
/// u64. Observations: highest obsSeq in the buffer. Commands: highest cmdSeq.
pub const H_WORD_C: usize = 48;
/// u64. Reserved, zero.
pub const H_RESERVED: usize = 56;

pub const OBS_FLAG_SESSION_START: u32 = 1;
pub const OBS_FLAG_REPLAY: u32 = 2;
pub const OBS_FLAG_DROPPED: u32 = 4;
pub const CMD_FLAG_MORE: u32 = 1;

pub const R_KIND: usize = 0;
pub const R_SIZE: usize = 2;
pub const R_AUX: usize = 4;
pub const RECORD_HEADER: usize = 8;
pub const RECORD_ALIGN: usize = 8;
/// The largest record size a u16 holds that is a multiple of 8.
pub const MAX_RECORD: usize = 0xFFF8;
/// The smallest record area an engine may allocate (`Layout.MIN_CAPACITY`).
pub const MIN_CAPACITY: usize = MAX_RECORD + 128;

/// Op ids made on the Java side have the top bit set (`Obs.JAVA_OP_BIT`, kernel `JAVA_OP_BIT`).
pub const JAVA_OP_BIT: u64 = 1 << 63;

/// Rounds up to the record alignment.
pub const fn align8(n: usize) -> usize {
    (n + (RECORD_ALIGN - 1)) & !(RECORD_ALIGN - 1)
}

/// Dictionary kinds (DICT aux, both directions).
pub const DICT_DIMENSION: u32 = 1;
pub const DICT_ITEM: u32 = 2;
pub const DICT_BODY: u32 = 3;

/// Observation records (Java to the engine). Kinds below 0x8000.
pub mod obs {
    pub const SESSION_START: u16 = 0x0001;
    pub const DICT: u16 = 0x0002;
    pub const TICK: u16 = 0x0003;
    pub const MODULES: u16 = 0x0004;
    pub const WORLD_SAVED: u16 = 0x0005;
    pub const PLAYER: u16 = 0x0010;
    pub const PUPPET_STATE: u16 = 0x0011;

    pub const MATERIALIZED: u16 = 0x0100;
    pub const SPAWN_REJECTED: u16 = 0x0101;
    pub const DEMATERIALIZED: u16 = 0x0102;
    pub const MEMBER_STATE: u16 = 0x0103;
    pub const TAKEN: u16 = 0x0104;
    pub const DIED: u16 = 0x0105;
    pub const TRANSFER: u16 = 0x0110;
    pub const COMMAND_RESULT: u16 = 0x0111;

    pub const fn is_journaled(kind: u16) -> bool {
        kind >= 0x0100 && kind < 0x8000
    }

    /// Prefix of every journaled record.
    pub const OBS_SEQ: usize = 8;
    pub const GAME_TIME: usize = 16;
    pub const BODY: usize = 24;

    pub mod session_start {
        pub const GAME_TIME: usize = 8;
        pub const LAST_APPLIED_CMD_SEQ: usize = 16;
        pub const JOURNAL_FIRST_OBS_SEQ: usize = 24;
        pub const WORLD_NEXT_OBS_SEQ: usize = 32;
        pub const MODULE_MASK: usize = 40;
        pub const SIZE: usize = 48;
    }

    pub mod dict {
        pub const ID: usize = 8;
        pub const NAME_LEN: usize = 12;
        pub const NAME: usize = 16;
    }

    pub mod tick {
        pub const GAME_TIME: usize = 8;
        pub const SIZE: usize = 16;
    }

    pub mod modules {
        pub const MASK: usize = 8;
        pub const SIZE: usize = 16;
    }

    pub mod world_saved {
        pub const DURABLE_OBS_SEQ: usize = 8;
        pub const SIZE: usize = 16;
    }

    pub mod player {
        pub const UUID_MOST: usize = 8;
        pub const UUID_LEAST: usize = 16;
        pub const X: usize = 24;
        pub const Y: usize = 32;
        pub const Z: usize = 40;
        pub const SIZE: usize = 48;
    }

    pub mod puppet_state {
        pub const AGENT: usize = 8;
        pub const EPOCH: usize = 16;
        pub const DIM: usize = 20;
        pub const HEALTH: usize = 24;
        pub const FLAGS: usize = 28;
        pub const X: usize = 32;
        pub const Y: usize = 40;
        pub const Z: usize = 48;
        pub const SIZE: usize = 56;
    }

    pub mod materialized {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const DIM: usize = 36;
        pub const SIZE: usize = 40;
    }

    pub mod spawn_rejected {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const CMD_SEQ: usize = 40;
        pub const SIZE: usize = 48;
    }

    pub mod dematerialized {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const MEMBERS: usize = 36;
        pub const SIZE: usize = 40;

        pub const COMMAND: u32 = 1;
        pub const NOT_TICKING: u32 = 2;
        pub const UNLOADED: u32 = 3;
        pub const SESSION_END: u32 = 4;
        pub const MEMBERS_GONE: u32 = 5;
        pub const NOT_EMBODIED: u32 = 6;
    }

    pub mod member_state {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const BODY_KIND: usize = 36;
        pub const DIM: usize = 40;
        pub const FLAGS: usize = 44;
        pub const HEALTH: usize = 48;
        pub const MAX_HEALTH: usize = 52;
        pub const X: usize = 56;
        pub const Y: usize = 64;
        pub const Z: usize = 72;
        pub const BODY_A: usize = 80;
        pub const BODY_B: usize = 84;
        pub const ITEM_COUNT: usize = 88;
        pub const ITEMS: usize = 96;

        pub const fn size(items: usize) -> usize {
            ITEMS + items * super::item::SIZE
        }
    }

    /// One item stack (MEMBER_STATE, and SPAWN_GROUP member blocks). 16 bytes.
    pub mod item {
        pub const SLOT: usize = 0;
        pub const ITEM: usize = 4;
        pub const COUNT: usize = 8;
        pub const SIZE: usize = 16;
    }

    pub mod taken {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const MEMBER: usize = 36;
        pub const PLAYER_MOST: usize = 40;
        pub const PLAYER_LEAST: usize = 48;
        pub const OP_ID: usize = 56;
        pub const SIZE: usize = 64;
    }

    pub mod died {
        pub const AGENT: usize = 24;
        pub const EPOCH: usize = 32;
        pub const KILLER_MOST: usize = 40;
        pub const KILLER_LEAST: usize = 48;
        pub const OP_ID: usize = 56;
        pub const SIZE: usize = 64;
    }

    pub mod transfer {
        pub const OP_ID: usize = 24;
        pub const FROM: usize = 32;
        pub const TO: usize = 40;
        pub const ITEM: usize = 48;
        pub const AMOUNT: usize = 56;
        pub const SIZE: usize = 64;
    }

    pub mod command_result {
        pub const CMD_SEQ: usize = 24;
        pub const OP_ID: usize = 32;
        pub const SIZE: usize = 40;

        pub const APPLIED: u32 = 1;
        pub const DUPLICATE_OP: u32 = 2;
        pub const FAILED: u32 = 3;
    }
}

/// Command records (the engine to Java). Kinds 0x8000 and up.
pub mod cmd {
    pub const DICT: u16 = 0x8001;
    pub const SPAWN_GROUP: u16 = 0x8002;
    pub const DESPAWN_GROUP: u16 = 0x8003;
    pub const STEER_GROUP: u16 = 0x8004;
    pub const TRANSFER_TO_CONTAINER: u16 = 0x8005;

    pub const CMD_SEQ: usize = 8;
    pub const BODY: usize = 16;

    pub mod dict {
        pub const ID: usize = 16;
        pub const NAME_LEN: usize = 20;
        pub const NAME: usize = 24;

        pub const fn size(name_bytes: usize) -> usize {
            crate::wire::align8(NAME + name_bytes)
        }
    }

    pub mod spawn_group {
        pub const AGENT: usize = 16;
        pub const EPOCH: usize = 24;
        pub const GROUP_KIND: usize = 28;
        pub const DIM: usize = 32;
        pub const FLAGS: usize = 36;
        pub const X: usize = 40;
        pub const Y: usize = 48;
        pub const Z: usize = 56;
        pub const MEMBERS: usize = 64;

        pub mod member {
            pub const INDEX: usize = 0;
            pub const ITEM_COUNT: usize = 4;
            pub const HEALTH: usize = 8;
            pub const BODY_A: usize = 12;
            pub const BODY_B: usize = 16;
            pub const ITEMS: usize = 24;

            pub const fn size(items: usize) -> usize {
                ITEMS + items * crate::wire::obs::item::SIZE
            }
        }
    }

    pub mod despawn_group {
        pub const AGENT: usize = 16;
        pub const EPOCH: usize = 24;
        pub const REASON: usize = 28;
        pub const SIZE: usize = 32;
    }

    pub mod steer_group {
        pub const AGENT: usize = 16;
        pub const EPOCH: usize = 24;
        pub const SPEED: usize = 28;
        pub const X: usize = 32;
        pub const Y: usize = 40;
        pub const Z: usize = 48;
        pub const SIZE: usize = 56;
    }

    pub mod transfer_to_container {
        pub const OP_ID: usize = 16;
        pub const DIM: usize = 24;
        pub const ITEM: usize = 28;
        pub const X: usize = 32;
        pub const Y: usize = 36;
        pub const Z: usize = 40;
        pub const COUNT: usize = 44;
        pub const SIZE: usize = 48;
    }
}

// ---- little-endian field access on byte slices -------------------------------------------------

#[inline]
pub fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().expect("2 bytes"))
}

#[inline]
pub fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

#[inline]
pub fn i32_at(b: &[u8], at: usize) -> i32 {
    u32_at(b, at) as i32
}

#[inline]
pub fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

#[inline]
pub fn i64_at(b: &[u8], at: usize) -> i64 {
    u64_at(b, at) as i64
}

#[inline]
pub fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}

#[inline]
pub fn f64_at(b: &[u8], at: usize) -> f64 {
    f64::from_bits(u64_at(b, at))
}

#[inline]
pub fn put_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline]
pub fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
pub fn put_i32(b: &mut [u8], at: usize, v: i32) {
    put_u32(b, at, v as u32);
}

#[inline]
pub fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

#[inline]
pub fn put_i64(b: &mut [u8], at: usize, v: i64) {
    put_u64(b, at, v as u64);
}

#[inline]
pub fn put_f32(b: &mut [u8], at: usize, v: f32) {
    put_u32(b, at, v.to_bits());
}

#[inline]
pub fn put_f64(b: &mut [u8], at: usize, v: f64) {
    put_u64(b, at, v.to_bits());
}

/// One record in a buffer.
#[derive(Clone, Copy, Debug)]
pub struct Rec<'a> {
    pub kind: u16,
    pub aux: u32,
    /// The whole record, header included (`bytes.len()` is its size).
    pub bytes: &'a [u8],
}

impl Rec<'_> {
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
}

/// Walks the records of a record area. A malformed record ends the walk and leaves its reason in
/// [`Records::error`]; an unknown kind is the caller's to skip.
pub struct Records<'a> {
    area: &'a [u8],
    cursor: usize,
    error: Option<String>,
}

impl<'a> Records<'a> {
    pub fn new(area: &'a [u8]) -> Self {
        Records {
            area,
            cursor: 0,
            error: None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = Rec<'a>;

    fn next(&mut self) -> Option<Rec<'a>> {
        let end = self.area.len();
        if self.cursor >= end {
            return None;
        }
        if end - self.cursor < RECORD_HEADER {
            self.error = Some(format!(
                "truncated record header at {}",
                HEADER_BYTES + self.cursor
            ));
            self.cursor = end;
            return None;
        }
        let kind = u16_at(self.area, self.cursor + R_KIND);
        let size = u16_at(self.area, self.cursor + R_SIZE) as usize;
        if size < RECORD_HEADER || !size.is_multiple_of(RECORD_ALIGN) || self.cursor + size > end {
            self.error = Some(format!(
                "bad record size {size} (kind {kind:#06x}) at {}",
                HEADER_BYTES + self.cursor
            ));
            self.cursor = end;
            return None;
        }
        let aux = u32_at(self.area, self.cursor + R_AUX);
        let bytes = &self.area[self.cursor..self.cursor + size];
        self.cursor += size;
        Some(Rec { kind, aux, bytes })
    }
}

/// Appends records to a record area (`Buf::area_mut`). Reserved records are zero-filled, so
/// padding and reserved fields never leak bytes from an earlier hand-over.
pub struct RecordWriter<'a> {
    area: &'a mut [u8],
    cursor: usize,
    count: u32,
}

impl<'a> RecordWriter<'a> {
    pub fn new(area: &'a mut [u8]) -> Self {
        RecordWriter {
            area,
            cursor: 0,
            count: 0,
        }
    }

    pub fn used(&self) -> usize {
        self.cursor
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn remaining(&self) -> usize {
        self.area.len() - self.cursor
    }

    pub fn capacity(&self) -> usize {
        self.area.len()
    }

    /// Reserves `size` bytes (aligned up) for a record and writes its header. Returns the record
    /// as a mutable slice, or `None` when it does not fit (nothing is written).
    pub fn reserve(&mut self, kind: u16, size: usize, aux: u32) -> Option<&mut [u8]> {
        let size = align8(size);
        assert!(
            (RECORD_HEADER..=MAX_RECORD).contains(&size),
            "bad record size {size} for kind {kind:#06x}"
        );
        if self.cursor + size > self.area.len() {
            return None;
        }
        let at = self.cursor;
        let rec = &mut self.area[at..at + size];
        rec.fill(0);
        put_u16(rec, R_KIND, kind);
        put_u16(rec, R_SIZE, size as u16);
        put_u32(rec, R_AUX, aux);
        self.cursor += size;
        self.count += 1;
        Some(&mut self.area[at..at + size])
    }

    /// Appends an already encoded record.
    pub fn copy(&mut self, record: &[u8]) -> bool {
        if self.cursor + record.len() > self.area.len() {
            return false;
        }
        self.area[self.cursor..self.cursor + record.len()].copy_from_slice(record);
        self.cursor += record.len();
        self.count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_and_reader_round_trip() {
        let mut area = vec![0xAAu8; 256];
        let mut w = RecordWriter::new(&mut area);
        let r = w.reserve(obs::TICK, obs::tick::SIZE, 0).unwrap();
        put_i64(r, obs::tick::GAME_TIME, 1234);
        let r = w.reserve(obs::DICT, obs::dict::NAME + 3, 2).unwrap();
        assert_eq!(r.len(), 24, "aligned to 8");
        put_u32(r, obs::dict::ID, 1);
        put_u32(r, obs::dict::NAME_LEN, 3);
        r[obs::dict::NAME..obs::dict::NAME + 3].copy_from_slice(b"abc");
        assert!(w.reserve(obs::TICK, 300, 0).is_none(), "does not fit");
        let used = w.used();
        assert_eq!((used, w.count()), (40, 2));
        let recs: Vec<_> = Records::new(&area[..used]).collect();
        assert_eq!(recs.len(), 2);
        assert_eq!(i64_at(recs[0].bytes, obs::tick::GAME_TIME), 1234);
        assert_eq!(
            (recs[1].kind, recs[1].aux, recs[1].size()),
            (obs::DICT, 2, 24)
        );
        assert_eq!(
            recs[1].bytes[obs::dict::NAME + 3..],
            [0u8; 5],
            "padding zeroed"
        );
    }

    #[test]
    fn a_bad_size_ends_the_walk() {
        let mut area = vec![0u8; 32];
        put_u16(&mut area, R_KIND, obs::TICK);
        put_u16(&mut area, R_SIZE, 12);
        let mut it = Records::new(&area);
        assert!(it.next().is_none());
        assert!(it.error().unwrap().contains("bad record size 12"));
    }
}
