// SPDX-License-Identifier: AGPL-3.0-or-later
//! The pending buffer (VIEWER.md §10.3): in the step, an archived event is a push of 32 bytes. The
//! buffer is capped; past the cap the lowest-severity rows are dropped and counted, and the step
//! never waits. At each save the buffer is swapped for an empty one ([`Pending::take`]).

use crate::row::{PAYLOAD, ROW_BYTES, RawRow, Row, Severity, severity};

/// Rows the buffer holds at most (TUNABLE).
pub const MAX_PENDING_ROWS: usize = 32_768;
/// Bytes the buffer holds at most, rows and payloads together (TUNABLE).
pub const MAX_PENDING_BYTES: usize = 1 << 20;
/// A payload larger than this is dropped (its row is kept) and counted (TUNABLE).
pub const MAX_PAYLOAD_BYTES: usize = 16 << 10;

/// What the archive could not keep, counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dropped {
    /// Rows dropped past the buffer's cap, by severity.
    pub by_severity: [u64; 4],
    /// Rows whose dimension had no frame number left (past 255 frames).
    pub no_frame: u64,
    /// Payloads dropped: on a row below notable, or too large. Their rows were kept.
    pub payloads: u64,
}

impl Dropped {
    pub fn add(&mut self, o: &Dropped) {
        for (a, b) in self.by_severity.iter_mut().zip(o.by_severity) {
            *a += b;
        }
        self.no_frame += o.no_frame;
        self.payloads += o.payloads;
    }

    /// Rows dropped for any reason.
    pub fn rows(&self) -> u64 {
        self.by_severity.iter().sum::<u64>() + self.no_frame
    }
}

/// The rows of one save, handed to the IO lane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    /// In push order.
    pub rows: Vec<RawRow>,
    /// Row index and bytes, ascending by index.
    pub payloads: Vec<(u32, Vec<u8>)>,
    /// What was dropped since the previous batch.
    pub dropped: Dropped,
}

#[derive(Debug)]
pub struct Pending {
    rows: Vec<RawRow>,
    payloads: Vec<(u32, Vec<u8>)>,
    payload_bytes: usize,
    by_severity: [usize; 4],
    dropped: Dropped,
    max_rows: usize,
    max_bytes: usize,
}

impl Default for Pending {
    fn default() -> Self {
        Pending::with_caps(MAX_PENDING_ROWS, MAX_PENDING_BYTES)
    }
}

impl Pending {
    pub fn new() -> Pending {
        Pending::default()
    }

    /// A buffer with other caps (at least 8 rows).
    pub fn with_caps(max_rows: usize, max_bytes: usize) -> Pending {
        let max_rows = max_rows.max(8);
        Pending {
            rows: Vec::new(),
            payloads: Vec::new(),
            payload_bytes: 0,
            by_severity: [0; 4],
            dropped: Dropped::default(),
            max_rows,
            max_bytes: max_bytes.max(max_rows.min(8) * ROW_BYTES),
        }
    }

    pub fn push(&mut self, row: &Row) {
        self.push_raw(row.encode(), None);
    }

    /// A row with a payload (a raid's loot by good). Only rows at notable severity and above keep
    /// one (§10.2); otherwise, or when it is over [`MAX_PAYLOAD_BYTES`], the payload is dropped and
    /// counted and the row kept.
    pub fn push_with_payload(&mut self, row: &Row, payload: Vec<u8>) {
        self.push_raw(row.encode(), Some(payload));
    }

    pub fn push_raw(&mut self, mut raw: RawRow, mut payload: Option<Vec<u8>>) {
        let sev = severity(&raw);
        if payload
            .as_ref()
            .is_some_and(|p| sev < Severity::Notable || p.len() > MAX_PAYLOAD_BYTES)
        {
            payload = None;
            self.dropped.payloads += 1;
        }
        let extra = payload.as_ref().map_or(0, Vec::len);
        if !self.admit(sev, extra) {
            self.dropped.by_severity[sev as usize] += 1;
            return;
        }
        raw[9] &= !PAYLOAD;
        if let Some(p) = payload {
            raw[9] |= PAYLOAD;
            self.payload_bytes += p.len();
            self.payloads.push((self.rows.len() as u32, p));
        }
        self.by_severity[sev as usize] += 1;
        self.rows.push(raw);
    }

    /// Counts a row dropped because its dimension had no frame number left.
    pub fn no_frame(&mut self) {
        self.dropped.no_frame += 1;
    }

    /// Everything pushed since the last take, leaving the buffer empty.
    pub fn take(&mut self) -> Batch {
        self.payload_bytes = 0;
        self.by_severity = [0; 4];
        Batch {
            rows: std::mem::take(&mut self.rows),
            payloads: std::mem::take(&mut self.payloads),
            dropped: std::mem::take(&mut self.dropped),
        }
    }

    /// The rows waiting for the next save, in push order (the live tap publishes them, §10.3).
    pub fn rows(&self) -> &[RawRow] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Bytes held, rows and payloads.
    pub fn bytes(&self) -> usize {
        self.rows.len() * ROW_BYTES + self.payload_bytes
    }

    /// What was dropped since the last take.
    pub fn dropped(&self) -> &Dropped {
        &self.dropped
    }

    fn fits(&self, rows: usize, bytes: usize) -> bool {
        rows <= self.max_rows && bytes <= self.max_bytes
    }

    /// Whether a row of severity `sev` with `extra` payload bytes may go in, making room if it
    /// must. Room is made an eighth of the cap at a time, so a full buffer costs one pass per
    /// eighth rather than one per push.
    fn admit(&mut self, sev: Severity, extra: usize) -> bool {
        if self.fits(self.rows.len() + 1, self.bytes() + ROW_BYTES + extra) {
            return true;
        }
        match self.by_severity.iter().position(|&n| n > 0) {
            // Everything held matters more than this row.
            Some(lowest) if (sev as usize) < lowest => return false,
            None => return false,
            Some(_) => {}
        }
        let target_rows = self.max_rows - self.max_rows / 8;
        let target_bytes = (self.max_bytes - self.max_bytes / 8).saturating_sub(ROW_BYTES + extra);
        self.shed(target_rows.min(self.max_rows - 1), target_bytes, sev);
        self.fits(self.rows.len() + 1, self.bytes() + ROW_BYTES + extra)
    }

    /// Drops rows, the lowest severity first and the oldest first within it, never above `upto`,
    /// until at most `rows` rows and `bytes` bytes are held.
    fn shed(&mut self, rows: usize, bytes: usize, upto: Severity) {
        let mut gone = vec![false; self.rows.len()];
        let (mut n, mut b) = (self.rows.len(), self.bytes());
        let payload_len = |payloads: &[(u32, Vec<u8>)], i: usize| {
            payloads
                .binary_search_by_key(&(i as u32), |p| p.0)
                .map_or(0, |j| payloads[j].1.len())
        };
        'levels: for s in 0..=upto as usize {
            if self.by_severity[s] == 0 {
                continue;
            }
            for (i, r) in self.rows.iter().enumerate() {
                if n <= rows && b <= bytes {
                    break 'levels;
                }
                if severity(r) as usize == s {
                    gone[i] = true;
                    n -= 1;
                    b -= ROW_BYTES + payload_len(&self.payloads, i);
                    self.by_severity[s] -= 1;
                    self.dropped.by_severity[s] += 1;
                }
            }
        }
        let mut new_of = vec![u32::MAX; self.rows.len()];
        let mut k = 0u32;
        for (i, g) in gone.iter().enumerate() {
            if !g {
                new_of[i] = k;
                k += 1;
            }
        }
        let mut i = 0;
        self.rows.retain(|_| {
            i += 1;
            !gone[i - 1]
        });
        let mut freed = 0;
        self.payloads.retain_mut(|p| {
            let new = new_of[p.0 as usize];
            if new == u32::MAX {
                freed += p.1.len();
                return false;
            }
            p.0 = new;
            true
        });
        self.payload_bytes -= freed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{Event, PAYLOAD, flags, tick};

    fn ev(t: u64, s: Severity) -> Row {
        Row::Event(Event {
            tick: t,
            severity: s,
            ..Event::default()
        })
    }

    #[test]
    fn pushes_and_takes() {
        let mut p = Pending::new();
        p.push(&ev(1, Severity::Routine));
        p.push_with_payload(&ev(2, Severity::Notable), b"12 iron".to_vec());
        p.push_with_payload(&ev(3, Severity::Minor), b"too low".to_vec());
        assert_eq!((p.len(), p.bytes()), (3, 3 * 32 + 7));
        let b = p.take();
        assert!(p.is_empty() && p.bytes() == 0);
        assert_eq!(b.rows.len(), 3);
        assert_eq!(b.payloads, [(1, b"12 iron".to_vec())]);
        assert_ne!(flags(&b.rows[1]) & PAYLOAD, 0);
        assert_eq!(flags(&b.rows[2]) & PAYLOAD, 0);
        assert_eq!(b.dropped.payloads, 1);
        assert_eq!(p.take().dropped, Dropped::default());
    }

    #[test]
    fn past_the_cap_the_lowest_and_oldest_go_first() {
        let mut p = Pending::with_caps(16, 1 << 20);
        for t in 0..8 {
            p.push(&ev(t, Severity::Routine));
        }
        for t in 8..16 {
            p.push(&ev(t, Severity::Notable));
        }
        assert_eq!(p.len(), 16);
        // Full: a minor row sheds the two oldest routine rows (to 14 held), then goes in.
        p.push(&ev(16, Severity::Minor));
        assert_eq!(p.len(), 15);
        assert_eq!(p.dropped().by_severity, [2, 0, 0, 0]);
        assert_eq!(tick(&p.rows()[0]), 2);
        // Fill with major rows: the routine rows go, then the minor, then the oldest notables.
        for t in 100..121 {
            p.push(&ev(t, Severity::Major));
        }
        assert_eq!(p.len(), 16);
        let held: Vec<_> = p.rows().iter().map(severity).collect();
        assert!(held.iter().all(|&s| s >= Severity::Notable), "{held:?}");
        assert_eq!(p.dropped().by_severity[0], 8);
        assert_eq!(p.dropped().by_severity[1], 1);
        // A routine row cannot push out rows that matter more.
        let before = p.dropped().by_severity;
        p.push(&ev(200, Severity::Routine));
        assert_eq!(p.dropped().by_severity[0], before[0] + 1);
        assert!(p.rows().iter().all(|r| tick(r) != 200));
        let b = p.take();
        assert_eq!(b.dropped.by_severity[0], 9);
        assert_eq!(*p.dropped(), Dropped::default());
        assert!(b.rows.is_sorted_by_key(tick));
    }

    #[test]
    fn payload_bytes_count_against_the_cap_and_follow_their_rows() {
        let mut p = Pending::with_caps(1_000, 8 * 32 + 64);
        for t in 0..6 {
            p.push(&ev(t, Severity::Routine));
        }
        p.push_with_payload(&ev(10, Severity::Notable), vec![7; 40]);
        p.push_with_payload(&ev(11, Severity::Notable), vec![8; 40]);
        // 8 rows and 80 payload bytes would pass 320 bytes, so the two oldest routine rows went.
        assert_eq!(
            (p.len(), p.bytes(), p.dropped().by_severity[0]),
            (6, 6 * 32 + 80, 2)
        );
        assert_eq!(tick(&p.rows()[0]), 2);
        let b = p.take();
        for (i, data) in &b.payloads {
            let t = tick(&b.rows[*i as usize]);
            assert_eq!(data[0], if t == 10 { 7 } else { 8 });
            assert_ne!(flags(&b.rows[*i as usize]) & PAYLOAD, 0);
        }
        assert_eq!(b.payloads.len(), 2);
        let mut q = Pending::new();
        q.push_with_payload(&ev(1, Severity::Major), vec![0; MAX_PAYLOAD_BYTES + 1]);
        assert_eq!((q.len(), q.dropped().payloads), (1, 1));
        q.no_frame();
        assert_eq!(q.dropped().rows(), 1);
    }
}
