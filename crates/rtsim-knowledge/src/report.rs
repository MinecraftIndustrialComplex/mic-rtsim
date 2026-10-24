// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reports (PLAN.md §5.6, SCALE.md §9): a report is a fact, never an opinion. Who did what to
//! whom, where, when, how severe. It is stored once, in a global table of packed 32-byte rows, and
//! holders know it by handle.
//!
//! - [`ReportRow`] is the exact SCALE.md §9 layout.
//! - [`ReportTable`] caps the rows and evicts by importance: player involvement, then recency,
//!   then magnitude (SCALE.md §6). The least important goes first, never the strongest (Veloren's
//!   sentiment cleanup got this backwards, `sentiment.rs:78-96`).
//! - Each kind has a lifetime. Expiry is booked on a timing wheel at insertion, so expiring costs
//!   per expiring report, not per report held.

use std::collections::BTreeSet;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::Writer;
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::wheel::TimingWheel;

use crate::actor::ActorId;

/// The largest |x| or |z| a packed row can hold: 24-bit signed, ±8,388,607 blocks. Minecraft's
/// world border is ±29,999,984, so a frame (`Place::frame`) must be chosen so its coordinates fit,
/// for example a frame per 16M-block square. Phase 1's overworld villages fit as they are.
pub const COORD_LIMIT: i32 = (1 << 23) - 1;

/// Where a report happened: a frame (dimension or coarser frame, PLAN.md §5.2) and block
/// coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Place {
    pub frame: u16,
    pub x: i32,
    pub z: i32,
    pub y: i16,
}

impl Place {
    pub const fn new(frame: u16, x: i32, y: i16, z: i32) -> Place {
        Place { frame, x, z, y }
    }

    /// Squared horizontal distance in blocks, or `None` in another frame.
    pub fn dist2(&self, other: &Place) -> Option<i64> {
        if self.frame != other.frame {
            return None;
        }
        let dx = i64::from(self.x) - i64::from(other.x);
        let dz = i64::from(self.z) - i64::from(other.z);
        Some(dx * dx + dz * dz)
    }
}

/// What happened. The numbers are the packed row's `kind` field and never change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum ReportKind {
    /// Hurting a trader, a llama or a ribbit merchant (PLAN.md §8.7).
    Assault = 1,
    /// Killing one.
    Killing = 2,
    /// Taking cargo.
    Robbery = 3,
    /// Leashing or picking up a caravan's llama.
    Theft = 4,
    /// A fair trade: positive.
    Trade = 5,
    /// Killing a mob that was attacking a caravan (C24): positive.
    Rescue = 6,
    /// Hurting a villager or a village ribbit.
    ResidentAssault = 7,
    /// Killing one.
    ResidentKilling = 8,
    /// A settlement working near an oil field a dragon refuels on (OILGATE.md §4.5, OG-8): the
    /// dragon notices a district, its oil sample, its fuel stock or its pollution. Negative.
    Encroachment = 9,
    /// A camp feuds with a target (TALK.md §9.4). `who` is the camp, `whom` its target (a player, a
    /// camp or a village), severity the feud's rung. News only: no sentiment, no vanilla gossip.
    Feud = 10,
    /// A camp swore a pact. `who` is the camp that swore, `whom` its ally; a second row with the
    /// "against" flag names the foe. News only.
    PactSworn = 11,
    /// A camp broke a pact. `who` is the camp that broke it, `whom` its ally; a betrayal is a flag.
    /// News only.
    PactBroken = 12,
    /// A camp raided a target. `who` is the camp, `whom` the target; severity is the outcome
    /// ([`raid`]: held, beaten, plundered) and a joint raid is a flag. News only.
    Raid = 13,
    /// A camp's notable fell. `who` is the camp, `whom` the fallen chief; severity is how
    /// ([`fell`]: died, killed). News only.
    Fell = 14,
    // 15 is a spare for conflict news (TK11); 21 to 31 are reserved. Neither is a kind.
    /// Pirates raided a port (the ports agent). `who` is the haven, `whom` the port village.
    /// News only.
    PortRaided = 16,
    /// A haven holds most of a port's lanes. `who` is that haven, `whom` the port. News only.
    Blockade = 17,
    /// A port closed under a blockade. `who` is the haven that closed it, `whom` the port. News
    /// only; Java also reads it as the PORT_CLOSED mood.
    PortClosed = 18,
    /// A port reopened. `who` is the haven whose blockade lifted, `whom` the port. News only.
    PortReopened = 19,
    /// A haven fell. `who` is the navy's home port that broke it, `whom` the haven. News only.
    HavenFell = 20,
}

impl ReportKind {
    /// How many kinds there are. The ids have gaps (15 and 21 and up are reserved, TK11), so this
    /// is the length of [`ReportKind::ALL`] and NOT the size of a per-kind table: size a table by
    /// [`ReportKind::SLOTS`] and index it with [`ReportKind::index`].
    pub const COUNT: usize = 19;
    /// The highest id: the size of every per-kind table, where the gap ids are unused slots.
    pub const SLOTS: usize = 20;
    pub const ALL: [ReportKind; ReportKind::COUNT] = [
        ReportKind::Assault,
        ReportKind::Killing,
        ReportKind::Robbery,
        ReportKind::Theft,
        ReportKind::Trade,
        ReportKind::Rescue,
        ReportKind::ResidentAssault,
        ReportKind::ResidentKilling,
        ReportKind::Encroachment,
        ReportKind::Feud,
        ReportKind::PactSworn,
        ReportKind::PactBroken,
        ReportKind::Raid,
        ReportKind::Fell,
        ReportKind::PortRaided,
        ReportKind::Blockade,
        ReportKind::PortClosed,
        ReportKind::PortReopened,
        ReportKind::HavenFell,
    ];

    /// The slot of a per-kind table, `id - 1`, always below [`ReportKind::SLOTS`].
    pub const fn index(self) -> usize {
        self as usize - 1
    }

    /// The kind with this id; `None` for 0, the gaps and anything past the highest.
    pub fn from_u16(v: u16) -> Option<ReportKind> {
        Some(match v {
            1 => ReportKind::Assault,
            2 => ReportKind::Killing,
            3 => ReportKind::Robbery,
            4 => ReportKind::Theft,
            5 => ReportKind::Trade,
            6 => ReportKind::Rescue,
            7 => ReportKind::ResidentAssault,
            8 => ReportKind::ResidentKilling,
            9 => ReportKind::Encroachment,
            10 => ReportKind::Feud,
            11 => ReportKind::PactSworn,
            12 => ReportKind::PactBroken,
            13 => ReportKind::Raid,
            14 => ReportKind::Fell,
            16 => ReportKind::PortRaided,
            17 => ReportKind::Blockade,
            18 => ReportKind::PortClosed,
            19 => ReportKind::PortReopened,
            20 => ReportKind::HavenFell,
            _ => return None,
        })
    }

    /// Kinds that make a holder think better of `who` (C24: rescues count).
    pub const fn is_positive(self) -> bool {
        matches!(self, ReportKind::Trade | ReportKind::Rescue)
    }

    /// Whether this is one of the news kinds (TALK.md §9.4, id [`FIRST_NEWS_KIND`] and up), whose
    /// severity is always a small code (a rung, an outcome, 1 or 2 for a port) and never a
    /// multiple of [`crate::SEVERITY_UNIT`].
    pub const fn is_news(self) -> bool {
        self as u16 >= FIRST_NEWS_KIND
    }

    /// How much a report weighs wherever its severity means "how much" in a structure that lasts:
    /// a notable's memory and the table's importance order. A news kind's rung counts as
    /// [`rung_weight`]; today's kinds keep their severity on the unit scale, where a mild assault
    /// of 20 is legitimately small (TK33).
    pub const fn magnitude(self, severity: u8) -> u8 {
        if self.is_news() {
            rung_weight(severity)
        } else {
            severity
        }
    }
}

/// The first id of the news kinds (TALK.md §9.4).
pub const FIRST_NEWS_KIND: u16 = 10;

/// A [`ReportKind::Raid`]'s severity, the outcome (TK26): the target held (a watched fight the band
/// left bored or done, not routed), the band was beaten (an unwatched raid that took nothing and
/// lost warriors, or a rout), or it plundered (an unwatched raid that took anything).
pub mod raid {
    pub const HELD: u8 = 1;
    pub const BEATEN: u8 = 2;
    pub const PLUNDERED: u8 = 3;
}

/// A [`ReportKind::Fell`]'s severity (TK26): the chief died of age, or was killed in a fight (by a
/// player or not).
pub mod fell {
    pub const DIED: u8 = 1;
    pub const KILLED: u8 = 2;
}

/// What a rung weighs against the unit scale: 100 plus 25 a step to four, so a rung 1 outweighs a
/// fair trade's 100, a rung 4 outweighs it by half, and none weighs less than the unit. The one
/// formula for "a code, not a multiple of the unit": the structures above use it for the news
/// kinds, and a talker's ranking also for today's kinds whose severity is under 25. Since MZB-36
/// (b053f0f5) the conflict records its robberies, rescues and killings on the unit scale like
/// everyone else, so for today's kinds that is only a guard against a stray small code.
pub const fn rung_weight(rung: u8) -> u8 {
    100 + 25 * if rung < 4 { rung } else { 4 }
}

const _: () = assert!(rung_weight(255) == 200);

/// Bit 0 of [`Report::flags`]: `who` or `whom` is a player. Set by `Knowledge::record`, and
/// what the table's importance order ranks first (SCALE.md §6). Bits 1 to 7 are the caller's.
pub const FLAG_PLAYER: u8 = 1;

/// A report, unpacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub who: ActorId,
    pub whom: ActorId,
    pub kind: ReportKind,
    /// How severe, on [`crate::tunables::SEVERITY_UNIT`] = the kind's base delta.
    pub severity: u8,
    pub flags: u8,
    /// Must fit 48 bits (about 445,000 in-game years); later ticks are stored as the maximum.
    pub at: Tick,
    pub place: Place,
}

impl Report {
    pub fn pack(&self) -> ReportRow {
        let mut b = [0u8; 32];
        b[0..6].copy_from_slice(&self.who.to_bytes());
        b[6..12].copy_from_slice(&self.whom.to_bytes());
        b[12..14].copy_from_slice(&(self.kind as u16).to_le_bytes());
        b[14] = self.severity;
        b[15] = self.flags;
        debug_assert!(
            self.at <= Tick::MAX_48,
            "report time {:?} past 48 bits",
            self.at
        );
        let at = self
            .at
            .to_u48()
            .unwrap_or_else(|| Tick::MAX_48.to_u48().unwrap());
        b[16..22].copy_from_slice(&at);
        b[22..24].copy_from_slice(&self.place.frame.to_le_bytes());
        b[24..27].copy_from_slice(&pack_i24(self.place.x));
        b[27..30].copy_from_slice(&pack_i24(self.place.z));
        b[30..32].copy_from_slice(&self.place.y.to_le_bytes());
        ReportRow(b)
    }
}

fn pack_i24(v: i32) -> [u8; 3] {
    debug_assert!(
        (-COORD_LIMIT - 1..=COORD_LIMIT).contains(&v),
        "coordinate {v} does not fit 24 bits"
    );
    let v = v.clamp(-COORD_LIMIT - 1, COORD_LIMIT);
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

fn unpack_i24(b: [u8; 3]) -> i32 {
    // Sign-extend from bit 23.
    i32::from_le_bytes([b[0], b[1], b[2], if b[2] & 0x80 != 0 { 0xff } else { 0 }])
}

/// A report as stored: exactly SCALE.md §9's 32 bytes. who 6, whom 6, kind 2, severity 1, flags 1,
/// time 6 (48-bit tick), frame 2, x 3, z 3, y 2; little endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ReportRow(pub [u8; 32]);

const _: () = assert!(std::mem::size_of::<ReportRow>() == 32);

impl ReportRow {
    /// `None` only for a row whose kind is not one this build knows (a corrupt page).
    pub fn unpack(&self) -> Option<Report> {
        let b = &self.0;
        let kind = ReportKind::from_u16(u16::from_le_bytes([b[12], b[13]]))?;
        Some(Report {
            who: Handle::from_bytes(b[0..6].try_into().unwrap()),
            whom: Handle::from_bytes(b[6..12].try_into().unwrap()),
            kind,
            severity: b[14],
            flags: b[15],
            at: Tick::from_u48(b[16..22].try_into().unwrap()),
            place: Place {
                frame: u16::from_le_bytes([b[22], b[23]]),
                x: unpack_i24(b[24..27].try_into().unwrap()),
                z: unpack_i24(b[27..30].try_into().unwrap()),
                y: i16::from_le_bytes([b[30], b[31]]),
            },
        })
    }

    pub fn kind(&self) -> Option<ReportKind> {
        ReportKind::from_u16(u16::from_le_bytes([self.0[12], self.0[13]]))
    }

    pub fn severity(&self) -> u8 {
        self.0[14]
    }

    pub fn flags(&self) -> u8 {
        self.0[15]
    }

    pub fn at(&self) -> Tick {
        Tick::from_u48(self.0[16..22].try_into().unwrap())
    }
}

/// A stored report and who knows it.
pub struct ReportEntry {
    pub row: ReportRow,
    pub expires: Tick,
    /// Holders that know this report, sorted. The authority for "already heard" (so a noticeboard
    /// that evicted an entry never relearns it as new), and how expiry finds whose counters to
    /// bump and whose boards to clean.
    pub(crate) knowers: Vec<ActorId>,
}

impl ReportEntry {
    pub fn knowers(&self) -> &[ActorId] {
        &self.knowers
    }
}

pub type ReportId = Handle<ReportEntry>;

/// Importance order, least important first: not player-involved before player-involved, then
/// older before newer, then milder before more severe, then by handle for a total order.
pub(crate) type Importance = (bool, u64, u8, u64);

/// A row's magnitude (see [`ReportKind::magnitude`]); the raw severity for a row of no kind.
fn row_magnitude(row: &ReportRow) -> u8 {
    row.kind()
        .map_or(row.severity(), |k| k.magnitude(row.severity()))
}

fn importance(id: ReportId, row: &ReportRow) -> Importance {
    (
        row.flags() & FLAG_PLAYER != 0,
        row.at().0,
        row_magnitude(row),
        id.to_bits(),
    )
}

/// The global report table.
pub struct ReportTable {
    pub(crate) arena: Arena<ReportEntry>,
    pub(crate) order: BTreeSet<Importance>,
    pub(crate) expiry: TimingWheel<ReportId>,
    pub(crate) cap: u32,
}

/// What [`ReportTable::insert`] did.
pub enum Inserted {
    Stored(ReportId),
    /// Stored, and the least important report was evicted to make room.
    Evicted(ReportId, ReportId, ReportEntry),
    /// The table is full of reports that all matter more; the new one was not kept.
    Dropped,
}

impl ReportTable {
    pub fn new(cap: u32) -> Self {
        assert!(cap > 0, "a report table needs room for one report");
        Self {
            arena: Arena::new(),
            order: BTreeSet::new(),
            expiry: TimingWheel::new(Tick::ZERO),
            cap,
        }
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    pub fn get(&self, id: ReportId) -> Option<&ReportEntry> {
        self.arena.get(id)
    }

    pub fn report(&self, id: ReportId) -> Option<Report> {
        self.arena.get(id).and_then(|e| e.row.unpack())
    }

    /// Whether [`ReportTable::insert`] would keep a row: there is room, or it matters more than
    /// the least important report held (a tie keeps the resident). Lets a caller refuse before it
    /// changes anything else.
    pub fn admits(&self, row: &ReportRow) -> bool {
        if self.arena.len() < self.cap as usize {
            return true;
        }
        let least = self.order.first().expect("a full table has an order");
        // The new report would sit at this importance; its id is not known yet, so compare on the
        // other fields.
        let new = (
            row.flags() & FLAG_PLAYER != 0,
            row.at().0,
            row_magnitude(row),
        );
        new > (least.0, least.1, least.2)
    }

    pub fn insert(&mut self, row: ReportRow, expires: Tick) -> Inserted {
        let mut evicted = None;
        if !self.admits(&row) {
            return Inserted::Dropped;
        }
        if self.arena.len() >= self.cap as usize {
            let least = *self.order.first().expect("a full table has an order");
            self.order.remove(&least);
            let old = Handle::from_bits(least.3);
            let entry = self.arena.remove(old).expect("order names live reports");
            evicted = Some((old, entry));
        }
        let id = self.arena.insert(ReportEntry {
            row,
            expires,
            knowers: Vec::new(),
        });
        self.order.insert(importance(id, &row));
        self.expiry.schedule(expires.max(self.expiry.now()), id);
        match evicted {
            Some((old, entry)) => Inserted::Evicted(id, old, entry),
            None => Inserted::Stored(id),
        }
    }

    pub fn remove(&mut self, id: ReportId) -> Option<ReportEntry> {
        let entry = self.arena.remove(id)?;
        self.order.remove(&importance(id, &entry.row));
        Some(entry)
    }

    /// The next report due to expire at or before `at`, removed. Wakes for reports already evicted
    /// are stale (the handle no longer validates) and skipped.
    pub fn pop_expired(&mut self, at: Tick) -> Option<(ReportId, ReportEntry)> {
        while let Some((_, id)) = self.expiry.pop_until(at) {
            if self.arena.get(id).is_some_and(|e| e.expires <= at) {
                let e = self.remove(id).expect("checked");
                return Some((id, e));
            }
        }
        None
    }

    /// The earliest booked expiry, possibly of a report already evicted.
    pub fn next_expiry(&self) -> Option<Tick> {
        self.expiry.next_due()
    }

    pub fn knows(&self, id: ReportId, holder: ActorId) -> bool {
        self.arena
            .get(id)
            .is_some_and(|e| e.knowers.binary_search(&holder).is_ok())
    }

    /// Records that `holder` knows `id`; false if it already did or the report is gone.
    pub fn add_knower(&mut self, id: ReportId, holder: ActorId) -> bool {
        let Some(e) = self.arena.get_mut(id) else {
            return false;
        };
        match e.knowers.binary_search(&holder) {
            Ok(_) => false,
            Err(i) => {
                e.knowers.insert(i, holder);
                true
            }
        }
    }

    pub fn remove_knower(&mut self, id: ReportId, holder: ActorId) {
        if let Some(e) = self.arena.get_mut(id)
            && let Ok(i) = e.knowers.binary_search(&holder)
        {
            e.knowers.remove(i);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (ReportId, &ReportEntry)> {
        self.arena.iter()
    }

    pub fn encode(&self, w: &mut Writer) {
        w.var_u64(self.arena.len() as u64);
        for (id, e) in self.arena.iter() {
            w.u64(id.to_bits());
            w.raw(&e.row.0);
            w.u64(e.expires.0);
            w.var_u64(e.knowers.len() as u64);
            for k in &e.knowers {
                w.u64(k.to_bits());
            }
        }
    }

    pub fn heap_bytes(&self) -> usize {
        let knowers: usize = self
            .arena
            .iter()
            .map(|(_, e)| e.knowers.capacity() * 6)
            .sum();
        self.arena.heap_bytes()
            + knowers
            + self.order.len() * (std::mem::size_of::<Importance>() + 8)
            + self.expiry.heap_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn report(at: u64, severity: u8, flags: u8) -> Report {
        Report {
            who: Handle::from_parts(1, 1),
            whom: Handle::from_parts(2, 1),
            kind: ReportKind::Robbery,
            severity,
            flags,
            at: Tick(at),
            place: Place::new(0, 10, 64, -10),
        }
    }

    proptest! {
        #[test]
        fn rows_round_trip(
            wi in any::<u32>(), wg in any::<u16>(), mi in any::<u32>(), mg in any::<u16>(),
            kind in proptest::sample::select(ReportKind::ALL.to_vec()), severity in any::<u8>(), flags in any::<u8>(),
            at in 0u64..(1 << 48), frame in any::<u16>(),
            x in -COORD_LIMIT - 1..=COORD_LIMIT, z in -COORD_LIMIT - 1..=COORD_LIMIT, y in any::<i16>(),
        ) {
            let r = Report {
                who: Handle::from_parts(wi, wg),
                whom: Handle::from_parts(mi, mg),
                kind,
                severity,
                flags,
                at: Tick(at),
                place: Place { frame, x, z, y },
            };
            prop_assert_eq!(r.pack().unpack(), Some(r));
        }
    }

    #[test]
    fn layout_matches_scale_md() {
        let r = Report {
            who: Handle::from_parts(0x0403_0201, 0x0605),
            whom: Handle::from_parts(0x0a09_0807, 0x0c0b),
            kind: ReportKind::Theft,
            severity: 0x0f,
            flags: 0x10,
            at: Tick(0x1615_1413_1211),
            place: Place {
                frame: 0x1817,
                x: -2,
                z: 0x1d_1c1b,
                y: 0x1f1e,
            },
        };
        let b = r.pack().0;
        assert_eq!(&b[0..12], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(&b[12..16], &[4, 0, 0x0f, 0x10]);
        assert_eq!(&b[16..22], &[0x11, 0x12, 0x13, 0x14, 0x15, 0x16]);
        assert_eq!(&b[22..24], &[0x17, 0x18]);
        assert_eq!(
            &b[24..27],
            &[0xfe, 0xff, 0xff],
            "x = -2 in 24-bit two's complement"
        );
        assert_eq!(&b[27..32], &[0x1b, 0x1c, 0x1d, 0x1e, 0x1f]);
    }

    /// Every id round-trips through `from_u16` for the real kinds, and the gaps (0, 15, 21 up)
    /// and everything past the highest are `None` (TK11).
    #[test]
    fn ids_round_trip_and_the_gaps_are_none() {
        let gaps = [0u16, 15, 21, 22, 23, 24, 31, 32, 33, 40, 255, u16::MAX];
        for id in 0..=40u16 {
            match ReportKind::from_u16(id) {
                Some(k) => {
                    assert_eq!(k as u16, id, "id {id} comes back as {k:?}");
                    assert!(ReportKind::ALL.contains(&k), "{k:?} is missing from ALL");
                    assert!(!gaps.contains(&id), "id {id} is a gap");
                }
                None => assert!(
                    id == 0 || id == 15 || id >= 21,
                    "id {id} is a real kind but from_u16 refuses it"
                ),
            }
        }
        for g in gaps {
            assert_eq!(ReportKind::from_u16(g), None, "id {g}");
        }
        // ALL is exactly the ids that parse, once each, in id order.
        let parsed: Vec<ReportKind> = (0..=ReportKind::SLOTS as u16 + 1)
            .filter_map(ReportKind::from_u16)
            .collect();
        assert_eq!(parsed, ReportKind::ALL.to_vec());
        assert_eq!(ReportKind::COUNT, 19);
        assert_eq!(ReportKind::SLOTS, 20);
        for k in ReportKind::ALL {
            assert_eq!(ReportKind::from_u16(k as u16), Some(k));
        }
        // The ids TALK owns, pinned: the packed row's kind field never changes.
        let want = [
            (ReportKind::Feud, 10),
            (ReportKind::PactSworn, 11),
            (ReportKind::PactBroken, 12),
            (ReportKind::Raid, 13),
            (ReportKind::Fell, 14),
            (ReportKind::PortRaided, 16),
            (ReportKind::Blockade, 17),
            (ReportKind::PortClosed, 18),
            (ReportKind::PortReopened, 19),
            (ReportKind::HavenFell, 20),
        ];
        for (k, id) in want {
            assert_eq!(k as u16, id, "{k:?}");
        }
    }

    /// One formula for a rung (TK33): the news kinds always are rungs, today's kinds keep the unit
    /// scale in the structures that last.
    #[test]
    fn magnitude_counts_news_rungs_on_the_unit_scale() {
        use ReportKind::*;
        for (kind, severity, want) in [
            (Feud, 1, 125),
            (Feud, 4, 200),
            (Feud, 9, 200),
            (Feud, 100, 200),
            (PortClosed, 0, 100),
            (Raid, 3, 175),
            // Today's kinds are on the unit scale: a mild assault of 20 stays small.
            (Assault, 20, 20),
            (Assault, 100, 100),
            (Robbery, 2, 2),
            (Killing, 255, 255),
        ] {
            assert_eq!(kind.magnitude(severity), want, "{kind:?} {severity}");
        }
        assert_eq!(rung_weight(0), 100);
        assert_eq!(rung_weight(2), 150);
        assert_eq!(rung_weight(4), 200);
        assert_eq!(rung_weight(200), 200, "past four is four");
        assert_eq!(FIRST_NEWS_KIND, Feud as u16);
        assert!(
            ReportKind::ALL
                .iter()
                .all(|k| k.is_news() == (*k as u16 >= 10)),
            "the news kinds are the ids from 10"
        );
    }

    /// The board is full of reports, and a rung 1 feud outranks a fair trade of severity 100: the
    /// table keeps the feud and evicts the trade. By raw severity (1 against 100) it would have
    /// dropped the feud.
    #[test]
    fn a_rung_is_not_the_least_important_report_on_a_full_table() {
        let mut t = ReportTable::new(1);
        let trade = Report {
            kind: ReportKind::Trade,
            ..report(50, 100, 0)
        };
        let feud = Report {
            kind: ReportKind::Feud,
            ..report(50, 1, 0)
        };
        assert!(t.admits(&trade.pack()), "room");
        let Inserted::Stored(_) = t.insert(trade.pack(), Tick(1_000)) else {
            panic!("stored");
        };
        assert!(t.admits(&feud.pack()), "125 outweighs 100 at the same time");
        match t.insert(feud.pack(), Tick(1_000)) {
            Inserted::Evicted(_, _, old) => assert_eq!(old.row.kind(), Some(ReportKind::Trade)),
            _ => panic!("the feud should evict the trade"),
        }
        // The other way round: a fair trade does not push a feud out, and `admits` says so
        // without changing anything.
        assert!(!t.admits(&trade.pack()));
        assert!(matches!(
            t.insert(trade.pack(), Tick(1_000)),
            Inserted::Dropped
        ));
        assert_eq!(t.len(), 1);
    }

    /// A table indexed by `index()` has a slot for every kind, and no id that parses to a kind can
    /// reach a gap slot: a gap id parses to `None`, so there is nothing to index with.
    #[test]
    fn a_table_indexed_by_a_gap_id_cannot_be_reached() {
        let mut hit = [false; ReportKind::SLOTS];
        for id in 0..=u16::MAX {
            if let Some(k) = ReportKind::from_u16(id) {
                assert!(k.index() < ReportKind::SLOTS, "{k:?}");
                hit[k.index()] = true;
            }
        }
        for (slot, reached) in hit.iter().enumerate() {
            let id = slot + 1;
            assert_eq!(*reached, id != 15, "slot {slot} (id {id})");
        }
    }

    /// Every kind (new ones included) packs into the 32-byte row and unpacks as the same report;
    /// a row whose kind field holds a gap id is a corrupt page and unpacks to `None`.
    #[test]
    fn every_kind_packs_and_a_gap_row_does_not_unpack() {
        for k in ReportKind::ALL {
            let r = Report {
                kind: k,
                ..report(777, 3, 0)
            };
            let row = r.pack();
            assert_eq!(&row.0[12..14], &(k as u16).to_le_bytes(), "{k:?}");
            assert_eq!(row.unpack(), Some(r), "{k:?}");
            assert_eq!(row.kind(), Some(k));
        }
        for gap in [0u16, 15, 21, 31, 32, 40] {
            let mut row = report(1, 1, 0).pack();
            row.0[12..14].copy_from_slice(&gap.to_le_bytes());
            assert_eq!(row.unpack(), None, "kind {gap}");
            assert_eq!(row.kind(), None);
        }
    }

    #[test]
    fn eviction_takes_the_least_important_never_the_strongest() {
        let mut t = ReportTable::new(3);
        let Inserted::Stored(player) = t.insert(report(5, 10, FLAG_PLAYER).pack(), Tick(1000))
        else {
            panic!()
        };
        let Inserted::Stored(old) = t.insert(report(1, 200, 0).pack(), Tick(1000)) else {
            panic!()
        };
        let Inserted::Stored(newer) = t.insert(report(9, 1, 0).pack(), Tick(1000)) else {
            panic!()
        };
        // Full. A new non-player report evicts the oldest non-player one, even though that one
        // is the most severe non-player report: recency ranks before magnitude (SCALE.md §6).
        match t.insert(report(10, 1, 0).pack(), Tick(1000)) {
            Inserted::Evicted(_, gone, _) => assert_eq!(gone, old),
            _ => panic!("expected an eviction"),
        }
        assert!(t.get(player).is_some(), "player involvement ranks first");
        assert!(t.get(newer).is_some());
        // A new report that matters less than everything resident is dropped.
        let mut full = ReportTable::new(1);
        full.insert(report(5, 10, FLAG_PLAYER).pack(), Tick(1000));
        assert!(matches!(
            full.insert(report(6, 255, 0).pack(), Tick(1000)),
            Inserted::Dropped
        ));
    }

    #[test]
    fn expiry_pops_in_time_order_and_skips_evicted() {
        let mut t = ReportTable::new(10);
        let Inserted::Stored(a) = t.insert(report(0, 1, 0).pack(), Tick(500)) else {
            panic!()
        };
        let Inserted::Stored(b) = t.insert(report(0, 1, 0).pack(), Tick(100)) else {
            panic!()
        };
        t.remove(a);
        assert_eq!(t.next_expiry(), Some(Tick(100)));
        assert_eq!(t.pop_expired(Tick(99)).map(|x| x.0), None);
        assert_eq!(t.pop_expired(Tick(1000)).map(|x| x.0), Some(b));
        assert!(t.pop_expired(Tick(1000)).is_none(), "a's wake is stale");
        assert!(t.is_empty());
    }
}
