// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recording (VIEWER.md §10.1). A module engine owns a [`Tape`] and writes to it where it already
//! does the thing; the engine's [`Recorder`] takes each tape's rows into the pending buffer, giving
//! them the archive dictionary's numbers. `rtsim-cli sim` drives the same module engines with no
//! JNI, so a tape records the same rows there as in the game.
//!
//! A tape is off until its owner turns it on (the engine does after loading, so a module that
//! restores by replaying its inputs records nothing twice). It is not state: it is not saved, it is
//! not in a state hash, and an engine compares equal whatever its tape holds.
//!
//! On a tape a row's kind is its place in [`crate::kinds::KINDS`], its frame an index into the
//! tape's own small table of dimensions, and an id past 48 bits an index into the tape's own id
//! list. A routine row is one push of 32 bytes; a payload's text is made only for a row at notable
//! severity or above.

use std::collections::{BTreeMap, HashMap};

use crate::dict::Dictionary;
use crate::kinds::{KINDS, Kind};
use crate::pending::{Batch, Dropped, Pending};
use crate::row::{
    self, Event, Handle, LEG, Leg, MAX_48, OBJECT_DICT, PAYLOAD, RawRow, Row, SUBJECT_DICT,
    Severity,
};

/// Rows a tape holds between two takes at most; further rows are dropped and counted. The engine
/// takes every step, so only a step making this many rows reaches it (TUNABLE).
pub const MAX_TAPE_ROWS: usize = 65_536;

/// Where and when an event happened: a game tick, the engine's numeric dimension id and a block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct At {
    pub tick: u64,
    pub dim: u32,
    pub x: i32,
    pub z: i32,
}

impl At {
    pub fn new(tick: u64, dim: u32, x: i32, z: i32) -> At {
        At { tick, dim, x, z }
    }

    /// A block from a position in blocks, rounded down.
    pub fn of(tick: u64, dim: u32, x: f64, z: f64) -> At {
        At::new(tick, dim, block(x), block(z))
    }
}

/// A coordinate in blocks, rounded down and saturating.
pub fn block(v: f64) -> i32 {
    v.floor() as i32
}

/// A magnitude as a row holds it, saturating.
pub fn magnitude(v: i64) -> i32 {
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// A module's recorder. See the module docs.
#[derive(Debug, Default)]
pub struct Tape {
    on: bool,
    rows: Vec<RawRow>,
    payloads: Vec<(u32, Vec<u8>)>,
    dims: Vec<u32>,
    ids: Vec<u128>,
    dropped: Dropped,
}

/// Not state: any two tapes are equal, so an engine's equality ignores its tape.
impl PartialEq for Tape {
    fn eq(&self, _: &Tape) -> bool {
        true
    }
}

impl Eq for Tape {}

/// The row a tape just took, for its payload ([`Mark::text`]).
#[derive(Debug)]
pub struct Mark<'a> {
    tape: &'a mut Tape,
    row: Option<u32>,
}

impl Mark<'_> {
    /// Gives the row a payload: one short line the page shows (`"loot: 12 iron ingot, 3 bread"`).
    /// `text` runs only for a row at notable severity or above.
    pub fn text(self, text: impl FnOnce() -> String) {
        let Some(i) = self.row else { return };
        if row::severity(&self.tape.rows[i as usize]) < Severity::Notable {
            return;
        }
        let t = text();
        if t.is_empty() {
            return;
        }
        self.tape.rows[i as usize][9] |= PAYLOAD;
        self.tape.payloads.push((i, t.into_bytes()));
    }
}

impl Tape {
    /// An empty tape, off.
    pub fn new() -> Tape {
        Tape::default()
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Rows waiting for the engine's recorder.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// An event of `kind` at its usual severity. `subject` and `object` are entity ids (0 for
    /// none); `magnitude` is goods, units or damage by kind, saturating at an i32.
    pub fn event(
        &mut self,
        kind: Kind,
        at: At,
        subject: u128,
        object: u128,
        magnitude: i64,
    ) -> Mark<'_> {
        self.event_as(kind, kind.severity(), at, subject, object, magnitude)
    }

    /// An event at another severity than its kind's usual.
    pub fn event_as(
        &mut self,
        kind: Kind,
        severity: Severity,
        at: At,
        subject: u128,
        object: u128,
        magnitude: i64,
    ) -> Mark<'_> {
        if !self.on || !self.room(severity) {
            return Mark {
                tape: self,
                row: None,
            };
        }
        let Some(frame) = self.dim(at.dim) else {
            self.dropped.no_frame += 1;
            return Mark {
                tape: self,
                row: None,
            };
        };
        let subject = self.id(subject);
        let object = self.id(object);
        let raw = Row::Event(Event {
            tick: at.tick,
            kind: kind.index() as u16,
            frame,
            severity,
            x: at.x,
            z: at.z,
            subject,
            object,
            magnitude: self::magnitude(magnitude),
            payload: false,
            clamped: false,
        })
        .encode();
        self.push(raw)
    }

    /// A leg of `kind` at its usual severity: `subject` is at `from` at tick `t0` and at `to` at
    /// `t1`. One leg per decision; the leg gives the position at any tick between.
    pub fn leg(
        &mut self,
        kind: Kind,
        dim: u32,
        from: (u64, i32, i32),
        to: (u64, i32, i32),
        subject: u128,
    ) -> Mark<'_> {
        self.leg_as(kind, kind.severity(), dim, from, to, subject)
    }

    /// A leg at another severity than its kind's usual (a named mover's legs are notable).
    pub fn leg_as(
        &mut self,
        kind: Kind,
        severity: Severity,
        dim: u32,
        from: (u64, i32, i32),
        to: (u64, i32, i32),
        subject: u128,
    ) -> Mark<'_> {
        if !self.on || !self.room(severity) {
            return Mark {
                tape: self,
                row: None,
            };
        }
        let Some(frame) = self.dim(dim) else {
            self.dropped.no_frame += 1;
            return Mark {
                tape: self,
                row: None,
            };
        };
        let subject = self.id(subject);
        let (t0, t1) = (from.0, to.0.max(from.0));
        let mut clamped = false;
        let duration = u32::try_from(t1 - t0).unwrap_or_else(|_| {
            clamped = true;
            u32::MAX
        });
        let raw = Row::Leg(Leg {
            t0,
            kind: kind.index() as u16,
            frame,
            severity,
            x0: from.1,
            z0: from.2,
            x1: to.1,
            z1: to.2,
            subject,
            duration,
            clamped,
        })
        .encode();
        self.push(raw)
    }

    fn room(&mut self, severity: Severity) -> bool {
        if self.rows.len() < MAX_TAPE_ROWS {
            return true;
        }
        self.dropped.by_severity[severity as usize] += 1;
        false
    }

    fn push(&mut self, raw: RawRow) -> Mark<'_> {
        let i = self.rows.len() as u32;
        self.rows.push(raw);
        Mark {
            tape: self,
            row: Some(i),
        }
    }

    /// The tape's index for a dimension; `None` past 255 dimensions between two takes (the row is
    /// dropped and counted). A tape sees a handful, so a scan is cheap.
    fn dim(&mut self, dim: u32) -> Option<u8> {
        if let Some(i) = self.dims.iter().position(|&d| d == dim) {
            return Some(i as u8);
        }
        if self.dims.len() >= crate::dict::MAX_FRAMES {
            return None;
        }
        self.dims.push(dim);
        Some((self.dims.len() - 1) as u8)
    }

    /// An id as the tape holds it: itself when it fits in 48 bits, otherwise an entry in the tape's
    /// id list (the recorder turns it into the dictionary's).
    fn id(&mut self, id: u128) -> Handle {
        if id <= u128::from(MAX_48) {
            return Handle::direct(id as u64);
        }
        let i = self.ids.iter().position(|&v| v == id).unwrap_or_else(|| {
            self.ids.push(id);
            self.ids.len() - 1
        });
        Handle {
            value: i as u64,
            dict: true,
        }
    }
}

/// The engine's side: the pending buffer and the dictionary, in the engine's state. Takes the
/// modules' tapes each step ([`Recorder::absorb`]) and hands a save its rows ([`Recorder::take`]).
#[derive(Debug)]
pub struct Recorder {
    pending: Pending,
    dict: Dictionary,
    /// Dictionary number by place in [`KINDS`].
    kinds: Vec<Option<u16>>,
    /// Frame number by dimension id; `None` when the dictionary had no frame left.
    frames: HashMap<u32, Option<u8>>,
}

impl Default for Recorder {
    /// A recorder with an empty dictionary.
    fn default() -> Recorder {
        Recorder::new(Dictionary::new())
    }
}

impl Recorder {
    /// A recorder starting from the archive's stored dictionary, so numbers keep their meaning.
    pub fn new(dict: Dictionary) -> Recorder {
        Recorder::with_pending(dict, Pending::new())
    }

    /// With a pending buffer of other caps.
    pub fn with_pending(dict: Dictionary, pending: Pending) -> Recorder {
        Recorder {
            pending,
            dict,
            kinds: vec![None; KINDS.len()],
            frames: HashMap::new(),
        }
    }

    /// The frame number of the engine's dimension `dim`: the dictionary's `dim<n>`, named on first
    /// use.
    pub fn frame(&mut self, dim: u32) -> Option<u8> {
        let dict = &mut self.dict;
        *self
            .frames
            .entry(dim)
            .or_insert_with(|| dict.frame(&frame_name(dim)))
    }

    fn kind(&mut self, k: usize) -> Option<u16> {
        if let Some(n) = self.kinds.get(k).copied().flatten() {
            return Some(n);
        }
        let n = self.dict.kind(KINDS.get(k)?.name)?;
        self.kinds[k] = Some(n);
        Some(n)
    }

    /// Moves a tape's rows into the pending buffer, leaving the tape empty.
    pub fn absorb(&mut self, tape: &mut Tape) {
        let frames: Vec<Option<u8>> = tape
            .dims
            .clone()
            .into_iter()
            .map(|d| self.frame(d))
            .collect();
        let mut payloads = std::mem::take(&mut tape.payloads).into_iter().peekable();
        for (i, mut raw) in tape.rows.drain(..).enumerate() {
            let payload = payloads.next_if(|p| p.0 as usize == i).map(|p| p.1);
            let Some(Some(frame)) = frames.get(raw[8] as usize).copied() else {
                self.pending.no_frame();
                continue;
            };
            let Some(kind) = self.kind(row::kind(&raw) as usize) else {
                // A kind past the dictionary's 65,536: counted like a row with no frame.
                self.pending.no_frame();
                continue;
            };
            raw[6..8].copy_from_slice(&kind.to_le_bytes());
            raw[8] = frame;
            let f = raw[9];
            let leg = f & LEG != 0;
            if f & SUBJECT_DICT != 0 {
                let at = if leg { 22 } else { 16 };
                self.redirect(&mut raw, at, &tape.ids, SUBJECT_DICT);
            }
            if !leg && f & OBJECT_DICT != 0 {
                self.redirect(&mut raw, 22, &tape.ids, OBJECT_DICT);
            }
            self.pending.push_raw(raw, payload);
        }
        tape.ids.clear();
        tape.dims.clear();
        let d = std::mem::take(&mut tape.dropped);
        self.pending.count(&d);
    }

    /// A tape's id-list entry at `raw[at..at + 6]` as the dictionary's handle.
    fn redirect(&mut self, raw: &mut RawRow, at: usize, ids: &[u128], flag: u8) {
        let mut b = [0u8; 8];
        b[..6].copy_from_slice(&raw[at..at + 6]);
        let id = ids
            .get(u64::from_le_bytes(b) as usize)
            .copied()
            .unwrap_or(0);
        let h = self.dict.handle(id);
        raw[at..at + 6].copy_from_slice(&h.value.to_le_bytes()[..6]);
        if h.dict {
            raw[9] |= flag;
        } else {
            raw[9] &= !flag;
        }
    }

    /// One save's rows, and the dictionary when it changed since the last take.
    pub fn take(&mut self) -> (Batch, Option<Dictionary>) {
        let dict = self.dict.is_dirty().then(|| {
            self.dict.mark_saved();
            self.dict.clone()
        });
        (self.pending.take(), dict)
    }

    pub fn pending(&self) -> &Pending {
        &self.pending
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dict
    }
}

/// A dimension's frame name in the dictionary: `dim<n>`, the engine's own numeric id (the viewer
/// names it from the world's journal).
pub fn frame_name(dim: u32) -> String {
    format!("dim{dim}")
}

/// The kinds a set of tapes recorded, by place in [`KINDS`]: each module's test that every kind
/// it records is registered reads it (a row's kind on a tape is always a registered one, so the
/// test checks the module records what it declares).
pub fn kinds_on(tape: &Tape) -> Vec<Kind> {
    let mut v: Vec<Kind> = tape
        .rows
        .iter()
        .filter_map(|r| Kind::from_index(row::kind(r) as usize))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// A tape's rows as the archive will hold them, with names for kinds and frames: for tests and
/// for `rtsim-cli sim`'s counts. Leaves the tape as it was.
pub fn rows_on(tape: &Tape) -> Vec<(Kind, u32, Row)> {
    tape.rows
        .iter()
        .filter_map(|r| {
            let kind = Kind::from_index(row::kind(r) as usize)?;
            let dim = *tape.dims.get(row::frame(r) as usize)?;
            Some((kind, dim, Row::decode(r)))
        })
        .collect()
}

/// What a run recorded, for `rtsim-cli sim`'s R (rows per game day, VIEWER.md §10.4): every row
/// its tapes took or dropped past their cap, by kind and by severity. Drain the tapes at least once
/// a game day so a busy day's rows are counted by kind, not only as dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meter {
    /// Rows taken, by kind name.
    pub by_kind: BTreeMap<&'static str, u64>,
    /// Rows taken or dropped, by severity (routine, minor, notable, major).
    pub by_severity: [u64; 4],
    /// Leg rows taken.
    pub legs: u64,
    /// Payload bytes taken.
    pub payload_bytes: u64,
    /// Rows the tapes dropped past their cap or for want of a frame (in `by_severity` too, when
    /// their severity is known).
    pub dropped: u64,
}

impl Meter {
    pub fn new() -> Meter {
        Meter::default()
    }

    /// Counts a tape's rows and empties it (its on switch stays as it was).
    pub fn drain(&mut self, tape: &mut Tape) {
        for r in &tape.rows {
            if let Some(k) = Kind::from_index(row::kind(r) as usize) {
                *self.by_kind.entry(k.name()).or_default() += 1;
            }
            self.by_severity[row::severity(r) as usize] += 1;
            if row::flags(r) & LEG != 0 {
                self.legs += 1;
            }
        }
        self.payload_bytes += tape.payloads.iter().map(|p| p.1.len() as u64).sum::<u64>();
        let d = std::mem::take(&mut tape.dropped);
        for (a, b) in self.by_severity.iter_mut().zip(d.by_severity) {
            *a += b;
        }
        self.dropped += d.rows();
        tape.rows.clear();
        tape.payloads.clear();
        tape.ids.clear();
        tape.dims.clear();
    }

    /// Adds another meter's counts (a run's parts).
    pub fn add(&mut self, o: &Meter) {
        for (k, n) in &o.by_kind {
            *self.by_kind.entry(k).or_default() += n;
        }
        for (a, b) in self.by_severity.iter_mut().zip(o.by_severity) {
            *a += b;
        }
        self.legs += o.legs;
        self.payload_bytes += o.payload_bytes;
        self.dropped += o.dropped;
    }

    /// Every row counted, taken or dropped.
    pub fn rows(&self) -> u64 {
        self.by_kind.values().sum::<u64>() + self.dropped
    }

    /// Rows per game day over `days`.
    pub fn per_day(&self, days: f64) -> f64 {
        if days > 0.0 {
            self.rows() as f64 / days
        } else {
            0.0
        }
    }

    /// Game days of full detail the full tier keeps at `per_day` rows a day: its 128 MiB share of
    /// the default budget holds 4,194,304 rows.
    pub fn full_days(per_day: f64) -> Option<f64> {
        (per_day > 0.0).then(|| FULL_TIER_ROWS as f64 / per_day)
    }
}

/// Rows the full tier's 128 MiB share of the default 256 MiB budget holds, at 32 bytes a row.
pub const FULL_TIER_ROWS: u64 = (128 << 20) / 32;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::{CONFLICT_RAID, DRAGONS_FLIGHT, HERDS_MIGRATION, TRAVEL_CARAVAN};

    #[test]
    fn a_tape_is_off_until_turned_on() {
        let mut t = Tape::new();
        t.event(CONFLICT_RAID, At::new(5, 0, 1, 2), 3, 4, 5)
            .text(|| panic!("no text while off"));
        assert!(t.is_empty());
        t.set_on(true);
        t.event(CONFLICT_RAID, At::new(5, 0, 1, 2), 3, 4, 5);
        assert_eq!(t.len(), 1);
        assert_eq!(t, Tape::new());
    }

    #[test]
    fn the_recorder_names_kinds_frames_and_big_ids() {
        let mut t = Tape::new();
        t.set_on(true);
        let uuid = 0xDEAD_BEEF_0000_0000_0000_0000_0000_0001u128;
        t.event(CONFLICT_RAID, At::new(100, 7, -3, 9), uuid, 12, 1 << 40)
            .text(|| "loot: 12 iron ingot".into());
        t.event(TRAVEL_CARAVAN, At::new(101, 0, 0, 0), 5, 0, 1)
            .text(|| panic!("routine rows carry no text"));
        t.leg(DRAGONS_FLIGHT, 7, (200, 0, 0), (300, 100, -100), uuid);
        t.leg_as(
            HERDS_MIGRATION,
            Severity::Minor,
            0,
            (50, 1, 1),
            (10, 2, 2),
            9,
        );
        assert_eq!(
            kinds_on(&t),
            [
                CONFLICT_RAID,
                DRAGONS_FLIGHT,
                HERDS_MIGRATION,
                TRAVEL_CARAVAN
            ]
        );
        // A dictionary that already holds names keeps their numbers.
        let mut d = Dictionary::new();
        d.kind("dragons.flight");
        d.frame("dim0");
        d.mark_saved();
        let mut r = Recorder::new(d);
        r.absorb(&mut t);
        assert!(t.is_empty());
        let (batch, dict) = r.take();
        let dict = dict.expect("new names make the dictionary dirty");
        assert!(r.take().1.is_none());
        assert_eq!(batch.rows.len(), 4);
        let rows: Vec<Row> = batch.rows.iter().map(Row::decode).collect();
        let Row::Event(raid) = rows[0] else { panic!() };
        assert_eq!(dict.kind_name(raid.kind), Some("conflict.raid"));
        assert_eq!(dict.frame_name(raid.frame), Some("dim7"));
        assert_eq!(dict.resolve(raid.subject), Some(uuid));
        assert_eq!(dict.resolve(raid.object), Some(12));
        assert!(!raid.object.dict);
        assert_eq!((raid.magnitude, raid.severity), (i32::MAX, Severity::Major));
        assert_eq!(batch.payloads, [(0, b"loot: 12 iron ingot".to_vec())]);
        let Row::Leg(flight) = rows[2] else { panic!() };
        assert_eq!(flight.kind, 0);
        assert_eq!(dict.resolve(flight.subject), Some(uuid));
        assert_eq!((flight.t0, flight.duration), (200, 100));
        let Row::Leg(herd) = rows[3] else { panic!() };
        // An end before the start is a leg of no length, not a wrap.
        assert_eq!((herd.t0, herd.duration), (50, 0));
        assert_eq!(dict.frame_name(herd.frame), Some("dim0"));
        assert_eq!(herd.frame, 0);
    }

    #[test]
    fn a_full_tape_drops_and_counts() {
        let mut t = Tape::new();
        t.set_on(true);
        for i in 0..MAX_TAPE_ROWS as u64 + 3 {
            t.event(TRAVEL_CARAVAN, At::new(i, 0, 0, 0), 0, 0, 0);
        }
        assert_eq!(t.len(), MAX_TAPE_ROWS);
        let mut r = Recorder::with_pending(Dictionary::new(), Pending::with_caps(1 << 20, 1 << 30));
        r.absorb(&mut t);
        let (b, _) = r.take();
        assert_eq!(b.rows.len(), MAX_TAPE_ROWS);
        assert_eq!(b.dropped.by_severity[0], 3);
    }

    #[test]
    fn a_meter_counts_taken_and_dropped_rows() {
        let mut t = Tape::new();
        t.set_on(true);
        for i in 0..MAX_TAPE_ROWS as u64 + 2 {
            t.event(TRAVEL_CARAVAN, At::new(i, 0, 0, 0), 0, 0, 0);
        }
        let mut m = Meter::new();
        m.drain(&mut t);
        assert!(t.is_empty() && t.is_on());
        t.event(CONFLICT_RAID, At::new(9, 1, 0, 0), 1, 2, 3)
            .text(|| "loot: 2 bread".into());
        t.leg(DRAGONS_FLIGHT, 0, (0, 0, 0), (10, 5, 5), 7);
        let mut m2 = Meter::new();
        m2.drain(&mut t);
        m.add(&m2);
        assert_eq!(m.rows(), MAX_TAPE_ROWS as u64 + 4);
        assert_eq!(m.dropped, 2);
        assert_eq!(m.by_kind["travel.caravan"], MAX_TAPE_ROWS as u64);
        assert_eq!((m.by_kind["conflict.raid"], m.legs), (1, 1));
        assert_eq!(m.by_severity, [MAX_TAPE_ROWS as u64 + 2, 0, 1, 1]);
        assert_eq!(m.payload_bytes, 13);
        assert_eq!(m.per_day(2.0), (MAX_TAPE_ROWS as u64 + 4) as f64 / 2.0);
        assert_eq!(Meter::full_days(1_000.0), Some(4_194.304));
        assert_eq!(Meter::full_days(0.0), None);
    }

    #[test]
    fn many_dimensions_on_one_tape() {
        let mut t = Tape::new();
        t.set_on(true);
        for d in 0..300u32 {
            t.event(TRAVEL_CARAVAN, At::new(1, d, 0, 0), 0, 0, 0);
        }
        assert_eq!(t.len(), 255);
        let mut r = Recorder::new(Dictionary::new());
        r.absorb(&mut t);
        // Past 255 dimensions rows are dropped and counted, never mislabelled.
        t.event(TRAVEL_CARAVAN, At::new(2, 299, 0, 0), 0, 0, 0);
        r.absorb(&mut t);
        let (b, dict) = r.take();
        assert_eq!((b.rows.len(), b.dropped.no_frame), (255, 46));
        let dict = dict.unwrap();
        for raw in &b.rows {
            let Row::Event(e) = Row::decode(raw) else {
                panic!()
            };
            assert!(dict.frame_name(e.frame).unwrap().starts_with("dim"));
        }
    }
}
