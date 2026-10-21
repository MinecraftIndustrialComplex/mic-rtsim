// SPDX-License-Identifier: AGPL-3.0-or-later
//! The count tier's rows (VIEWER.md §10.4): per kind, per 512-block region, per game day, how many
//! rows there were and the sum of their magnitudes, in 16 bytes. Past the tier's share they are
//! coarsened to per 4,096-block square per 7 game days, and then the oldest go. The page draws them
//! as heat where the full rows are gone (§10.6).
//!
//! | Bytes | Count row |
//! |---|---|
//! | 0..3 | the bucket's first game day (23 bits); bit 23 set for a coarse row |
//! | 3..5 | kind |
//! | 5 | frame |
//! | 6..8 | region x (i16: blocks / 512, or / 4,096 when coarse, rounded down) |
//! | 8..10 | region z |
//! | 10..12 | rows counted (u16; more go in a second row with the same key) |
//! | 12..16 | their magnitudes' sum (i32, saturating) |
//!
//! An event is counted at its place and day; a leg at its start. Each row is counted once: the
//! full tier's rows below notable are counted when they leave it, the notable tier's when they
//! leave that.

use std::collections::BTreeMap;

use crate::row::{self, LEG, RawRow};

/// A count row's size.
pub const COUNT_BYTES: usize = 16;
pub type RawCount = [u8; COUNT_BYTES];

/// A game day in ticks.
pub const TICKS_PER_DAY: u64 = 24_000;
/// A fine count's region side in blocks.
pub const REGION_BLOCKS: i32 = 512;
/// A coarse count's square side in blocks.
pub const COARSE_BLOCKS: i32 = 4096;
/// A coarse count's span in game days.
pub const COARSE_DAYS: u32 = 7;
/// The largest day a count row holds (23 bits, about 23,000 game years).
pub const MAX_DAY: u32 = (1 << 23) - 1;
const COARSE_BIT: u32 = 1 << 23;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Count {
    /// The bucket's first game day (a multiple of [`COARSE_DAYS`] when coarse).
    pub day: u32,
    /// Per 4,096-block square per 7 days rather than per 512-block region per day.
    pub coarse: bool,
    pub kind: u16,
    pub frame: u8,
    /// The region (or square) in units of its side.
    pub rx: i16,
    pub rz: i16,
    pub count: u16,
    pub magnitude: i32,
}

/// What groups count rows: everything but the count and the sum.
pub type Key = (u32, bool, u8, u16, i16, i16);

impl Count {
    pub fn encode(&self) -> RawCount {
        let mut b = [0u8; COUNT_BYTES];
        let day = self.day.min(MAX_DAY) | if self.coarse { COARSE_BIT } else { 0 };
        b[0..3].copy_from_slice(&day.to_le_bytes()[..3]);
        b[3..5].copy_from_slice(&self.kind.to_le_bytes());
        b[5] = self.frame;
        b[6..8].copy_from_slice(&self.rx.to_le_bytes());
        b[8..10].copy_from_slice(&self.rz.to_le_bytes());
        b[10..12].copy_from_slice(&self.count.to_le_bytes());
        b[12..16].copy_from_slice(&self.magnitude.to_le_bytes());
        b
    }

    /// Every 16 bytes decode to some count row; a chunk's checksum is what catches damage.
    pub fn decode(b: &RawCount) -> Count {
        let day = u32::from_le_bytes([b[0], b[1], b[2], 0]);
        Count {
            day: day & MAX_DAY,
            coarse: day & COARSE_BIT != 0,
            kind: u16::from_le_bytes([b[3], b[4]]),
            frame: b[5],
            rx: i16::from_le_bytes([b[6], b[7]]),
            rz: i16::from_le_bytes([b[8], b[9]]),
            count: u16::from_le_bytes([b[10], b[11]]),
            magnitude: i32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        }
    }

    pub fn key(&self) -> Key {
        (
            self.day,
            self.coarse,
            self.frame,
            self.kind,
            self.rx,
            self.rz,
        )
    }

    /// The bucket's side in blocks.
    pub fn side(&self) -> i32 {
        if self.coarse {
            COARSE_BLOCKS
        } else {
            REGION_BLOCKS
        }
    }

    /// The bucket's span in game days.
    pub fn days(&self) -> u32 {
        if self.coarse { COARSE_DAYS } else { 1 }
    }

    /// The bucket's first tick.
    pub fn first_tick(&self) -> u64 {
        u64::from(self.day) * TICKS_PER_DAY
    }

    /// The bucket's last tick.
    pub fn last_tick(&self) -> u64 {
        (u64::from(self.day) + u64::from(self.days())) * TICKS_PER_DAY - 1
    }

    /// Whether the bucket touches a tick in `t0..=t1`.
    pub fn overlaps(&self, t0: u64, t1: u64) -> bool {
        self.first_tick() <= t1 && self.last_tick() >= t0
    }

    /// min x, min z, max x, max z of the bucket's blocks.
    pub fn bbox(&self) -> [i32; 4] {
        let s = self.side();
        let (x, z) = (i32::from(self.rx) * s, i32::from(self.rz) * s);
        [x, z, x.saturating_add(s - 1), z.saturating_add(s - 1)]
    }
}

/// Sums by key, splitting a count past a row's u16 into further rows with the same key.
#[derive(Debug, Default)]
pub struct Tally {
    sums: BTreeMap<Key, (u64, i64)>,
}

impl Tally {
    pub fn new() -> Tally {
        Tally::default()
    }

    pub fn is_empty(&self) -> bool {
        self.sums.is_empty()
    }

    /// Counts an archive row (an event at its place and day, a leg at its start).
    pub fn row(&mut self, r: &RawRow) {
        let [(x, z), _] = row::points(r);
        let day = (row::tick(r) / TICKS_PER_DAY).min(u64::from(MAX_DAY)) as u32;
        let magnitude = if row::flags(r) & LEG != 0 {
            0
        } else {
            i64::from(i32::from_le_bytes([r[28], r[29], r[30], r[31]]))
        };
        let key = (
            day,
            false,
            row::frame(r),
            row::kind(r),
            region(x, REGION_BLOCKS),
            region(z, REGION_BLOCKS),
        );
        self.add(key, 1, magnitude);
    }

    /// Adds a count row, coarsened when `coarse` (a coarse row stays as it is).
    pub fn count(&mut self, c: &Count, coarse: bool) {
        let key = if coarse && !c.coarse {
            let per = COARSE_BLOCKS / REGION_BLOCKS;
            (
                c.day - c.day % COARSE_DAYS,
                true,
                c.frame,
                c.kind,
                (i32::from(c.rx).div_euclid(per)) as i16,
                (i32::from(c.rz).div_euclid(per)) as i16,
            )
        } else {
            c.key()
        };
        self.add(key, u64::from(c.count), i64::from(c.magnitude));
    }

    fn add(&mut self, key: Key, n: u64, magnitude: i64) {
        let s = self.sums.entry(key).or_default();
        s.0 += n;
        s.1 = s.1.saturating_add(magnitude);
    }

    /// The rows, ascending by key (day first).
    pub fn rows(self) -> Vec<Count> {
        let mut out = Vec::with_capacity(self.sums.len());
        for ((day, coarse, frame, kind, rx, rz), (mut n, mut m)) in self.sums {
            while n > 0 {
                let c = n.min(u64::from(u16::MAX));
                let part = m.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
                out.push(Count {
                    day,
                    coarse,
                    kind,
                    frame,
                    rx,
                    rz,
                    count: c as u16,
                    magnitude: part as i32,
                });
                n -= c;
                m -= part;
            }
        }
        out
    }
}

/// A block coordinate's region of `side` blocks, rounded down.
pub fn region(v: i32, side: i32) -> i16 {
    v.div_euclid(side)
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{Event, Leg, MAX_COORD, Row};

    #[test]
    fn a_count_row_reads_back() {
        let c = Count {
            day: MAX_DAY,
            coarse: true,
            kind: 0xBEEF,
            frame: 254,
            rx: i16::MIN,
            rz: i16::MAX,
            count: u16::MAX,
            magnitude: i32::MIN,
        };
        assert_eq!(Count::decode(&c.encode()), c);
        let fine = Count {
            day: 3,
            rx: -1,
            ..Count::default()
        };
        assert_eq!(Count::decode(&fine.encode()), fine);
        assert_eq!(fine.bbox(), [-512, 0, -1, 511]);
        assert_eq!((fine.first_tick(), fine.last_tick()), (72_000, 96_000 - 1));
        assert_eq!(region(-MAX_COORD, REGION_BLOCKS), -16_384);
        assert_eq!(region(MAX_COORD, REGION_BLOCKS), 16_383);
    }

    #[test]
    fn rows_are_counted_then_coarsened() {
        let ev = |tick, x, m| {
            Row::Event(Event {
                tick,
                kind: 2,
                x,
                z: 10,
                magnitude: m,
                ..Event::default()
            })
            .encode()
        };
        let mut t = Tally::new();
        // Two in region 0 on day 0, one in region -1, one on day 8.
        for r in [
            ev(5, 3, 10),
            ev(23_999, 511, 5),
            ev(100, -1, 7),
            ev(8 * 24_000, 0, 1),
        ] {
            t.row(&r);
        }
        t.row(
            &Row::Leg(Leg {
                t0: 24_001,
                kind: 3,
                x0: 600,
                z0: 0,
                x1: 9_000,
                z1: 0,
                duration: 50_000,
                ..Leg::default()
            })
            .encode(),
        );
        let fine = t.rows();
        let got: Vec<_> = fine
            .iter()
            .map(|c| (c.day, c.kind, c.rx, c.count, c.magnitude))
            .collect();
        assert_eq!(
            got,
            [
                (0, 2, -1, 1, 7),
                (0, 2, 0, 2, 15),
                (1, 3, 1, 1, 0),
                (8, 2, 0, 1, 1)
            ]
        );
        let mut c = Tally::new();
        for r in &fine {
            c.count(r, true);
        }
        let coarse = c.rows();
        let got: Vec<_> = coarse
            .iter()
            .map(|c| (c.day, c.coarse, c.kind, c.rx, c.count, c.magnitude))
            .collect();
        assert_eq!(
            got,
            [
                (0, true, 2, -1, 1, 7),
                (0, true, 2, 0, 2, 15),
                (0, true, 3, 0, 1, 0),
                (7, true, 2, 0, 1, 1)
            ]
        );
        assert_eq!(coarse[0].bbox(), [-4096, 0, -1, 4095]);
        assert_eq!(coarse[3].last_tick(), 14 * 24_000 - 1);
    }

    #[test]
    fn a_big_count_splits_and_sums_saturate() {
        let mut t = Tally::new();
        let c = Count {
            count: 60_000,
            magnitude: i32::MAX,
            ..Count::default()
        };
        t.count(&c, false);
        t.count(&c, false);
        let rows = t.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().map(|r| u64::from(r.count)).sum::<u64>(),
            120_000
        );
        assert_eq!(
            rows.iter().map(|r| i64::from(r.magnitude)).sum::<i64>(),
            2 * i64::from(i32::MAX)
        );
    }
}
