// SPDX-License-Identifier: AGPL-3.0-or-later
//! The site ledger's two layouts (EQUILIBRIUM.md §2.3, slice S3; docs/boundary.md §2.4 and §2.5).
//!
//! - **SITE_STATE** (core command 0x8006): an engine tells core's ledger (`mic_rtsim_sites.dat`, world
//!   SavedData outside `world/rtsim/`) where one site stands now: its K, the members at home and away, its
//!   pressure now and the wait left now, and the UUIDs of its home bodies, the bodies it left behind and
//!   the bodies gone missing. Latest wins per (kind, key). No answer, no op id, never resent: a lost one is
//!   healed by the next change and by the refresh each found record gives a known site.
//! - **The found tail:** core appends the site's row to the found record it already sends (FACTION_FOUND,
//!   LAIR_FOUND, NEST_FOUND), as relative values: the pressure and the wait left as of the row's stamp, and
//!   the game ticks elapsed since. The engine ages both by its own clock, so a clock that restarted after
//!   a reset neither shortens a wait nor resets a pressure.
//!
//! Each layout lives here once; the engines, rtsim-jni and rtsim-host's fake Java all use these types.

/// A body's UUID as (most significant, least significant) bits, as the boundary carries it.
pub type Uuid = (u64, u64);

/// The kind of site a ledger row is for. The site key is the module's own (a faction key, a lair key, a
/// nest key), so a row's identity is the pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SiteKind {
    /// A conflict camp (a Mowzie den or grove, or the pool of a kind with no home bodies).
    Camp = 1,
    /// A dragon lair.
    Lair = 2,
    /// An oil nest.
    Nest = 3,
}

impl SiteKind {
    pub fn code(self) -> u32 {
        self as u32
    }

    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Camp),
            2 => Some(Self::Lair),
            3 => Some(Self::Nest),
            _ => None,
        }
    }
}

/// Why a layout did not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteError {
    /// The bytes do not match the size the counts give.
    BadSize,
    /// A field is out of range (an unknown kind, a body count above K).
    BadValue(&'static str),
}

/// The found tail's flags (+0).
pub mod tail_flags {
    /// The ledger has a row for the site; the rest of the tail is that row.
    pub const ROW: u32 = 1;
    /// The reinstall guard reset the sim since the row was written: rtsim did not run the whole time.
    pub const GAP: u32 = 2;
    /// The row's wait left is set (the site was short with an arrival booked).
    pub const WAITING: u32 = 4;
    /// No row, but the plugin knows the site stood before this engine life (nests only; lairs and camps
    /// have LATE).
    pub const EXISTED: u32 = 8;
}

/// The found tail's offsets, from the tail's start (the found record's old end).
pub mod tail {
    pub const FLAGS: usize = 0;
    pub const K: usize = 4;
    pub const COUNT: usize = 6;
    pub const P: usize = 8;
    pub const AWAY: usize = 12;
    pub const N: usize = 14;
    pub const WAIT_LEFT: usize = 16;
    pub const ELAPSED: usize = 24;
    pub const L: usize = 32;
    pub const M: usize = 34;
    pub const UUIDS: usize = 40;
}

/// The found tail's size with no bodies.
pub const TAIL_FIXED: usize = tail::UUIDS;

/// SITE_STATE's flags (+28).
pub mod state_flags {
    /// Fewer members than K, counting those away.
    pub const SHORT: u32 = 1;
    /// No member at all, home or away.
    pub const EMPTY: u32 = 2;
    /// The site cannot hold members now (for the archive and the viewer; no engine reads it back).
    pub const BROKEN: u32 = 4;
    /// An arrival is booked; the wait left is set.
    pub const WAITING: u32 = 8;
}

/// SITE_STATE's offsets, from the record's start (the header is 0 to 7, the command sequence 8 to 15).
pub mod state {
    pub const KEY: usize = 16;
    pub const KIND: usize = 24;
    pub const FLAGS: usize = 28;
    pub const P: usize = 32;
    pub const K: usize = 36;
    pub const COUNT: usize = 38;
    pub const AWAY: usize = 40;
    pub const N: usize = 42;
    pub const L: usize = 44;
    pub const M: usize = 46;
    pub const WAIT_LEFT: usize = 48;
    pub const UUIDS: usize = 56;
}

/// SITE_STATE's size with no bodies.
pub const STATE_FIXED: usize = state::UUIDS;

/// The most bodies one record may list: each of home, left behind and missing is at most K, and the
/// largest record (FACTION_FOUND's 56 bytes, the tail's 40 and the UUIDs) must fit a u16 size. Far above
/// any site's K.
pub const MAX_BODIES: usize = (0xFFF8 - 128) / 16;

/// The UUIDs a row keeps: home bodies, bodies left behind (a life's bodies the site no longer counts,
/// removed on sight), and bodies gone missing (written off while away; a stray if they turn up at a full
/// site, and their death still counts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bodies {
    pub home: Vec<Uuid>,
    pub left_behind: Vec<Uuid>,
    pub missing: Vec<Uuid>,
}

impl Bodies {
    pub fn len(&self) -> usize {
        self.home.len() + self.left_behind.len() + self.missing.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bounds each list to K (and the three to [`MAX_BODIES`]): home and left behind keep their first K,
    /// missing keeps its newest K (the oldest, at the front, dropped first).
    pub fn bound(&mut self, k: u16) {
        let [n, l, m] = self.bounded_counts(k);
        self.home.truncate(n);
        self.left_behind.truncate(l);
        if self.missing.len() > m {
            self.missing.drain(..self.missing.len() - m);
        }
    }

    /// The list lengths [`Self::bound`] leaves.
    pub fn bounded_counts(&self, k: u16) -> [usize; 3] {
        let k = usize::from(k).min(MAX_BODIES / 3);
        [
            self.home.len().min(k),
            self.left_behind.len().min(k),
            self.missing.len().min(k),
        ]
    }

    fn counts(&self) -> [usize; 3] {
        [self.home.len(), self.left_behind.len(), self.missing.len()]
    }

    fn write(&self, b: &mut [u8], at: usize) {
        let all = self
            .home
            .iter()
            .chain(&self.left_behind)
            .chain(&self.missing);
        for (i, &(most, least)) in all.enumerate() {
            put_u64(b, at + 16 * i, most);
            put_u64(b, at + 16 * i + 8, least);
        }
    }

    fn read(b: &[u8], at: usize, [n, l, m]: [usize; 3]) -> Self {
        let uuid = |i: usize| (get_u64(b, at + 16 * i), get_u64(b, at + 16 * i + 8));
        Self {
            home: (0..n).map(uuid).collect(),
            left_behind: (n..n + l).map(uuid).collect(),
            missing: (n + l..n + l + m).map(uuid).collect(),
        }
    }
}

/// Checks the counts against K and the record limit.
fn check_counts(k: u16, counts: [usize; 3]) -> Result<(), SiteError> {
    if counts.iter().any(|&c| c > usize::from(k)) {
        return Err(SiteError::BadValue("bodies above K"));
    }
    if counts.iter().sum::<usize>() > MAX_BODIES {
        return Err(SiteError::BadValue("bodies"));
    }
    Ok(())
}

/// What a SITE_STATE carries: one site as it stands at the engine's now.
#[derive(Clone, Debug, PartialEq)]
pub struct SiteState {
    pub key: u64,
    pub kind: SiteKind,
    /// [`state_flags`].
    pub flags: u32,
    /// The pressure now.
    pub p: f32,
    pub k: u16,
    /// Members at home (bodies, or a pool's count).
    pub count: u16,
    /// Members out (in parties, or puppets flying), so lost with an engine that is reset.
    pub away: u16,
    /// The wait left now, in the kind's units (ticks for camps and lairs, thousandths of a drilling day
    /// for nests); 0 unless WAITING.
    pub wait_left: u64,
    pub bodies: Bodies,
}

impl SiteState {
    /// The record's size, header included, with the lists bounded to K.
    pub fn size(&self) -> usize {
        STATE_FIXED + 16 * self.bodies.bounded_counts(self.k).iter().sum::<usize>()
    }

    pub fn waiting(&self) -> bool {
        self.flags & state_flags::WAITING != 0
    }

    /// Bounds the body lists to K (an engine bounds its own; this is the ledger's guard).
    pub fn bound(&mut self) {
        self.bodies.bound(self.k);
    }

    /// Writes the fields into `rec`, a record of [`Self::size`] bytes whose header and command sequence
    /// (0 to 15) the caller writes. Lists above K are bounded first.
    pub fn encode(&self, rec: &mut [u8]) {
        let mut b = self.bodies.clone();
        b.bound(self.k);
        let [n, l, m] = b.counts();
        put_u64(rec, state::KEY, self.key);
        put_u32(rec, state::KIND, self.kind.code());
        put_u32(rec, state::FLAGS, self.flags);
        put_u32(rec, state::P, sane(self.p).to_bits());
        put_u16(rec, state::K, self.k);
        put_u16(rec, state::COUNT, self.count);
        put_u16(rec, state::AWAY, self.away);
        put_u16(rec, state::N, n as u16);
        put_u16(rec, state::L, l as u16);
        put_u16(rec, state::M, m as u16);
        put_u64(
            rec,
            state::WAIT_LEFT,
            if self.waiting() { self.wait_left } else { 0 },
        );
        b.write(rec, state::UUIDS);
    }

    /// Reads a whole SITE_STATE record (padding to 8 included; its size is a multiple of 8 already).
    pub fn decode(rec: &[u8]) -> Result<Self, SiteError> {
        if rec.len() < STATE_FIXED {
            return Err(SiteError::BadSize);
        }
        let kind = SiteKind::from_code(get_u32(rec, state::KIND))
            .ok_or(SiteError::BadValue("site kind"))?;
        let k = get_u16(rec, state::K);
        let counts = [state::N, state::L, state::M].map(|at| usize::from(get_u16(rec, at)));
        check_counts(k, counts)?;
        if rec.len() != STATE_FIXED + 16 * counts.iter().sum::<usize>() {
            return Err(SiteError::BadSize);
        }
        let flags = get_u32(rec, state::FLAGS);
        Ok(Self {
            key: get_u64(rec, state::KEY),
            kind,
            flags,
            p: sane(f32::from_bits(get_u32(rec, state::P))),
            k,
            count: get_u16(rec, state::COUNT),
            away: get_u16(rec, state::AWAY),
            wait_left: if flags & state_flags::WAITING != 0 {
                get_u64(rec, state::WAIT_LEFT)
            } else {
                0
            },
            bodies: Bodies::read(rec, state::UUIDS, counts),
        })
    }
}

/// A site's ledger row as the found tail hands it over: values as of the row's stamp, and the game ticks
/// elapsed since. With no row (`flags` without ROW) every field is zero but EXISTED.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FoundRow {
    /// [`tail_flags`].
    pub flags: u32,
    pub k: u16,
    pub count: u16,
    /// The pressure as of the stamp.
    pub p: f32,
    pub away: u16,
    /// The wait left as of the stamp, in the kind's units; 0 unless WAITING.
    pub wait_left: u64,
    /// Game ticks since the stamp, saturating at 0 (a world clock below the stamp reads as none).
    pub elapsed: u64,
    pub bodies: Bodies,
}

impl FoundRow {
    /// No row, and the plugin knows of no earlier life of the site.
    pub fn none() -> Self {
        Self::default()
    }

    /// The tail core writes for a row it holds: the state as last sent, its stamp `elapsed` ticks ago,
    /// GAP when the reinstall guard reset the sim since. Rows are only written from a SITE_STATE, so this
    /// is also how rtsim-host's fake ledger hands one back.
    pub fn from_state(s: &SiteState, elapsed: u64, gap: bool) -> Self {
        let mut flags = tail_flags::ROW;
        if gap {
            flags |= tail_flags::GAP;
        }
        if s.waiting() {
            flags |= tail_flags::WAITING;
        }
        let mut bodies = s.bodies.clone();
        bodies.bound(s.k);
        Self {
            flags,
            k: s.k,
            count: s.count,
            p: sane(s.p),
            away: s.away,
            wait_left: if s.waiting() { s.wait_left } else { 0 },
            elapsed,
            bodies,
        }
    }

    pub fn has_row(&self) -> bool {
        self.flags & tail_flags::ROW != 0
    }

    pub fn gap(&self) -> bool {
        self.flags & tail_flags::GAP != 0
    }

    pub fn waiting(&self) -> bool {
        self.has_row() && self.flags & tail_flags::WAITING != 0
    }

    pub fn existed(&self) -> bool {
        self.flags & tail_flags::EXISTED != 0
    }

    /// The wait still to run as of the found record, in the kind's units, for a kind whose units are
    /// game ticks (camps, lairs): the wait left less the ticks elapsed, saturating at 0 (due at once).
    pub fn wait_now_ticks(&self) -> u64 {
        self.wait_left.saturating_sub(self.elapsed)
    }

    /// The tail's size, with the lists bounded to K.
    pub fn size(&self) -> usize {
        Self::size_for(self.bodies.bounded_counts(self.k).iter().sum())
    }

    /// The tail's size for a record that names `bodies` UUIDs.
    pub fn size_for(bodies: usize) -> usize {
        TAIL_FIXED + 16 * bodies
    }

    /// Writes the tail at `b[0..self.size()]`. Lists above K are bounded first.
    pub fn encode(&self, b: &mut [u8]) {
        let mut bodies = self.bodies.clone();
        bodies.bound(self.k);
        let [n, l, m] = bodies.counts();
        b[..TAIL_FIXED].fill(0);
        put_u32(b, tail::FLAGS, self.flags);
        put_u16(b, tail::K, self.k);
        put_u16(b, tail::COUNT, self.count);
        put_u32(b, tail::P, sane(self.p).to_bits());
        put_u16(b, tail::AWAY, self.away);
        put_u16(b, tail::N, n as u16);
        put_u64(b, tail::WAIT_LEFT, self.wait_left);
        put_u64(b, tail::ELAPSED, self.elapsed);
        put_u16(b, tail::L, l as u16);
        put_u16(b, tail::M, m as u16);
        bodies.write(b, tail::UUIDS);
    }

    /// The tail encoded on its own (tests, the fake ledger).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0; self.size()];
        self.encode(&mut b);
        b
    }

    /// Reads a tail that runs exactly to the record's end. Bodies above K, or a size that does not match
    /// the counts, do not decode.
    pub fn decode(b: &[u8]) -> Result<Self, SiteError> {
        if b.len() < TAIL_FIXED {
            return Err(SiteError::BadSize);
        }
        let k = get_u16(b, tail::K);
        let counts = [tail::N, tail::L, tail::M].map(|at| usize::from(get_u16(b, at)));
        check_counts(k, counts)?;
        if b.len() != TAIL_FIXED + 16 * counts.iter().sum::<usize>() {
            return Err(SiteError::BadSize);
        }
        Ok(Self {
            flags: get_u32(b, tail::FLAGS),
            k,
            count: get_u16(b, tail::COUNT),
            p: sane(f32::from_bits(get_u32(b, tail::P))),
            away: get_u16(b, tail::AWAY),
            wait_left: get_u64(b, tail::WAIT_LEFT),
            elapsed: get_u64(b, tail::ELAPSED),
            bodies: Bodies::read(b, tail::UUIDS, counts),
        })
    }

    /// The body count a tail names, from its fixed part (for a decoder that sizes the record first).
    pub fn bodies_in(b: &[u8]) -> Option<usize> {
        if b.len() < TAIL_FIXED {
            return None;
        }
        Some(
            [tail::N, tail::L, tail::M]
                .iter()
                .map(|&at| usize::from(get_u16(b, at)))
                .sum(),
        )
    }
}

/// A pressure that is not a finite positive number reads as 0.
fn sane(p: f32) -> f32 {
    if p.is_finite() && p > 0.0 { p } else { 0.0 }
}

fn put_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn get_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

fn get_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn get_u64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> SiteState {
        SiteState {
            key: 0xDEAD_BEEF_0000_0001,
            kind: SiteKind::Camp,
            flags: state_flags::SHORT | state_flags::WAITING,
            p: 1.25,
            k: 3,
            count: 1,
            away: 1,
            wait_left: 480_000,
            bodies: Bodies {
                home: vec![(1, 2)],
                left_behind: vec![(3, 4)],
                missing: vec![(5, 6), (7, 8)],
            },
        }
    }

    #[test]
    fn site_state_round_trips() {
        let s = state();
        let mut rec = vec![0u8; s.size()];
        assert_eq!(rec.len(), 56 + 16 * 4);
        s.encode(&mut rec);
        assert_eq!(SiteState::decode(&rec), Ok(s.clone()));
        // Not waiting: the wait left is not sent, whatever the field holds.
        let mut idle = s.clone();
        idle.flags = 0;
        idle.encode(&mut rec);
        assert_eq!(SiteState::decode(&rec).unwrap().wait_left, 0);
    }

    #[test]
    fn site_state_rejects_bad_records() {
        let s = state();
        let mut rec = vec![0u8; s.size()];
        s.encode(&mut rec);
        assert_eq!(
            SiteState::decode(&rec[..rec.len() - 16]),
            Err(SiteError::BadSize)
        );
        assert_eq!(SiteState::decode(&rec[..40]), Err(SiteError::BadSize));
        let mut bad = rec.clone();
        put_u32(&mut bad, state::KIND, 9);
        assert_eq!(
            SiteState::decode(&bad),
            Err(SiteError::BadValue("site kind"))
        );
        let mut bad = rec.clone();
        put_u16(&mut bad, state::K, 1);
        assert_eq!(
            SiteState::decode(&bad),
            Err(SiteError::BadValue("bodies above K"))
        );
        let mut nan = rec;
        put_u32(&mut nan, state::P, f32::NAN.to_bits());
        assert_eq!(SiteState::decode(&nan).unwrap().p, 0.0);
    }

    #[test]
    fn missing_keeps_the_newest_k() {
        let mut s = state();
        s.k = 2;
        s.bodies.missing = vec![(1, 1), (2, 2), (3, 3)];
        let mut rec = vec![0u8; s.size()];
        assert_eq!(rec.len(), STATE_FIXED + 16 * 4);
        s.encode(&mut rec);
        let back = SiteState::decode(&rec).unwrap();
        assert_eq!(back.bodies.missing, vec![(2, 2), (3, 3)]);
        assert_eq!(back.bodies.home, vec![(1, 2)]);
    }

    #[test]
    fn found_row_round_trips() {
        let row = FoundRow::from_state(&state(), 24_000, true);
        assert!(row.has_row() && row.gap() && row.waiting() && !row.existed());
        assert_eq!(row.wait_now_ticks(), 456_000);
        let b = row.to_bytes();
        assert_eq!(b.len(), 40 + 16 * 4);
        assert_eq!(FoundRow::bodies_in(&b), Some(4));
        assert_eq!(FoundRow::decode(&b), Ok(row.clone()));
        // No row: 40 zero bytes, EXISTED aside.
        let none = FoundRow::none().to_bytes();
        assert_eq!(none, vec![0u8; 40]);
        let existed = FoundRow {
            flags: tail_flags::EXISTED,
            ..FoundRow::none()
        };
        let back = FoundRow::decode(&existed.to_bytes()).unwrap();
        assert!(back.existed() && !back.has_row() && !back.waiting());
        // A wait already run reads as due at once.
        let late = FoundRow {
            elapsed: u64::MAX,
            ..row
        };
        assert_eq!(late.wait_now_ticks(), 0);
    }

    #[test]
    fn found_row_rejects_bad_tails() {
        let b = FoundRow::from_state(&state(), 0, false).to_bytes();
        assert_eq!(FoundRow::decode(&b[..b.len() - 8]), Err(SiteError::BadSize));
        assert_eq!(FoundRow::decode(&b[..39]), Err(SiteError::BadSize));
        let mut long = b.clone();
        long.extend_from_slice(&[0; 16]);
        assert_eq!(FoundRow::decode(&long), Err(SiteError::BadSize));
        let mut over = b;
        put_u16(&mut over, tail::K, 0);
        assert_eq!(
            FoundRow::decode(&over),
            Err(SiteError::BadValue("bodies above K"))
        );
    }
}
