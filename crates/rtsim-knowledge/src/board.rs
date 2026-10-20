// SPDX-License-Identifier: AGPL-3.0-or-later
//! Noticeboards and gossip packets (PLAN.md §5.6): a holder's board is the log of reports it has
//! learned, in the order it learned them. A traveller leaving carries a [`Packet`] of what the
//! board gained since the last packet on that road ([`BoardCursor`]), so gossip moves only what
//! changed, and only when a carrier goes. Veloren syncs every report every tick
//! (`sync_npcs.rs:108-124`, "TODO: Only share new reports"); this is the fix.
//!
//! "Already heard" is not the board's job: the report table's knower list is the authority, so a
//! board that dropped an old entry for its cap never relearns that report as news.

use std::collections::BTreeMap;

use rtsim_kernel::codec::Writer;

use crate::report::ReportId;

/// Where a road's last packet stopped reading a board. One per (holder, outgoing road), kept by
/// whoever runs the road.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BoardCursor(pub u64);

/// Reports a traveller carries from one holder to another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packet {
    pub reports: Vec<ReportId>,
}

impl Packet {
    pub fn is_empty(&self) -> bool {
        self.reports.is_empty()
    }

    pub fn len(&self) -> usize {
        self.reports.len()
    }

    /// Adds another packet's reports, keeping order and dropping repeats: two packets bound for
    /// the same departure become one.
    pub fn merge(&mut self, other: &Packet) {
        for &r in &other.reports {
            if !self.reports.contains(&r) {
                self.reports.push(r);
            }
        }
    }
}

/// A holder's board: reports by the sequence number they arrived with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Noticeboard {
    by_seq: BTreeMap<u64, ReportId>,
    seq_of: BTreeMap<ReportId, u64>,
    next_seq: u64,
    cap: u32,
}

impl Noticeboard {
    pub fn new(cap: u32) -> Self {
        Self {
            cap,
            ..Self::default()
        }
    }

    pub fn len(&self) -> usize {
        self.by_seq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_seq.is_empty()
    }

    /// The sequence the next report will get: a cursor here has read everything.
    pub fn head(&self) -> BoardCursor {
        BoardCursor(self.next_seq)
    }

    pub fn contains(&self, id: ReportId) -> bool {
        self.seq_of.contains_key(&id)
    }

    /// Appends a report. When full, the entry learned longest ago goes: it has had the longest
    /// to be passed on, and the report itself and the knower list stay in the table. Returns the
    /// report pushed off, if any.
    pub fn push(&mut self, id: ReportId) -> Option<ReportId> {
        if self.cap == 0 || self.seq_of.contains_key(&id) {
            return None;
        }
        let mut dropped = None;
        if self.by_seq.len() >= self.cap as usize
            && let Some((_, old)) = self.by_seq.pop_first()
        {
            self.seq_of.remove(&old);
            dropped = Some(old);
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        self.by_seq.insert(seq, id);
        self.seq_of.insert(id, seq);
        dropped
    }

    pub fn remove(&mut self, id: ReportId) {
        if let Some(seq) = self.seq_of.remove(&id) {
            self.by_seq.remove(&seq);
        }
    }

    /// Up to `max` reports learned since `cursor`, oldest first; the cursor moves past what was
    /// taken, so a capped packet leaves the rest for the next traveller instead of losing it.
    pub fn take_since(&self, cursor: &mut BoardCursor, max: u32) -> Packet {
        let mut reports = Vec::new();
        for (&seq, &id) in self.by_seq.range(cursor.0..) {
            if reports.len() >= max as usize {
                break;
            }
            reports.push(id);
            cursor.0 = seq + 1;
        }
        if reports.len() < max as usize {
            // Nothing left after what was taken: skip any gap of removed entries too.
            cursor.0 = cursor.0.max(self.next_seq);
        }
        Packet { reports }
    }

    /// Reports in the order learned.
    pub fn iter(&self) -> impl Iterator<Item = ReportId> + '_ {
        self.by_seq.values().copied()
    }

    pub fn encode(&self, w: &mut Writer) {
        w.u64(self.next_seq);
        w.u32(self.cap);
        w.var_u64(self.by_seq.len() as u64);
        for (&seq, id) in &self.by_seq {
            w.var_u64(seq);
            w.u64(id.to_bits());
        }
    }

    pub fn heap_bytes(&self) -> usize {
        // Two map entries per report, each with a share of its B-tree node.
        self.by_seq.len() * 2 * (8 + 8 + 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_kernel::handle::Handle;

    fn r(i: u32) -> ReportId {
        Handle::from_parts(i, 1)
    }

    #[test]
    fn cursors_take_only_what_changed() {
        let mut b = Noticeboard::new(10);
        let mut c = BoardCursor::default();
        b.push(r(1));
        b.push(r(2));
        assert_eq!(b.take_since(&mut c, 64).reports, vec![r(1), r(2)]);
        assert!(b.take_since(&mut c, 64).is_empty());
        b.push(r(3));
        b.push(r(2)); // already on the board: not a change
        assert_eq!(b.take_since(&mut c, 64).reports, vec![r(3)]);
    }

    #[test]
    fn capped_packets_leave_the_rest_for_later() {
        let mut b = Noticeboard::new(10);
        let mut c = BoardCursor::default();
        for i in 1..=5 {
            b.push(r(i));
        }
        assert_eq!(b.take_since(&mut c, 2).reports, vec![r(1), r(2)]);
        assert_eq!(b.take_since(&mut c, 2).reports, vec![r(3), r(4)]);
        assert_eq!(b.take_since(&mut c, 2).reports, vec![r(5)]);
        assert!(b.take_since(&mut c, 2).is_empty());
    }

    #[test]
    fn a_full_board_drops_its_oldest() {
        let mut b = Noticeboard::new(2);
        b.push(r(1));
        b.push(r(2));
        assert_eq!(b.push(r(3)), Some(r(1)));
        assert_eq!(b.iter().collect::<Vec<_>>(), vec![r(2), r(3)]);
    }
}
