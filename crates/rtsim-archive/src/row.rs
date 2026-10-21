// SPDX-License-Identifier: AGPL-3.0-or-later
//! The two 32-byte rows (VIEWER.md §10.2), little-endian like the kernel's codec.
//!
//! | Bytes | Event row | Leg row |
//! |---|---|---|
//! | 0..6 | tick (48 bits) | t0 (48 bits) |
//! | 6..8 | kind | kind |
//! | 8 | frame | frame |
//! | 9 | severity and flags | severity and flags ([`LEG`] set) |
//! | 10..16 | x, z (24 bits each) | x0, z0 |
//! | 16..22 | subject (48 bits) | x1, z1 |
//! | 22..28 | object (48 bits) | subject (48 bits) |
//! | 28..32 | magnitude (i32) | duration in ticks (u32) |
//!
//! Byte 9: bits 0 to 2 the severity, then [`LEG`], [`SUBJECT_DICT`], [`OBJECT_DICT`], [`CLAMPED`] and
//! [`PAYLOAD`]. A leg's severity is its mover's: a named mover's legs (dragons, war bands, named
//! ships) are [`Severity::Notable`], so the notable tier's filter is one comparison for both rows.

/// A row's size.
pub const ROW_BYTES: usize = 32;
/// A row as stored.
pub type RawRow = [u8; ROW_BYTES];

/// Bits 0 to 2 of byte 9.
pub const SEVERITY_MASK: u8 = 0b111;
/// The row is a leg row.
pub const LEG: u8 = 1 << 3;
/// The subject is an entry in the id dictionary, not the entity's own id.
pub const SUBJECT_DICT: u8 = 1 << 4;
/// The object is an entry in the id dictionary (event rows).
pub const OBJECT_DICT: u8 = 1 << 5;
/// A coordinate, tick or duration was out of range and clamped.
pub const CLAMPED: u8 = 1 << 6;
/// The chunk's side block holds a payload for this row (notable and above only).
pub const PAYLOAD: u8 = 1 << 7;

/// The largest coordinate a row holds; further out is clamped and flagged.
pub const MAX_COORD: i32 = (1 << 23) - 1;
/// The largest tick or id a row holds.
pub const MAX_48: u64 = (1 << 48) - 1;

/// How much an event matters. The pending buffer drops the lowest first; the notable tier keeps
/// [`Severity::Notable`] and up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Severity {
    /// Kept only in the full tier: a trade, a caravan arriving, a routine leg.
    #[default]
    Routine = 0,
    Minor = 1,
    /// Kept for the notable tier's horizon; may carry a payload.
    Notable = 2,
    /// A war, a raid on a district, a dragon's death.
    Major = 3,
}

impl Severity {
    /// The severity of byte 9's low bits; values past [`Severity::Major`] read as major.
    pub fn from_bits(b: u8) -> Severity {
        match b & SEVERITY_MASK {
            0 => Severity::Routine,
            1 => Severity::Minor,
            2 => Severity::Notable,
            _ => Severity::Major,
        }
    }
}

/// A 48-bit archive id: the entity's own id when it fits, otherwise an index into the archive's id
/// dictionary ([`crate::dict::Dictionary::handle`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Handle {
    pub value: u64,
    pub dict: bool,
}

impl Handle {
    /// An id that fits in 48 bits, as itself.
    pub fn direct(value: u64) -> Handle {
        debug_assert!(value <= MAX_48, "use the dictionary for {value:#x}");
        Handle {
            value: value & MAX_48,
            dict: false,
        }
    }
}

/// Something that happened at a place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub tick: u64,
    pub kind: u16,
    pub frame: u8,
    pub severity: Severity,
    pub x: i32,
    pub z: i32,
    pub subject: Handle,
    pub object: Handle,
    /// Goods, units or damage, by kind.
    pub magnitude: i32,
    /// The chunk holds a payload for this row.
    pub payload: bool,
    /// Something was clamped when the row was written.
    pub clamped: bool,
}

/// A straight piece of a mover's route: at `t0` at (x0, z0), at `t0 + duration` at (x1, z1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Leg {
    pub t0: u64,
    pub kind: u16,
    pub frame: u8,
    pub severity: Severity,
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub subject: Handle,
    pub duration: u32,
    pub clamped: bool,
}

impl Leg {
    /// The tick the leg ends.
    pub fn end(&self) -> u64 {
        self.t0 + u64::from(self.duration)
    }

    /// Whether the mover is on this leg at `tick` (both ends included).
    pub fn covers(&self, tick: u64) -> bool {
        (self.t0..=self.end()).contains(&tick)
    }

    /// The mover's position at `tick`, clamped to the leg's ends.
    pub fn at(&self, tick: u64) -> (f64, f64) {
        let k = if self.duration == 0 {
            1.0
        } else {
            (tick.saturating_sub(self.t0) as f64 / f64::from(self.duration)).min(1.0)
        };
        let lerp = |a: i32, b: i32| f64::from(a) + (f64::from(b) - f64::from(a)) * k;
        (lerp(self.x0, self.x1), lerp(self.z0, self.z1))
    }
}

/// Either row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Event(Event),
    Leg(Leg),
}

fn put48(out: &mut [u8], v: u64) {
    out[..6].copy_from_slice(&v.to_le_bytes()[..6]);
}

fn get48(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a[..6].copy_from_slice(&b[..6]);
    u64::from_le_bytes(a)
}

fn put24(out: &mut [u8], v: i32) {
    out[..3].copy_from_slice(&v.to_le_bytes()[..3]);
}

fn get24(b: &[u8]) -> i32 {
    // Sign-extend from bit 23.
    (i32::from_le_bytes([b[0], b[1], b[2], 0]) << 8) >> 8
}

/// A coordinate in range, and whether it had to move.
fn coord(v: i32, clamped: &mut bool) -> i32 {
    let c = v.clamp(-MAX_COORD, MAX_COORD);
    *clamped |= c != v;
    c
}

fn tick48(v: u64, clamped: &mut bool) -> u64 {
    *clamped |= v > MAX_48;
    v.min(MAX_48)
}

impl Row {
    /// The row's 32 bytes. Coordinates past ±[`MAX_COORD`] and ticks past 48 bits are clamped and
    /// the row flagged [`CLAMPED`].
    pub fn encode(&self) -> RawRow {
        let mut b = [0u8; ROW_BYTES];
        let mut clamped = false;
        match self {
            Row::Event(e) => {
                put48(&mut b[0..], tick48(e.tick, &mut clamped));
                b[6..8].copy_from_slice(&e.kind.to_le_bytes());
                b[8] = e.frame;
                put24(&mut b[10..], coord(e.x, &mut clamped));
                put24(&mut b[13..], coord(e.z, &mut clamped));
                put48(&mut b[16..], e.subject.value & MAX_48);
                put48(&mut b[22..], e.object.value & MAX_48);
                b[28..32].copy_from_slice(&e.magnitude.to_le_bytes());
                let mut f = e.severity as u8;
                f |= if e.subject.dict { SUBJECT_DICT } else { 0 };
                f |= if e.object.dict { OBJECT_DICT } else { 0 };
                f |= if e.payload { PAYLOAD } else { 0 };
                f |= if e.clamped || clamped { CLAMPED } else { 0 };
                b[9] = f;
            }
            Row::Leg(l) => {
                put48(&mut b[0..], tick48(l.t0, &mut clamped));
                b[6..8].copy_from_slice(&l.kind.to_le_bytes());
                b[8] = l.frame;
                put24(&mut b[10..], coord(l.x0, &mut clamped));
                put24(&mut b[13..], coord(l.z0, &mut clamped));
                put24(&mut b[16..], coord(l.x1, &mut clamped));
                put24(&mut b[19..], coord(l.z1, &mut clamped));
                put48(&mut b[22..], l.subject.value & MAX_48);
                b[28..32].copy_from_slice(&l.duration.to_le_bytes());
                let mut f = l.severity as u8 | LEG;
                f |= if l.subject.dict { SUBJECT_DICT } else { 0 };
                f |= if l.clamped || clamped { CLAMPED } else { 0 };
                b[9] = f;
            }
        }
        b
    }

    /// The row a stored row holds. Every 32 bytes decode to some row: the format has no invalid
    /// values, so a chunk's checksum is what catches damage.
    pub fn decode(b: &RawRow) -> Row {
        let f = b[9];
        let kind = u16::from_le_bytes([b[6], b[7]]);
        let severity = Severity::from_bits(f);
        let clamped = f & CLAMPED != 0;
        if f & LEG != 0 {
            Row::Leg(Leg {
                t0: get48(&b[0..]),
                kind,
                frame: b[8],
                severity,
                x0: get24(&b[10..]),
                z0: get24(&b[13..]),
                x1: get24(&b[16..]),
                z1: get24(&b[19..]),
                subject: Handle {
                    value: get48(&b[22..]),
                    dict: f & SUBJECT_DICT != 0,
                },
                duration: u32::from_le_bytes([b[28], b[29], b[30], b[31]]),
                clamped,
            })
        } else {
            Row::Event(Event {
                tick: get48(&b[0..]),
                kind,
                frame: b[8],
                severity,
                x: get24(&b[10..]),
                z: get24(&b[13..]),
                subject: Handle {
                    value: get48(&b[16..]),
                    dict: f & SUBJECT_DICT != 0,
                },
                object: Handle {
                    value: get48(&b[22..]),
                    dict: f & OBJECT_DICT != 0,
                },
                magnitude: i32::from_le_bytes([b[28], b[29], b[30], b[31]]),
                payload: f & PAYLOAD != 0,
                clamped,
            })
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            Row::Event(e) => e.severity,
            Row::Leg(l) => l.severity,
        }
    }

    pub fn kind(&self) -> u16 {
        match self {
            Row::Event(e) => e.kind,
            Row::Leg(l) => l.kind,
        }
    }
}

/// A stored row's tick (an event's tick, a leg's start), without decoding the rest.
pub fn tick(b: &RawRow) -> u64 {
    get48(b)
}

/// A stored row's last tick: a leg's end, an event's tick.
pub fn end(b: &RawRow) -> u64 {
    if b[9] & LEG != 0 {
        get48(b) + u64::from(u32::from_le_bytes([b[28], b[29], b[30], b[31]]))
    } else {
        get48(b)
    }
}

/// A stored row's kind.
pub fn kind(b: &RawRow) -> u16 {
    u16::from_le_bytes([b[6], b[7]])
}

/// A stored row's frame.
pub fn frame(b: &RawRow) -> u8 {
    b[8]
}

/// A stored row's severity.
pub fn severity(b: &RawRow) -> Severity {
    Severity::from_bits(b[9])
}

/// A stored row's flags byte.
pub fn flags(b: &RawRow) -> u8 {
    b[9]
}

/// The blocks a stored row touches: an event's point, both ends of a leg.
pub fn points(b: &RawRow) -> [(i32, i32); 2] {
    let p = (get24(&b[10..]), get24(&b[13..]));
    if b[9] & LEG != 0 {
        [p, (get24(&b[16..]), get24(&b[19..]))]
    } else {
        [p, p]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_and_legs_read_back_as_written() {
        let e = Event {
            tick: MAX_48,
            kind: 0xBEEF,
            frame: 254,
            severity: Severity::Major,
            x: -MAX_COORD,
            z: MAX_COORD,
            subject: Handle {
                value: MAX_48,
                dict: true,
            },
            object: Handle::direct(7),
            magnitude: i32::MIN,
            payload: true,
            clamped: false,
        };
        let raw = Row::Event(e).encode();
        assert_eq!(Row::decode(&raw), Row::Event(e));
        assert_eq!(
            (tick(&raw), end(&raw), kind(&raw), frame(&raw)),
            (MAX_48, MAX_48, 0xBEEF, 254)
        );
        assert_eq!(severity(&raw), Severity::Major);
        let l = Leg {
            t0: 1_000,
            kind: 3,
            frame: 1,
            severity: Severity::Notable,
            x0: -5,
            z0: 6,
            x1: 1_000_000,
            z1: -1,
            subject: Handle {
                value: 12,
                dict: true,
            },
            duration: u32::MAX,
            clamped: false,
        };
        let raw = Row::Leg(l).encode();
        assert_eq!(Row::decode(&raw), Row::Leg(l));
        assert_eq!(end(&raw), 1_000 + u64::from(u32::MAX));
        assert_eq!(points(&raw), [(-5, 6), (1_000_000, -1)]);
        assert_ne!(flags(&raw) & LEG, 0);
    }

    #[test]
    fn out_of_range_values_are_clamped_and_flagged() {
        let e = Event {
            tick: MAX_48 + 5,
            x: i32::MAX,
            z: i32::MIN,
            ..Event::default()
        };
        let Row::Event(back) = Row::decode(&Row::Event(e).encode()) else {
            panic!()
        };
        assert_eq!(
            (back.tick, back.x, back.z, back.clamped),
            (MAX_48, MAX_COORD, -MAX_COORD, true)
        );
        let l = Leg {
            x1: 9_000_000,
            ..Leg::default()
        };
        let Row::Leg(back) = Row::decode(&Row::Leg(l).encode()) else {
            panic!()
        };
        assert_eq!((back.x1, back.clamped), (MAX_COORD, true));
        // In range: not flagged.
        let Row::Event(back) = Row::decode(
            &Row::Event(Event {
                x: -1,
                ..Event::default()
            })
            .encode(),
        ) else {
            panic!()
        };
        assert!(!back.clamped);
    }

    #[test]
    fn a_leg_places_its_mover_between_its_ends() {
        let l = Leg {
            t0: 100,
            duration: 50,
            x0: 0,
            z0: 10,
            x1: 100,
            z1: -10,
            ..Leg::default()
        };
        assert_eq!(l.at(100), (0.0, 10.0));
        assert_eq!(l.at(125), (50.0, 0.0));
        assert_eq!(l.at(150), (100.0, -10.0));
        assert_eq!(l.at(10_000), (100.0, -10.0));
        assert_eq!(l.at(0), (0.0, 10.0));
        assert!(l.covers(100) && l.covers(150) && !l.covers(151) && !l.covers(99));
        let still = Leg {
            t0: 5,
            x1: 3,
            ..Leg::default()
        };
        assert_eq!(still.at(5), (3.0, 0.0));
    }
}
