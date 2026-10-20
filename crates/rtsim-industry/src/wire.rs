// SPDX-License-Identifier: AGPL-3.0-or-later
//! The reference byte codec for the records of docs/industry-boundary.md: industry observations
//! (latest-wins 0x0040 to 0x005F, journaled 0x0400 to 0x04FF) and industry commands (0x8400 to
//! 0x84FF).
//!
//! Every record is little-endian and starts with the 8-byte header `u16 kind, u16 size, u32 aux`;
//! `size` includes the header, is a multiple of 8 and at most [`MAX_RECORD`]; padding and reserved
//! fields are written as zero and ignored on read. Journaled observations carry `u64 obsSeq` at 8
//! and `i64 gameTime` at 16; latest-wins observations carry `i64 gameTime` at 8; commands carry
//! `u64 cmdSeq` at 8 and their body from 16.
//!
//! Decoders read fields at the absolute offsets the spec tables give. Encoders write the same
//! fields in order. The two are independent, so the round-trip tests check one against the other.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;

use crate::cmd::{
    BlockDestroy, CarriageDelta, CarriageReason, ChunkReconcile, Cmd, DepositDelta, InvDelta,
    LedgerLine, WritePackage,
};
use crate::glass;
use crate::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodVec, StationId, TrainId};
use crate::obs::{
    BlockFlags, Carriage, CarriageResult, CarriageResultEntry, Conditions, DefenceSurvey,
    DepositPluginId, DepositSample, Direction, DiscardReason, DistrictDecl, EditKind, ExposedBlock,
    FightObserved, InvKind, InventoryDecl, LoadState, NotLearnable, Obs, PackageEvent, PortKind,
    PortTransfer, ReconcileResult, ResultEntry, SpawnSample, StationDecl, SweepEnd, Timed,
    TrainCall, TrainDeparture,
};
use crate::threat::ThreatClass;

/// The largest record, header included (docs/boundary.md).
pub const MAX_RECORD: usize = 65_528;

// Latest-wins observations, Java to engine.
pub const IND_STOCK_SAMPLE: u16 = 0x0040;
pub const IND_SWEEP_END: u16 = 0x0041;
pub const IND_DEPOSIT_SAMPLE: u16 = 0x0042;
// 0x0043 was IND_CHANNEL_SAMPLE: retired with Ender Transmission channels as shared stock (I13 as
// corrected), and read as an unknown kind.
pub const IND_LEDGER_REQUEST: u16 = 0x0044;
pub const IND_WATCHED: u16 = 0x0045;
/// The Java side's proposal H23 (docs/industry-boundary.md §6.1), read as data (W16).
pub const IND_APPROACH: u16 = 0x0046;

// Journaled observations, Java to engine.
pub const IND_DISTRICT: u16 = 0x0400;
pub const IND_DISTRICT_CHUNKS: u16 = 0x0401;
pub const IND_INVENTORY: u16 = 0x0402;
pub const IND_DISTRICT_REMOVED: u16 = 0x0403;
pub const IND_EDIT: u16 = 0x0404;
pub const IND_DISCARD: u16 = 0x0405;
pub const IND_LOAD_STATE: u16 = 0x0406;
pub const IND_CHUNK_LOADED: u16 = 0x0407;
pub const IND_CONDITIONS: u16 = 0x0408;
pub const IND_POWER_LINK: u16 = 0x0409;
/// A chunk of a district unloaded (ChunkEvent.Unload): it rejoins the replay of the district's
/// unloaded part (I7, docs/industry-boundary.md §3.2).
pub const IND_CHUNK_UNLOADED: u16 = 0x040A;
pub const IND_PORT: u16 = 0x0410;
pub const IND_TRAIN_ARRIVED: u16 = 0x0411;
pub const IND_TRAIN_DEPARTED: u16 = 0x0412;
pub const IND_STATION: u16 = 0x0413;
pub const IND_PACKAGE: u16 = 0x0414;
pub const IND_RECONCILE_RESULT: u16 = 0x0420;
pub const IND_CARRIAGE_RESULT: u16 = 0x0421;
pub const IND_SPAWN_SAMPLE: u16 = 0x0430;
pub const IND_FIGHT: u16 = 0x0431;
pub const IND_SURVEY: u16 = 0x0432;
pub const IND_SURVEY_BLOCKS: u16 = 0x0433;

// Commands, engine to Java.
pub const IND_CHUNK_RECONCILE: u16 = 0x8400;
pub const IND_CARRIAGE_DELTA: u16 = 0x8401;
// 0x8402 was IND_CHANNEL_DELTA: retired with IND_CHANNEL_SAMPLE.
pub const IND_DEPOSIT_DELTA: u16 = 0x8403;
pub const IND_WRITE_PACKAGE: u16 = 0x8404;
pub const IND_LEDGER: u16 = 0x8405;

const OBS_KINDS: [u16; 28] = [
    IND_STOCK_SAMPLE,
    IND_SWEEP_END,
    IND_DEPOSIT_SAMPLE,
    IND_LEDGER_REQUEST,
    IND_WATCHED,
    IND_APPROACH,
    IND_DISTRICT,
    IND_DISTRICT_CHUNKS,
    IND_INVENTORY,
    IND_DISTRICT_REMOVED,
    IND_EDIT,
    IND_DISCARD,
    IND_LOAD_STATE,
    IND_CHUNK_LOADED,
    IND_CHUNK_UNLOADED,
    IND_CONDITIONS,
    IND_POWER_LINK,
    IND_PORT,
    IND_TRAIN_ARRIVED,
    IND_TRAIN_DEPARTED,
    IND_STATION,
    IND_PACKAGE,
    IND_RECONCILE_RESULT,
    IND_CARRIAGE_RESULT,
    IND_SPAWN_SAMPLE,
    IND_FIGHT,
    IND_SURVEY,
    IND_SURVEY_BLOCKS,
];

const CMD_KINDS: [u16; 5] = [
    IND_CHUNK_RECONCILE,
    IND_CARRIAGE_DELTA,
    IND_DEPOSIT_DELTA,
    IND_WRITE_PACKAGE,
    IND_LEDGER,
];

/// `Option<u32>` inventory index: none.
const NO_INDEX: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Fewer bytes than a header, or than the header's `size`.
    Truncated,
    /// The header's size is malformed, or differs from the size the kind's counts imply.
    BadSize { kind: u16, size: usize },
    /// An enum field holds a code the spec does not define.
    BadCode {
        kind: u16,
        field: &'static str,
        value: u64,
    },
    /// The record would exceed [`MAX_RECORD`]; nothing was written.
    TooBig { kind: u16, size: usize },
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::Truncated => write!(f, "record truncated"),
            WireError::BadSize { kind, size } => {
                write!(f, "record 0x{kind:04x} has a bad size ({size} bytes)")
            }
            WireError::BadCode { kind, field, value } => {
                write!(f, "record 0x{kind:04x} has an unknown {field} code {value}")
            }
            WireError::TooBig { kind, size } => write!(
                f,
                "record 0x{kind:04x} would be {size} bytes, over the {MAX_RECORD} limit"
            ),
        }
    }
}

impl std::error::Error for WireError {}

// ---------------------------------------------------------------------------------------------
// Enum codes (docs/industry-boundary.md §3.2, §4).

macro_rules! codes {
    ($to:ident, $from:ident, $ty:ty, $field:literal, $($code:literal => $v:path),+ $(,)?) => {
        fn $to(v: $ty) -> u32 {
            match v {
                $($v => $code,)+
            }
        }

        fn $from(kind: u16, code: u64) -> Result<$ty, WireError> {
            match code {
                $($code => Ok($v),)+
                _ => Err(WireError::BadCode { kind, field: $field, value: code }),
            }
        }
    };
}

codes!(plugin_code, plugin_from, DepositPluginId, "plugin",
    1 => DepositPluginId::OreExcavation,
    2 => DepositPluginId::DieselOil,
    3 => DepositPluginId::DestroyCrude);

codes!(edit_code, edit_from, EditKind, "edit_kind",
    1 => EditKind::Placed,
    2 => EditKind::Broken,
    3 => EditKind::Configured,
    4 => EditKind::Command);

codes!(discard_code, discard_from, DiscardReason, "discard_reason",
    1 => DiscardReason::UnattributedSession,
    2 => DiscardReason::CreativeBlock,
    3 => DiscardReason::Overstressed,
    4 => DiscardReason::ProgressiveContraption,
    5 => DiscardReason::Other);

codes!(load_code, load_from, LoadState, "load_state",
    0 => LoadState::Unloaded,
    1 => LoadState::Partial,
    2 => LoadState::Live);

codes!(port_code, port_from, PortKind, "port_kind",
    1 => PortKind::TrainCargo,
    2 => PortKind::Package,
    3 => PortKind::PlayerSession,
    4 => PortKind::PlayerToss,
    5 => PortKind::PlayerPickup,
    6 => PortKind::StockBroken,
    8 => PortKind::Reconcile,
    9 => PortKind::Other);

codes!(direction_code, direction_from, Direction, "direction",
    1 => Direction::In,
    2 => Direction::Out);

codes!(class_code, class_from, ThreatClass, "threat_class",
    0 => ThreatClass::GroundMelee,
    1 => ThreatClass::GroundRanged,
    2 => ThreatClass::Flying,
    3 => ThreatClass::Siege);

codes!(reason_code, reason_from, CarriageReason, "carriage_reason",
    1 => CarriageReason::Exchange,
    2 => CarriageReason::Loot);

// ---------------------------------------------------------------------------------------------
// Writer and reader.

/// Appends one record to a buffer. Puts go at the current position; `finish` pads to 8 and patches
/// the header's size. Bytes past [`MAX_RECORD`] are counted but not stored, so an oversize record
/// costs no memory and `finish` removes whatever was written.
pub(crate) struct W<'a> {
    out: &'a mut Vec<u8>,
    start: usize,
    pos: usize,
    kind: u16,
}

impl<'a> W<'a> {
    pub(crate) fn header(out: &'a mut Vec<u8>, kind: u16, aux: u32) -> Self {
        let start = out.len();
        let mut w = W {
            out,
            start,
            pos: 0,
            kind,
        };
        w.put_u16(kind);
        w.put_u16(0);
        w.put_u32(aux);
        w
    }

    /// Header, then `obsSeq` (journaled kinds only), then `gameTime`.
    pub(crate) fn obs(
        out: &'a mut Vec<u8>,
        kind: u16,
        aux: u32,
        seq: Option<u64>,
        at: Tick,
    ) -> Self {
        debug_assert_eq!(seq.is_some(), kind >= 0x0400);
        let mut w = Self::header(out, kind, aux);
        if let Some(s) = seq {
            w.put_u64(s);
        }
        w.put_i64(at.0 as i64);
        w
    }

    /// Header, then `cmdSeq`.
    pub(crate) fn cmd(out: &'a mut Vec<u8>, kind: u16, aux: u32, seq: u64) -> Self {
        let mut w = Self::header(out, kind, aux);
        w.put_u64(seq);
        w
    }

    pub(crate) fn raw(&mut self, b: &[u8]) {
        self.pos += b.len();
        if self.pos <= MAX_RECORD {
            self.out.extend_from_slice(b);
        }
    }

    pub(crate) fn put_u16(&mut self, v: u16) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_u32(&mut self, v: u32) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_u64(&mut self, v: u64) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_i32(&mut self, v: i32) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_i64(&mut self, v: i64) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_f32(&mut self, v: f32) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn put_f64(&mut self, v: f64) {
        self.raw(&v.to_le_bytes());
    }

    pub(crate) fn zero(&mut self, n: usize) {
        self.pos += n;
        if self.pos <= MAX_RECORD {
            self.out.resize(self.out.len() + n, 0);
        }
    }

    pub(crate) fn pos3(&mut self, p: BlockPos) {
        self.put_i32(p.x);
        self.put_i32(p.y);
        self.put_i32(p.z);
    }

    /// Entry16 list: `u32 good, u32 reserved, i64 amount` each.
    pub(crate) fn goods(&mut self, goods: &GoodVec) {
        for (g, a) in goods.iter() {
            self.put_u32(g.0);
            self.zero(4);
            self.put_i64(a);
        }
    }

    /// Result24 list: `u32 inventory (or carriage), u32 good, i64 requested, i64 applied`.
    pub(crate) fn result(&mut self, first: u32, good: Good, requested: i64, applied: i64) {
        self.put_u32(first);
        self.put_u32(good.0);
        self.put_i64(requested);
        self.put_i64(applied);
    }

    /// Carriage blocks: `u32 index, u32 itemSlots, i64 fluidCapacity, u32 entryCount,
    /// u32 reserved`, then the entries.
    pub(crate) fn carriages(&mut self, carriages: &[Carriage]) {
        for c in carriages {
            self.put_u32(c.index);
            self.put_u32(c.item_slots);
            self.put_i64(c.fluid_capacity);
            self.put_u32(n32(c.goods.len()));
            self.zero(4);
            self.goods(&c.goods);
        }
    }

    pub(crate) fn finish(mut self) -> Result<(), WireError> {
        self.zero((8 - self.pos % 8) % 8);
        if self.pos > MAX_RECORD {
            self.out.truncate(self.start);
            return Err(WireError::TooBig {
                kind: self.kind,
                size: self.pos,
            });
        }
        let size = (self.pos as u16).to_le_bytes();
        self.out[self.start + 2..self.start + 4].copy_from_slice(&size);
        Ok(())
    }
}

/// Reads fields of one record at absolute offsets. The caller has checked the record is long
/// enough for every offset it reads.
pub(crate) struct R<'a>(pub(crate) &'a [u8]);

impl R<'_> {
    pub(crate) fn arr<const N: usize>(&self, o: usize) -> [u8; N] {
        let mut a = [0u8; N];
        a.copy_from_slice(&self.0[o..o + N]);
        a
    }

    pub(crate) fn get_u16(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_u32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_u64(&self, o: usize) -> u64 {
        u64::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_i32(&self, o: usize) -> i32 {
        i32::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_i64(&self, o: usize) -> i64 {
        i64::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_f32(&self, o: usize) -> f32 {
        f32::from_le_bytes(self.arr(o))
    }

    pub(crate) fn get_f64(&self, o: usize) -> f64 {
        f64::from_le_bytes(self.arr(o))
    }

    pub(crate) fn pos3(&self, o: usize) -> BlockPos {
        BlockPos::new(self.get_i32(o), self.get_i32(o + 4), self.get_i32(o + 8))
    }

    pub(crate) fn district(&self, o: usize) -> DistrictId {
        DistrictId(self.get_u64(o))
    }

    /// `n` Entry16 at `o`.
    pub(crate) fn goods(&self, o: usize, n: usize) -> GoodVec {
        (0..n)
            .map(|i| {
                let e = o + 16 * i;
                (Good(self.get_u32(e)), self.get_i64(e + 8))
            })
            .collect()
    }

    /// `n` carriage blocks from `o`; returns them and the offset after the last.
    pub(crate) fn carriages(
        &self,
        kind: u16,
        mut o: usize,
        n: usize,
    ) -> Result<(Vec<Carriage>, usize), WireError> {
        let size = self.0.len();
        let bad = WireError::BadSize { kind, size };
        let mut out = Vec::with_capacity(n.min(size / 24));
        for _ in 0..n {
            if o.saturating_add(24) > size {
                return Err(bad);
            }
            let count = self.get_u32(o + 16) as usize;
            let end = sized(o + 24, count, 16);
            if end > size {
                return Err(bad);
            }
            out.push(Carriage {
                index: self.get_u32(o),
                item_slots: self.get_u32(o + 4),
                fluid_capacity: self.get_i64(o + 8),
                goods: self.goods(o + 24, count),
            });
            o = end;
        }
        Ok((out, o))
    }
}

/// `fixed + n * each`, saturating, so a hostile count never wraps to a matching size.
pub(crate) fn sized(fixed: usize, n: usize, each: usize) -> usize {
    fixed.saturating_add(n.saturating_mul(each))
}

pub(crate) fn check(kind: u16, size: usize, want: usize) -> Result<(), WireError> {
    if size == want {
        Ok(())
    } else {
        Err(WireError::BadSize { kind, size })
    }
}

/// A count as a header `aux`; a count too large for it is also too large for [`MAX_RECORD`], which
/// `W::finish` reports.
pub(crate) fn n32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn op_or_none(op: Option<OpId>) -> u64 {
    op.map_or(0, |o| o.0)
}

fn none_if_zero(v: u64) -> Option<OpId> {
    (v != 0).then_some(OpId(v))
}

fn index_or_none(i: Option<u32>) -> u32 {
    i.unwrap_or(NO_INDEX)
}

fn none_if_unset(v: u32) -> Option<u32> {
    (v != NO_INDEX).then_some(v)
}

/// Splits the header off: `(kind, aux, the record trimmed to its size)`. Bytes past `size` are
/// ignored.
fn header(rec: &[u8]) -> Result<(u16, u32, &[u8]), WireError> {
    if rec.len() < 8 {
        return Err(WireError::Truncated);
    }
    let r = R(rec);
    let kind = r.get_u16(0);
    let size = usize::from(r.get_u16(2));
    if size < 8 || size % 8 != 0 {
        return Err(WireError::BadSize { kind, size });
    }
    if size > rec.len() {
        return Err(WireError::Truncated);
    }
    Ok((kind, r.get_u32(4), &rec[..size]))
}

// ---------------------------------------------------------------------------------------------
// Observations.

/// Appends one observation record. `obs_seq` is written only for journaled kinds; latest-wins
/// kinds have no sequence and ignore it. On error nothing is appended.
pub fn encode_obs(obs_seq: u64, t: &Timed, out: &mut Vec<u8>) -> Result<(), WireError> {
    let seq = t.obs.is_journaled().then_some(obs_seq);
    let at = t.at;
    match &t.obs {
        Obs::Glass(g) => glass::codec::encode_obs(obs_seq, at, g, out),
        Obs::StockSample {
            district,
            inventory,
            sweep,
            goods,
        } => {
            let mut w = W::obs(out, IND_STOCK_SAMPLE, n32(goods.len()), seq, at);
            w.put_u64(district.0);
            w.put_u32(*inventory);
            w.put_u32(*sweep);
            w.goods(goods);
            w.finish()
        }
        Obs::SweepEnd(s) => {
            let flags =
                u32::from(s.live) | (u32::from(s.creative) << 1) | (u32::from(s.overstressed) << 2);
            let mut w = W::obs(out, IND_SWEEP_END, flags, seq, at);
            w.put_u64(s.district.0);
            w.put_u32(s.sweep);
            w.put_u32(s.records);
            w.put_u32(s.inventories);
            w.zero(4);
            w.finish()
        }
        Obs::DepositSample(d) => {
            let mut w = W::obs(out, IND_DEPOSIT_SAMPLE, plugin_code(d.plugin), seq, at);
            w.put_u64(d.district.0);
            w.put_u32(d.good.0);
            w.put_u32(d.sweep);
            w.put_i64(d.remaining);
            w.put_u32(d.chunk.dim);
            w.put_i32(d.chunk.x);
            w.put_i32(d.chunk.z);
            w.put_u32(u32::from(d.infinite));
            w.finish()
        }
        Obs::LedgerRequest { district } => {
            let mut w = W::obs(out, IND_LEDGER_REQUEST, 0, seq, at);
            w.put_u64(district.0);
            w.finish()
        }
        Obs::Watched { district, watched } => {
            let mut w = W::obs(out, IND_WATCHED, u32::from(*watched), seq, at);
            w.put_u64(district.0);
            w.finish()
        }
        Obs::Approach {
            district,
            distance,
            eta_ticks,
            closing,
        } => {
            let mut w = W::obs(out, IND_APPROACH, u32::from(*closing), seq, at);
            w.put_u64(district.0);
            w.put_u32(*distance);
            w.put_u32(index_or_none(*eta_ticks));
            w.finish()
        }
        Obs::District(d) => {
            let reason = d.not_learnable.map_or(0, NotLearnable::code);
            let mut w = W::obs(out, IND_DISTRICT, reason, seq, at);
            w.put_u64(d.district.0);
            w.put_u32(d.revision);
            w.put_u32(d.dim);
            w.put_u32(d.name);
            w.put_u32(d.inventory_count);
            w.finish()
        }
        Obs::DistrictChunks {
            district,
            revision,
            part,
            chunks,
        } => {
            let mut w = W::obs(out, IND_DISTRICT_CHUNKS, n32(chunks.len()), seq, at);
            w.put_u64(district.0);
            w.put_u32(*revision);
            w.put_u32(*part);
            for &(x, z) in chunks {
                w.put_i32(x);
                w.put_i32(z);
            }
            w.finish()
        }
        Obs::Inventory(i) => {
            let mut w = W::obs(out, IND_INVENTORY, i.kind.0, seq, at);
            w.put_u64(i.district.0);
            w.put_u32(i.revision);
            w.put_u32(i.index);
            w.pos3(i.pos);
            w.put_u32(i.item_slots);
            w.put_i64(i.fluid_capacity);
            w.put_i64(i.energy_capacity);
            w.finish()
        }
        Obs::DistrictRemoved { district } => {
            let mut w = W::obs(out, IND_DISTRICT_REMOVED, 0, seq, at);
            w.put_u64(district.0);
            w.finish()
        }
        Obs::PowerLink { sink, source } => {
            let mut w = W::obs(out, IND_POWER_LINK, 0, seq, at);
            w.put_u64(sink.0);
            w.put_u64(source.map_or(0, |d| d.0));
            w.finish()
        }
        Obs::Edit {
            district,
            kind,
            player,
            pos,
        } => {
            let mut w = W::obs(out, IND_EDIT, edit_code(*kind), seq, at);
            w.put_u64(district.0);
            w.put_u64(player.0);
            w.put_u64(player.1);
            w.pos3(*pos);
            w.zero(4);
            w.finish()
        }
        Obs::Discard { district, reason } => {
            let mut w = W::obs(out, IND_DISCARD, discard_code(*reason), seq, at);
            w.put_u64(district.0);
            w.finish()
        }
        Obs::LoadState {
            district,
            state,
            ticking_chunks,
            total_chunks,
        } => {
            let mut w = W::obs(out, IND_LOAD_STATE, load_code(*state), seq, at);
            w.put_u64(district.0);
            w.put_u32(*ticking_chunks);
            w.put_u32(*total_chunks);
            w.finish()
        }
        Obs::ChunkLoaded {
            district,
            chunk,
            applied_epoch,
        } => {
            let mut w = W::obs(out, IND_CHUNK_LOADED, 0, seq, at);
            w.put_u64(district.0);
            w.put_u32(chunk.dim);
            w.put_i32(chunk.x);
            w.put_i32(chunk.z);
            w.put_u32(*applied_epoch);
            w.finish()
        }
        Obs::ChunkUnloaded { district, chunk } => {
            let mut w = W::obs(out, IND_CHUNK_UNLOADED, 0, seq, at);
            w.put_u64(district.0);
            w.put_u32(chunk.dim);
            w.put_i32(chunk.x);
            w.put_i32(chunk.z);
            w.put_u32(0);
            w.finish()
        }
        Obs::Conditions(c) => {
            let mut w = W::obs(out, IND_CONDITIONS, 0, seq, at);
            w.put_u32(c.season);
            w.put_u32(c.season_count);
            w.put_i64(c.season_length as i64);
            w.put_i64(c.season_origin.0 as i64);
            w.finish()
        }
        Obs::Port(p) => {
            let mut w = W::obs(out, IND_PORT, n32(p.goods.len()), seq, at);
            w.put_u64(p.district.0);
            w.put_u32(port_code(p.kind));
            w.put_u32(index_or_none(p.inventory));
            w.put_u64(p.counterpart);
            w.put_u64(op_or_none(p.op));
            w.goods(&p.goods);
            w.finish()
        }
        Obs::TrainArrived(c) => {
            let mut w = W::obs(out, IND_TRAIN_ARRIVED, n32(c.carriages.len()), seq, at);
            w.put_u64(c.train.0);
            w.put_u64(c.station.0);
            w.put_u64(c.district.0);
            w.put_u32(u32::from(c.simulated));
            w.zero(4);
            w.carriages(&c.carriages);
            w.finish()
        }
        Obs::TrainDeparted(c) => {
            let mut w = W::obs(out, IND_TRAIN_DEPARTED, n32(c.carriages.len()), seq, at);
            w.put_u64(c.train.0);
            w.put_u64(c.station.0);
            w.put_u64(c.district.0);
            w.put_u64(c.destination.map_or(0, |s| s.0));
            w.put_i64(c.eta_ticks as i64);
            w.carriages(&c.carriages);
            w.finish()
        }
        Obs::Station(s) => {
            let mut w = W::obs(out, IND_STATION, 0, seq, at);
            w.put_u64(s.station.0);
            w.put_u64(s.district.map_or(0, |d| d.0));
            w.put_u32(s.dim);
            w.pos3(s.pos);
            w.put_u32(s.name);
            w.zero(4);
            w.finish()
        }
        Obs::Package(p) => {
            let mut w = W::obs(out, IND_PACKAGE, n32(p.goods.len()), seq, at);
            w.put_u64(p.district.0);
            w.put_u32(p.inventory);
            w.put_u32(direction_code(p.direction));
            w.put_u32(p.address);
            w.put_u32(p.packages);
            w.put_u64(op_or_none(p.op));
            w.goods(&p.goods);
            w.finish()
        }
        Obs::ReconcileResult(r) => {
            let mut w = W::obs(out, IND_RECONCILE_RESULT, n32(r.entries.len()), seq, at);
            w.put_u64(r.op.0);
            w.put_u64(r.district.0);
            w.put_u32(r.epoch);
            w.put_u32(r.chunk.dim);
            w.put_i32(r.chunk.x);
            w.put_i32(r.chunk.z);
            w.put_u32(n32(r.destroyed.len()));
            w.put_u32(r.blocks_skipped);
            for e in &r.entries {
                w.result(e.inventory, e.good, e.requested, e.applied);
            }
            for p in &r.destroyed {
                w.pos3(*p);
                w.put_u32(0);
            }
            w.finish()
        }
        Obs::CarriageResult(r) => {
            let mut w = W::obs(out, IND_CARRIAGE_RESULT, n32(r.entries.len()), seq, at);
            w.put_u64(r.op.0);
            w.put_u64(r.train.0);
            for e in &r.entries {
                w.result(e.carriage, e.good, e.requested, e.applied);
            }
            w.finish()
        }
        Obs::SpawnSample(s) => {
            let mut w = W::obs(out, IND_SPAWN_SAMPLE, 0, seq, at);
            w.put_u64(s.district.0);
            w.put_i64(s.observed_ticks as i64);
            w.put_i64(s.dark_blocks as i64);
            for n in s.spawns {
                w.put_u32(n);
            }
            w.finish()
        }
        Obs::Fight(f) => {
            let mut w = W::obs(out, IND_FIGHT, class_code(f.class), seq, at);
            w.put_u64(f.district.0);
            w.put_u32(f.attackers);
            w.put_f32(f.attacker_strength);
            w.put_u32(f.attackers_killed);
            w.put_u32(f.blocks_destroyed);
            w.put_u32(f.machines_destroyed);
            w.put_u32(f.guards_lost);
            w.put_i64(f.items_stolen);
            w.put_i64(f.duration_ticks as i64);
            w.finish()
        }
        Obs::Survey(s) => {
            let mut w = W::obs(out, IND_SURVEY, u32::from(s.mob_griefing), seq, at);
            w.put_u64(s.district.0);
            w.put_u32(s.revision);
            w.put_u32(s.perimeter_columns);
            w.put_u32(s.perimeter_gaps);
            w.put_u32(s.dark_spawnable);
            w.put_u32(s.machine_blocks);
            w.put_u32(s.roofed_machines);
            w.put_u32(s.kill_machines_at_gaps);
            w.put_u32(s.ammo_good.map_or(0, |g| g.0));
            for v in s.turret_strength {
                w.put_f32(v);
            }
            for g in s.guards {
                w.put_u32(g);
            }
            w.put_u32(s.ammo_per_engagement);
            w.zero(4);
            w.finish()
        }
        Obs::SurveyBlocks {
            district,
            revision,
            part,
            blocks,
        } => {
            let mut w = W::obs(out, IND_SURVEY_BLOCKS, n32(blocks.len()), seq, at);
            w.put_u64(district.0);
            w.put_u32(*revision);
            w.put_u32(*part);
            for b in blocks {
                w.pos3(b.pos);
                w.put_u32(b.state);
                w.put_u32(b.flags.0);
                w.put_u32(index_or_none(b.inventory));
            }
            w.finish()
        }
    }
}

/// Decodes one record (exactly `rec`, header included; bytes past the header's size are ignored).
/// `Ok(None)` for a kind outside the industry observation ranges, or unknown inside them: callers
/// skip those. Returns `obsSeq` (0 for latest-wins kinds) and the observation.
pub fn decode_obs(rec: &[u8]) -> Result<Option<(u64, Timed)>, WireError> {
    if rec.len() < 8 {
        return Err(WireError::Truncated);
    }
    let k = R(rec).get_u16(0);
    if glass::codec::OBS_KINDS.contains(&k) {
        let (kind, aux, rec) = header(rec)?;
        let (seq, at, g) = glass::codec::decode_obs(kind, aux, rec)?;
        return Ok(Some((
            seq,
            Timed {
                at,
                obs: Obs::Glass(g),
            },
        )));
    }
    if !OBS_KINDS.contains(&k) {
        return Ok(None);
    }
    let (kind, aux, rec) = header(rec)?;
    let r = R(rec);
    let size = rec.len();
    let n = aux as usize;
    let obs = match kind {
        IND_STOCK_SAMPLE => {
            check(kind, size, sized(32, n, 16))?;
            Obs::StockSample {
                district: r.district(16),
                inventory: r.get_u32(24),
                sweep: r.get_u32(28),
                goods: r.goods(32, n),
            }
        }
        IND_SWEEP_END => {
            check(kind, size, 40)?;
            Obs::SweepEnd(SweepEnd {
                district: r.district(16),
                sweep: r.get_u32(24),
                records: r.get_u32(28),
                inventories: r.get_u32(32),
                live: aux & 1 != 0,
                creative: aux & 2 != 0,
                overstressed: aux & 4 != 0,
            })
        }
        IND_DEPOSIT_SAMPLE => {
            check(kind, size, 56)?;
            Obs::DepositSample(DepositSample {
                district: r.district(16),
                good: Good(r.get_u32(24)),
                plugin: plugin_from(kind, aux.into())?,
                chunk: ChunkPos {
                    dim: r.get_u32(40),
                    x: r.get_i32(44),
                    z: r.get_i32(48),
                },
                remaining: r.get_i64(32),
                infinite: r.get_u32(52) & 1 != 0,
                sweep: r.get_u32(28),
            })
        }
        IND_LEDGER_REQUEST => {
            check(kind, size, 24)?;
            Obs::LedgerRequest {
                district: r.district(16),
            }
        }
        IND_WATCHED => {
            check(kind, size, 24)?;
            let watched = match aux {
                0 => false,
                1 => true,
                v => {
                    return Err(WireError::BadCode {
                        kind,
                        field: "watched",
                        value: u64::from(v),
                    });
                }
            };
            Obs::Watched {
                district: r.district(16),
                watched,
            }
        }
        IND_APPROACH => {
            check(kind, size, 32)?;
            let closing = match aux {
                0 => false,
                1 => true,
                v => {
                    return Err(WireError::BadCode {
                        kind,
                        field: "closing",
                        value: u64::from(v),
                    });
                }
            };
            Obs::Approach {
                district: r.district(16),
                distance: r.get_u32(24),
                eta_ticks: none_if_unset(r.get_u32(28)),
                closing,
            }
        }
        IND_DISTRICT => {
            check(kind, size, 48)?;
            Obs::District(DistrictDecl {
                district: r.district(24),
                revision: r.get_u32(32),
                dim: r.get_u32(36),
                name: r.get_u32(40),
                not_learnable: NotLearnable::from_code(aux),
                inventory_count: r.get_u32(44),
            })
        }
        IND_DISTRICT_CHUNKS => {
            check(kind, size, sized(40, n, 8))?;
            Obs::DistrictChunks {
                district: r.district(24),
                revision: r.get_u32(32),
                part: r.get_u32(36),
                chunks: (0..n)
                    .map(|i| (r.get_i32(40 + 8 * i), r.get_i32(44 + 8 * i)))
                    .collect(),
            }
        }
        IND_INVENTORY => {
            check(kind, size, 72)?;
            Obs::Inventory(InventoryDecl {
                district: r.district(24),
                revision: r.get_u32(32),
                index: r.get_u32(36),
                pos: r.pos3(40),
                kind: InvKind(aux),
                item_slots: r.get_u32(52),
                fluid_capacity: r.get_i64(56),
                energy_capacity: r.get_i64(64),
            })
        }
        IND_DISTRICT_REMOVED => {
            check(kind, size, 32)?;
            Obs::DistrictRemoved {
                district: r.district(24),
            }
        }
        IND_POWER_LINK => {
            check(kind, size, 40)?;
            let source = r.get_u64(32);
            Obs::PowerLink {
                sink: r.district(24),
                source: (source != 0).then_some(DistrictId(source)),
            }
        }
        IND_EDIT => {
            check(kind, size, 64)?;
            Obs::Edit {
                district: r.district(24),
                kind: edit_from(kind, aux.into())?,
                player: (r.get_u64(32), r.get_u64(40)),
                pos: r.pos3(48),
            }
        }
        IND_DISCARD => {
            check(kind, size, 32)?;
            Obs::Discard {
                district: r.district(24),
                reason: discard_from(kind, aux.into())?,
            }
        }
        IND_LOAD_STATE => {
            check(kind, size, 40)?;
            Obs::LoadState {
                district: r.district(24),
                state: load_from(kind, aux.into())?,
                ticking_chunks: r.get_u32(32),
                total_chunks: r.get_u32(36),
            }
        }
        IND_CHUNK_LOADED => {
            check(kind, size, 48)?;
            Obs::ChunkLoaded {
                district: r.district(24),
                chunk: ChunkPos {
                    dim: r.get_u32(32),
                    x: r.get_i32(36),
                    z: r.get_i32(40),
                },
                applied_epoch: r.get_u32(44),
            }
        }
        IND_CHUNK_UNLOADED => {
            check(kind, size, 48)?;
            Obs::ChunkUnloaded {
                district: r.district(24),
                chunk: ChunkPos {
                    dim: r.get_u32(32),
                    x: r.get_i32(36),
                    z: r.get_i32(40),
                },
            }
        }
        IND_CONDITIONS => {
            check(kind, size, 48)?;
            Obs::Conditions(Conditions {
                season: r.get_u32(24),
                season_count: r.get_u32(28),
                season_length: r.get_i64(32) as u64,
                season_origin: Tick(r.get_i64(40) as u64),
            })
        }
        IND_PORT => {
            check(kind, size, sized(56, n, 16))?;
            Obs::Port(PortTransfer {
                district: r.district(24),
                kind: port_from(kind, r.get_u32(32).into())?,
                inventory: none_if_unset(r.get_u32(36)),
                counterpart: r.get_u64(40),
                op: none_if_zero(r.get_u64(48)),
                goods: r.goods(56, n),
            })
        }
        IND_TRAIN_ARRIVED => {
            let (carriages, end) = r.carriages(kind, 56, n)?;
            check(kind, size, end)?;
            Obs::TrainArrived(TrainCall {
                train: TrainId(r.get_u64(24)),
                station: StationId(r.get_u64(32)),
                district: r.district(40),
                simulated: r.get_u32(48) & 1 != 0,
                carriages,
            })
        }
        IND_TRAIN_DEPARTED => {
            let (carriages, end) = r.carriages(kind, 64, n)?;
            check(kind, size, end)?;
            let destination = r.get_u64(48);
            Obs::TrainDeparted(TrainDeparture {
                train: TrainId(r.get_u64(24)),
                station: StationId(r.get_u64(32)),
                district: r.district(40),
                destination: (destination != 0).then_some(StationId(destination)),
                eta_ticks: r.get_i64(56) as u64,
                carriages,
            })
        }
        IND_STATION => {
            check(kind, size, 64)?;
            let district = r.get_u64(32);
            Obs::Station(StationDecl {
                station: StationId(r.get_u64(24)),
                district: (district != 0).then_some(DistrictId(district)),
                dim: r.get_u32(40),
                pos: r.pos3(44),
                name: r.get_u32(56),
            })
        }
        IND_PACKAGE => {
            check(kind, size, sized(56, n, 16))?;
            Obs::Package(PackageEvent {
                district: r.district(24),
                inventory: r.get_u32(32),
                direction: direction_from(kind, r.get_u32(36).into())?,
                address: r.get_u32(40),
                packages: r.get_u32(44),
                op: none_if_zero(r.get_u64(48)),
                goods: r.goods(56, n),
            })
        }
        IND_RECONCILE_RESULT => {
            if size < 64 {
                return Err(WireError::BadSize { kind, size });
            }
            let m = r.get_u32(56) as usize;
            check(kind, size, sized(sized(64, n, 24), m, 16))?;
            let blocks = 64 + 24 * n;
            Obs::ReconcileResult(ReconcileResult {
                op: OpId(r.get_u64(24)),
                district: r.district(32),
                epoch: r.get_u32(40),
                chunk: ChunkPos {
                    dim: r.get_u32(44),
                    x: r.get_i32(48),
                    z: r.get_i32(52),
                },
                destroyed: (0..m).map(|i| r.pos3(blocks + 16 * i)).collect(),
                blocks_skipped: r.get_u32(60),
                entries: (0..n)
                    .map(|i| {
                        let e = 64 + 24 * i;
                        ResultEntry {
                            inventory: r.get_u32(e),
                            good: Good(r.get_u32(e + 4)),
                            requested: r.get_i64(e + 8),
                            applied: r.get_i64(e + 16),
                        }
                    })
                    .collect(),
            })
        }
        IND_CARRIAGE_RESULT => {
            check(kind, size, sized(40, n, 24))?;
            Obs::CarriageResult(CarriageResult {
                op: OpId(r.get_u64(24)),
                train: TrainId(r.get_u64(32)),
                entries: (0..n)
                    .map(|i| {
                        let e = 40 + 24 * i;
                        CarriageResultEntry {
                            carriage: r.get_u32(e),
                            good: Good(r.get_u32(e + 4)),
                            requested: r.get_i64(e + 8),
                            applied: r.get_i64(e + 16),
                        }
                    })
                    .collect(),
            })
        }
        IND_SPAWN_SAMPLE => {
            check(kind, size, 64)?;
            Obs::SpawnSample(SpawnSample {
                district: r.district(24),
                observed_ticks: r.get_i64(32) as u64,
                dark_blocks: r.get_i64(40) as u64,
                spawns: std::array::from_fn(|i| r.get_u32(48 + 4 * i)),
            })
        }
        IND_FIGHT => {
            check(kind, size, 72)?;
            Obs::Fight(FightObserved {
                district: r.district(24),
                class: class_from(kind, aux.into())?,
                attackers: r.get_u32(32),
                attacker_strength: r.get_f32(36),
                attackers_killed: r.get_u32(40),
                blocks_destroyed: r.get_u32(44),
                machines_destroyed: r.get_u32(48),
                guards_lost: r.get_u32(52),
                items_stolen: r.get_i64(56),
                duration_ticks: r.get_i64(64) as u64,
            })
        }
        IND_SURVEY => {
            check(kind, size, 104)?;
            let ammo = r.get_u32(60);
            Obs::Survey(DefenceSurvey {
                district: r.district(24),
                revision: r.get_u32(32),
                mob_griefing: aux & 1 != 0,
                perimeter_columns: r.get_u32(36),
                perimeter_gaps: r.get_u32(40),
                dark_spawnable: r.get_u32(44),
                machine_blocks: r.get_u32(48),
                roofed_machines: r.get_u32(52),
                kill_machines_at_gaps: r.get_u32(56),
                turret_strength: std::array::from_fn(|i| r.get_f32(64 + 4 * i)),
                guards: std::array::from_fn(|i| r.get_u32(80 + 4 * i)),
                ammo_good: (ammo != 0).then_some(Good(ammo)),
                ammo_per_engagement: r.get_u32(96),
            })
        }
        IND_SURVEY_BLOCKS => {
            check(kind, size, sized(40, n, 24))?;
            Obs::SurveyBlocks {
                district: r.district(24),
                revision: r.get_u32(32),
                part: r.get_u32(36),
                blocks: (0..n)
                    .map(|i| {
                        let e = 40 + 24 * i;
                        ExposedBlock {
                            pos: r.pos3(e),
                            state: r.get_u32(e + 12),
                            flags: BlockFlags(r.get_u32(e + 16)),
                            inventory: none_if_unset(r.get_u32(e + 20)),
                        }
                    })
                    .collect(),
            }
        }
        _ => return Ok(None),
    };
    // Every arm has checked the size, which covers these fields.
    let journaled = kind >= 0x0400;
    let seq = if journaled { r.get_u64(8) } else { 0 };
    let at = Tick(r.get_i64(if journaled { 16 } else { 8 }) as u64);
    Ok(Some((seq, Timed { at, obs })))
}

// ---------------------------------------------------------------------------------------------
// Commands.

/// Appends one command record with the bridge's `cmdSeq`. On error nothing is appended.
pub fn encode_cmd(cmd_seq: u64, cmd: &Cmd, out: &mut Vec<u8>) -> Result<(), WireError> {
    match cmd {
        Cmd::ProbeSet(p) => glass::codec::encode_probe_set(cmd_seq, p, out),
        Cmd::ChunkReconcile(c) => {
            let mut w = W::cmd(out, IND_CHUNK_RECONCILE, n32(c.deltas.len()), cmd_seq);
            w.put_u64(c.op.0);
            w.put_u64(c.district.0);
            w.put_u32(c.epoch);
            w.put_u32(c.chunk.dim);
            w.put_i32(c.chunk.x);
            w.put_i32(c.chunk.z);
            w.put_u32(n32(c.destroy.len()));
            w.zero(4);
            for d in &c.deltas {
                w.put_u32(d.inventory);
                w.put_u32(d.good.0);
                w.pos3(d.pos);
                w.zero(4);
                w.put_i64(d.delta);
            }
            for b in &c.destroy {
                w.pos3(b.pos);
                w.put_u32(b.expected_state);
            }
            w.finish()
        }
        Cmd::CarriageDelta(c) => {
            let mut w = W::cmd(out, IND_CARRIAGE_DELTA, n32(c.entries.len()), cmd_seq);
            w.put_u64(c.op.0);
            w.put_u64(c.train.0);
            w.put_u32(reason_code(c.reason));
            w.zero(4);
            for &(carriage, good, delta) in &c.entries {
                w.put_u32(carriage);
                w.put_u32(good.0);
                w.put_i64(delta);
            }
            w.finish()
        }
        Cmd::DepositDelta(d) => {
            let mut w = W::cmd(out, IND_DEPOSIT_DELTA, plugin_code(d.plugin), cmd_seq);
            w.put_u64(d.op.0);
            w.put_u32(d.chunk.dim);
            w.put_i32(d.chunk.x);
            w.put_i32(d.chunk.z);
            w.put_u32(d.good.0);
            w.put_i64(d.delta);
            w.finish()
        }
        Cmd::WritePackage(p) => {
            let mut w = W::cmd(out, IND_WRITE_PACKAGE, n32(p.goods.len()), cmd_seq);
            w.put_u64(p.op.0);
            w.put_u64(p.district.0);
            w.put_u32(p.inventory);
            w.pos3(p.pos);
            w.put_u32(p.address);
            w.put_u32(p.count);
            w.goods(&p.goods);
            w.finish()
        }
        Cmd::Ledger(l) => {
            // None is -1, so a tick that does not fit a positive i64 cannot be told from it.
            let breakpoint = match l.next_breakpoint {
                None => -1,
                Some(t) if t.0 <= i64::MAX as u64 => t.0 as i64,
                Some(t) => {
                    return Err(WireError::BadCode {
                        kind: IND_LEDGER,
                        field: "next_breakpoint",
                        value: t.0,
                    });
                }
            };
            let mut w = W::cmd(out, IND_LEDGER, n32(l.goods.len()), cmd_seq);
            w.put_u64(l.district.0);
            w.put_u32(l.state);
            w.put_u32(l.freeze);
            w.put_i64(breakpoint);
            w.put_u32(l.next_kind);
            w.put_u32(l.next_good.map_or(0, |g| g.0));
            w.put_i64(l.since.0 as i64);
            for &(good, per_hour, since_start) in &l.goods {
                w.put_u32(good.0);
                w.zero(4);
                w.put_i64(per_hour);
                w.put_i64(since_start);
            }
            w.finish()
        }
    }
}

/// Decodes one command record (exactly `rec`, header included). `Ok(None)` for a kind outside the
/// industry command range or unknown inside it. Returns `cmdSeq` and the command.
pub fn decode_cmd(rec: &[u8]) -> Result<Option<(u64, Cmd)>, WireError> {
    if rec.len() < 8 {
        return Err(WireError::Truncated);
    }
    if R(rec).get_u16(0) == glass::codec::IND_PROBE_SET {
        let (kind, aux, rec) = header(rec)?;
        let (seq, p) = glass::codec::decode_probe_set(kind, aux, rec)?;
        return Ok(Some((seq, Cmd::ProbeSet(p))));
    }
    if !CMD_KINDS.contains(&R(rec).get_u16(0)) {
        return Ok(None);
    }
    let (kind, aux, rec) = header(rec)?;
    let r = R(rec);
    let size = rec.len();
    let n = aux as usize;
    let cmd = match kind {
        IND_CHUNK_RECONCILE => {
            if size < 56 {
                return Err(WireError::BadSize { kind, size });
            }
            let m = r.get_u32(48) as usize;
            check(kind, size, sized(sized(56, n, 32), m, 16))?;
            let destroy_at = 56 + 32 * n;
            Cmd::ChunkReconcile(ChunkReconcile {
                op: OpId(r.get_u64(16)),
                district: r.district(24),
                epoch: r.get_u32(32),
                chunk: ChunkPos {
                    dim: r.get_u32(36),
                    x: r.get_i32(40),
                    z: r.get_i32(44),
                },
                deltas: (0..n)
                    .map(|i| {
                        let e = 56 + 32 * i;
                        InvDelta {
                            inventory: r.get_u32(e),
                            good: Good(r.get_u32(e + 4)),
                            pos: r.pos3(e + 8),
                            delta: r.get_i64(e + 24),
                        }
                    })
                    .collect(),
                destroy: (0..m)
                    .map(|i| {
                        let e = destroy_at + 16 * i;
                        BlockDestroy {
                            pos: r.pos3(e),
                            expected_state: r.get_u32(e + 12),
                        }
                    })
                    .collect(),
            })
        }
        IND_CARRIAGE_DELTA => {
            check(kind, size, sized(40, n, 16))?;
            Cmd::CarriageDelta(CarriageDelta {
                op: OpId(r.get_u64(16)),
                train: TrainId(r.get_u64(24)),
                reason: reason_from(kind, r.get_u32(32).into())?,
                entries: (0..n)
                    .map(|i| {
                        let e = 40 + 16 * i;
                        (r.get_u32(e), Good(r.get_u32(e + 4)), r.get_i64(e + 8))
                    })
                    .collect(),
            })
        }
        IND_DEPOSIT_DELTA => {
            check(kind, size, 48)?;
            Cmd::DepositDelta(DepositDelta {
                op: OpId(r.get_u64(16)),
                plugin: plugin_from(kind, aux.into())?,
                chunk: ChunkPos {
                    dim: r.get_u32(24),
                    x: r.get_i32(28),
                    z: r.get_i32(32),
                },
                good: Good(r.get_u32(36)),
                delta: r.get_i64(40),
            })
        }
        IND_WRITE_PACKAGE => {
            check(kind, size, sized(56, n, 16))?;
            Cmd::WritePackage(WritePackage {
                op: OpId(r.get_u64(16)),
                district: r.district(24),
                inventory: r.get_u32(32),
                pos: r.pos3(36),
                address: r.get_u32(48),
                count: r.get_u32(52),
                goods: r.goods(56, n),
            })
        }
        IND_LEDGER => {
            check(kind, size, sized(56, n, 24))?;
            let breakpoint = r.get_i64(32);
            let next_good = r.get_u32(44);
            Cmd::Ledger(LedgerLine {
                district: r.district(16),
                state: r.get_u32(24),
                freeze: r.get_u32(28),
                next_breakpoint: match breakpoint {
                    -1 => None,
                    b if b >= 0 => Some(Tick(b as u64)),
                    b => {
                        return Err(WireError::BadCode {
                            kind,
                            field: "next_breakpoint",
                            value: b as u64,
                        });
                    }
                },
                next_kind: r.get_u32(40),
                next_good: (next_good != 0).then_some(Good(next_good)),
                since: Tick(r.get_i64(48) as u64),
                goods: (0..n)
                    .map(|i| {
                        let e = 56 + 24 * i;
                        (Good(r.get_u32(e)), r.get_i64(e + 8), r.get_i64(e + 16))
                    })
                    .collect(),
            })
        }
        _ => return Ok(None),
    };
    Ok(Some((r.get_u64(8), cmd)))
}

// ---------------------------------------------------------------------------------------------
// Walking a buffer area.

/// Iterates the records of a buffer area, yielding `(kind, record bytes)` with the header
/// included. Yields one `Err` and stops on a malformed size (below 8, not a multiple of 8, or
/// past the end of the buffer) or on a tail shorter than a header.
pub struct Records<'a> {
    rest: &'a [u8],
}

impl<'a> Records<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Records { rest: buf }
    }
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<(u16, &'a [u8]), WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        let buf = self.rest;
        if buf.is_empty() {
            return None;
        }
        if buf.len() < 8 {
            self.rest = &[];
            return Some(Err(WireError::Truncated));
        }
        let r = R(buf);
        let kind = r.get_u16(0);
        let size = usize::from(r.get_u16(2));
        if size < 8 || size % 8 != 0 || size > buf.len() {
            self.rest = &[];
            return Some(Err(WireError::BadSize { kind, size }));
        }
        let (rec, rest) = buf.split_at(size);
        self.rest = rest;
        Some(Ok((kind, rec)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const AT: Tick = Tick(123_456_789);
    const SEQ: u64 = 0xDEAD_BEEF_0000_0042;
    const ENGINE_OP: OpId = OpId((0x49 << 56) | 7);
    const JAVA_OP: OpId = OpId((1 << 63) | 9);

    fn goods3() -> GoodVec {
        GoodVec::from_pairs([(Good(2), 150), (Good(7), -33), (Good(9), i64::MAX / 2)])
    }

    fn goods1() -> GoodVec {
        GoodVec::from_pairs([(Good(40), -5)])
    }

    fn carriages() -> Vec<Carriage> {
        vec![
            Carriage {
                index: 0,
                item_slots: 36,
                fluid_capacity: 16_000,
                goods: goods3(),
            },
            Carriage {
                index: 1,
                item_slots: 0,
                fluid_capacity: 0,
                goods: GoodVec::new(),
            },
            Carriage {
                index: 2,
                item_slots: 18,
                fluid_capacity: 8_000,
                goods: goods1(),
            },
        ]
    }

    /// Every observation variant at least once; enums with codes once per code.
    fn all_obs() -> Vec<Obs> {
        let d = DistrictId(0xAABB_CCDD_0000_0001);
        let mut v = vec![
            Obs::StockSample {
                district: d,
                inventory: 12,
                sweep: 3,
                goods: goods3(),
            },
            Obs::StockSample {
                district: d,
                inventory: 0,
                sweep: 0,
                goods: GoodVec::new(),
            },
            Obs::SweepEnd(SweepEnd {
                district: d,
                sweep: 3,
                records: 17,
                inventories: 4,
                live: true,
                creative: false,
                overstressed: true,
            }),
            Obs::SweepEnd(SweepEnd {
                district: d,
                sweep: 4,
                records: 0,
                inventories: 0,
                live: false,
                creative: true,
                overstressed: false,
            }),
            Obs::LedgerRequest { district: d },
            Obs::Watched {
                district: d,
                watched: true,
            },
            Obs::Watched {
                district: d,
                watched: false,
            },
            Obs::Approach {
                district: d,
                distance: 180,
                eta_ticks: Some(400),
                closing: true,
            },
            Obs::Approach {
                district: d,
                distance: 250,
                eta_ticks: None,
                closing: false,
            },
            Obs::District(DistrictDecl {
                district: d,
                revision: 5,
                dim: 2,
                name: 301,
                not_learnable: None,
                inventory_count: 14,
            }),
            Obs::District(DistrictDecl {
                district: d,
                revision: 6,
                dim: 0,
                name: 1,
                not_learnable: Some(NotLearnable::MixtureNotSteady),
                inventory_count: 0,
            }),
            Obs::District(DistrictDecl {
                district: d,
                revision: 7,
                dim: 1,
                name: 2,
                not_learnable: Some(NotLearnable::Other(77)),
                inventory_count: 1,
            }),
            Obs::DistrictChunks {
                district: d,
                revision: 5,
                part: 1,
                chunks: vec![(-3, 4), (0, 0), (i32::MIN, i32::MAX)],
            },
            Obs::DistrictChunks {
                district: d,
                revision: 5,
                part: 0,
                chunks: vec![],
            },
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 5,
                index: 9,
                pos: BlockPos::new(-100, 64, 2_000_000),
                kind: InvKind(InvKind::ITEMS | InvKind::POSTBOX),
                item_slots: 180,
                fluid_capacity: 64_000,
                energy_capacity: 1_000_000,
            }),
            Obs::DistrictRemoved { district: d },
            Obs::PowerLink {
                sink: d,
                source: Some(DistrictId(77)),
            },
            Obs::PowerLink {
                sink: d,
                source: None,
            },
            Obs::ChunkLoaded {
                district: d,
                chunk: ChunkPos {
                    dim: 1,
                    x: -7,
                    z: 12,
                },
                applied_epoch: 3,
            },
            Obs::ChunkUnloaded {
                district: d,
                chunk: ChunkPos {
                    dim: 1,
                    x: -7,
                    z: 12,
                },
            },
            Obs::Conditions(Conditions {
                season: 2,
                season_count: 4,
                season_length: 672_000,
                season_origin: Tick(1_344_000),
            }),
            Obs::Port(PortTransfer {
                district: d,
                kind: PortKind::TrainCargo,
                inventory: Some(4),
                counterpart: 0x1234,
                op: Some(JAVA_OP),
                goods: goods3(),
            }),
            Obs::Port(PortTransfer {
                district: d,
                kind: PortKind::Other,
                inventory: None,
                counterpart: 99,
                op: None,
                goods: GoodVec::new(),
            }),
            Obs::TrainArrived(TrainCall {
                train: TrainId(77),
                station: StationId(78),
                district: d,
                simulated: true,
                carriages: carriages(),
            }),
            Obs::TrainArrived(TrainCall {
                train: TrainId(1),
                station: StationId(2),
                district: d,
                simulated: false,
                carriages: vec![],
            }),
            Obs::TrainDeparted(TrainDeparture {
                train: TrainId(77),
                station: StationId(78),
                district: d,
                destination: Some(StationId(91)),
                eta_ticks: 24_000,
                carriages: carriages(),
            }),
            Obs::TrainDeparted(TrainDeparture {
                train: TrainId(77),
                station: StationId(78),
                district: d,
                destination: None,
                eta_ticks: 0,
                carriages: vec![],
            }),
            Obs::Station(StationDecl {
                station: StationId(78),
                district: Some(d),
                dim: 1,
                pos: BlockPos::new(10, -20, -30),
                name: 17,
            }),
            Obs::Station(StationDecl {
                station: StationId(79),
                district: None,
                dim: 0,
                pos: BlockPos::new(0, 0, 0),
                name: 18,
            }),
            Obs::Package(PackageEvent {
                district: d,
                inventory: 3,
                direction: Direction::In,
                address: 55,
                packages: 6,
                op: Some(ENGINE_OP),
                goods: goods3(),
            }),
            Obs::Package(PackageEvent {
                district: d,
                inventory: 3,
                direction: Direction::Out,
                address: 56,
                packages: 1,
                op: None,
                goods: goods1(),
            }),
            Obs::ReconcileResult(ReconcileResult {
                op: ENGINE_OP,
                district: d,
                epoch: 8,
                chunk: ChunkPos {
                    dim: 0,
                    x: -1,
                    z: -2,
                },
                entries: vec![
                    ResultEntry {
                        inventory: 2,
                        good: Good(5),
                        requested: -100,
                        applied: -60,
                    },
                    ResultEntry {
                        inventory: 3,
                        good: Good(6),
                        requested: 100,
                        applied: 100,
                    },
                ],
                destroyed: vec![BlockPos::new(1, 2, 3), BlockPos::new(-4, 70, 9)],
                blocks_skipped: 1,
            }),
            Obs::CarriageResult(CarriageResult {
                op: ENGINE_OP,
                train: TrainId(77),
                entries: vec![
                    CarriageResultEntry {
                        carriage: 3,
                        good: Good(5),
                        requested: 64,
                        applied: 10,
                    },
                    CarriageResultEntry {
                        carriage: 0,
                        good: Good(8),
                        requested: -1,
                        applied: -1,
                    },
                ],
            }),
            Obs::SpawnSample(SpawnSample {
                district: d,
                observed_ticks: 12_000,
                dark_blocks: 345,
                spawns: [1, 2, 3, 4],
            }),
            Obs::Survey(DefenceSurvey {
                district: d,
                revision: 5,
                mob_griefing: true,
                perimeter_columns: 120,
                perimeter_gaps: 6,
                dark_spawnable: 77,
                machine_blocks: 40,
                roofed_machines: 3,
                kill_machines_at_gaps: 2,
                turret_strength: [0.5, 1.25, -0.0, 1e9],
                guards: [4, 0, 1, 2],
                ammo_good: Some(Good(31)),
                ammo_per_engagement: 12,
            }),
            Obs::Survey(DefenceSurvey {
                district: d,
                revision: 6,
                mob_griefing: false,
                perimeter_columns: 0,
                perimeter_gaps: 0,
                dark_spawnable: 0,
                machine_blocks: 0,
                roofed_machines: 0,
                kill_machines_at_gaps: 0,
                turret_strength: [0.0; 4],
                guards: [0; 4],
                ammo_good: None,
                ammo_per_engagement: 0,
            }),
            Obs::SurveyBlocks {
                district: d,
                revision: 5,
                part: 2,
                blocks: vec![
                    ExposedBlock {
                        pos: BlockPos::new(1, 2, 3),
                        state: 44,
                        flags: BlockFlags(BlockFlags::PERIMETER | BlockFlags::ROOFLESS),
                        inventory: None,
                    },
                    ExposedBlock {
                        pos: BlockPos::new(-1, -2, -3),
                        state: 45,
                        flags: BlockFlags(BlockFlags::MACHINE | BlockFlags::INVENTORY),
                        inventory: Some(6),
                    },
                ],
            },
            Obs::SurveyBlocks {
                district: d,
                revision: 5,
                part: 3,
                blocks: vec![],
            },
        ];
        for plugin in [
            DepositPluginId::OreExcavation,
            DepositPluginId::DieselOil,
            DepositPluginId::DestroyCrude,
        ] {
            v.push(Obs::DepositSample(DepositSample {
                district: d,
                good: Good(77),
                plugin,
                chunk: ChunkPos {
                    dim: 2,
                    x: -5,
                    z: 9,
                },
                remaining: 123_456_789_012,
                infinite: plugin == DepositPluginId::DieselOil,
                sweep: 8,
            }));
        }
        for kind in [
            EditKind::Placed,
            EditKind::Broken,
            EditKind::Configured,
            EditKind::Command,
        ] {
            v.push(Obs::Edit {
                district: d,
                kind,
                player: (0x1111_2222_3333_4444, 0xFFFF_0000_FFFF_0001),
                pos: BlockPos::new(-5, 70, 123),
            });
        }
        for reason in [
            DiscardReason::UnattributedSession,
            DiscardReason::CreativeBlock,
            DiscardReason::Overstressed,
            DiscardReason::ProgressiveContraption,
            DiscardReason::Other,
        ] {
            v.push(Obs::Discard {
                district: d,
                reason,
            });
        }
        for state in [LoadState::Unloaded, LoadState::Partial, LoadState::Live] {
            v.push(Obs::LoadState {
                district: d,
                state,
                ticking_chunks: 2,
                total_chunks: 9,
            });
        }
        for kind in [
            PortKind::Package,
            PortKind::PlayerSession,
            PortKind::PlayerToss,
            PortKind::PlayerPickup,
            PortKind::StockBroken,
            PortKind::Reconcile,
            PortKind::Other,
        ] {
            v.push(Obs::Port(PortTransfer {
                district: d,
                kind,
                inventory: Some(0),
                counterpart: u64::MAX,
                op: Some(ENGINE_OP),
                goods: goods1(),
            }));
        }
        for class in ThreatClass::ALL {
            v.push(Obs::Fight(FightObserved {
                district: d,
                class,
                attackers: 12,
                attacker_strength: 37.5,
                attackers_killed: 9,
                blocks_destroyed: 21,
                machines_destroyed: 2,
                guards_lost: 1,
                items_stolen: 4_000_000_000,
                duration_ticks: 2_400,
            }));
        }
        v
    }

    fn all_cmds() -> Vec<Cmd> {
        let d = DistrictId(0xAABB_CCDD_0000_0001);
        let mut v = vec![
            Cmd::ChunkReconcile(ChunkReconcile {
                op: ENGINE_OP,
                district: d,
                epoch: 4,
                chunk: ChunkPos {
                    dim: 1,
                    x: -9,
                    z: 33,
                },
                deltas: vec![
                    InvDelta {
                        inventory: 2,
                        pos: BlockPos::new(-140, 64, 530),
                        good: Good(5),
                        delta: -1_000,
                    },
                    InvDelta {
                        inventory: 3,
                        pos: BlockPos::new(-141, 65, 531),
                        good: Good(6),
                        delta: 2_500_000_000,
                    },
                ],
                destroy: vec![
                    BlockDestroy {
                        pos: BlockPos::new(-139, 64, 529),
                        expected_state: 812,
                    },
                    BlockDestroy {
                        pos: BlockPos::new(1, 2, 3),
                        expected_state: 1,
                    },
                    BlockDestroy {
                        pos: BlockPos::new(i32::MIN, 0, i32::MAX),
                        expected_state: u32::MAX,
                    },
                ],
            }),
            Cmd::ChunkReconcile(ChunkReconcile {
                op: ENGINE_OP,
                district: d,
                epoch: 5,
                chunk: ChunkPos::default(),
                deltas: vec![],
                destroy: vec![],
            }),
            Cmd::ChunkReconcile(ChunkReconcile {
                op: ENGINE_OP,
                district: d,
                epoch: 6,
                chunk: ChunkPos::default(),
                deltas: vec![],
                destroy: vec![BlockDestroy {
                    pos: BlockPos::new(0, 0, 0),
                    expected_state: 9,
                }],
            }),
            Cmd::WritePackage(WritePackage {
                op: ENGINE_OP,
                district: d,
                inventory: 6,
                pos: BlockPos::new(10, -2, -3),
                address: 99,
                count: 4,
                goods: goods3(),
            }),
            Cmd::Ledger(LedgerLine {
                district: d,
                state: 2,
                freeze: 0,
                next_breakpoint: Some(Tick(1_000_000)),
                next_kind: 3,
                next_good: Some(Good(12)),
                since: Tick(500_000),
                goods: vec![
                    (Good(3), 7_200, -90),
                    (Good(4), -3_600, 123_456),
                    (Good(5), 0, 0),
                ],
            }),
            Cmd::Ledger(LedgerLine {
                district: d,
                state: 4,
                freeze: 5,
                next_breakpoint: None,
                next_kind: 0,
                next_good: None,
                since: Tick(0),
                goods: vec![],
            }),
        ];
        for reason in [CarriageReason::Exchange, CarriageReason::Loot] {
            v.push(Cmd::CarriageDelta(CarriageDelta {
                op: ENGINE_OP,
                train: TrainId(77),
                reason,
                entries: vec![
                    (0, Good(5), -64),
                    (0, Good(6), 8),
                    (3, Good(5), 1_000_000_000_000),
                ],
            }));
        }
        for plugin in [
            DepositPluginId::OreExcavation,
            DepositPluginId::DieselOil,
            DepositPluginId::DestroyCrude,
        ] {
            v.push(Cmd::DepositDelta(DepositDelta {
                op: ENGINE_OP,
                plugin,
                chunk: ChunkPos {
                    dim: 0,
                    x: 40,
                    z: -41,
                },
                good: Good(77),
                delta: -5_000,
            }));
        }
        v
    }

    /// The size column of docs/industry-boundary.md §3, written out independently of the codec.
    fn spec_obs_size(o: &Obs) -> usize {
        let carriage_bytes =
            |cs: &[Carriage]| cs.iter().map(|c| 24 + 16 * c.goods.len()).sum::<usize>();
        let pad8 = |n: usize| n.div_ceil(8) * 8;
        match o {
            Obs::Glass(g) => {
                use crate::glass::records::GlassObs as G;
                match g {
                    G::Node(n) => 112 + 24 * n.rates.len(),
                    G::Edge(e) => pad8(72 + 4 * e.filter.len()),
                    G::Recipe(r) => 56 + 24 * (r.inputs.len() + r.outputs.len()),
                    G::Domain(d) => 72 + 24 * d.levels.len() + 24 * d.members.len(),
                    G::End(_) => 80,
                    G::ProbeSample(p) => 40 + 16 * p.goods.len(),
                    G::ChunkTicking(_) => 48,
                }
            }
            Obs::StockSample { goods, .. } => 32 + 16 * goods.len(),
            Obs::SweepEnd(_) => 40,
            Obs::DepositSample(_) => 56,
            Obs::LedgerRequest { .. } => 24,
            Obs::Watched { .. } => 24,
            Obs::Approach { .. } => 32,
            Obs::District(_) => 48,
            Obs::DistrictChunks { chunks, .. } => 40 + 8 * chunks.len(),
            Obs::Inventory(_) => 72,
            Obs::DistrictRemoved { .. } => 32,
            Obs::PowerLink { .. } => 40,
            Obs::Edit { .. } => 64,
            Obs::Discard { .. } => 32,
            Obs::LoadState { .. } => 40,
            Obs::ChunkLoaded { .. } => 48,
            Obs::ChunkUnloaded { .. } => 48,
            Obs::Conditions(_) => 48,
            Obs::Port(p) => 56 + 16 * p.goods.len(),
            Obs::TrainArrived(c) => 56 + carriage_bytes(&c.carriages),
            Obs::TrainDeparted(c) => 64 + carriage_bytes(&c.carriages),
            Obs::Station(_) => 64,
            Obs::Package(p) => 56 + 16 * p.goods.len(),
            Obs::ReconcileResult(r) => 64 + 24 * r.entries.len() + 16 * r.destroyed.len(),
            Obs::CarriageResult(r) => 40 + 24 * r.entries.len(),
            Obs::SpawnSample(_) => 64,
            Obs::Fight(_) => 72,
            Obs::Survey(_) => 104,
            Obs::SurveyBlocks { blocks, .. } => 40 + 24 * blocks.len(),
        }
    }

    fn spec_obs_kind(o: &Obs) -> u16 {
        match o {
            Obs::Glass(g) => {
                use crate::glass::records::GlassObs as G;
                match g {
                    G::Node(_) => 0x0048,
                    G::Edge(_) => 0x0049,
                    G::Recipe(_) => 0x004A,
                    G::Domain(_) => 0x004B,
                    G::End(_) => 0x004C,
                    G::ProbeSample(_) => 0x004D,
                    G::ChunkTicking(_) => 0x040B,
                }
            }
            Obs::StockSample { .. } => 0x0040,
            Obs::SweepEnd(_) => 0x0041,
            Obs::DepositSample(_) => 0x0042,
            Obs::LedgerRequest { .. } => 0x0044,
            Obs::Watched { .. } => 0x0045,
            Obs::Approach { .. } => 0x0046,
            Obs::District(_) => 0x0400,
            Obs::DistrictChunks { .. } => 0x0401,
            Obs::Inventory(_) => 0x0402,
            Obs::DistrictRemoved { .. } => 0x0403,
            Obs::PowerLink { .. } => 0x0409,
            Obs::Edit { .. } => 0x0404,
            Obs::Discard { .. } => 0x0405,
            Obs::LoadState { .. } => 0x0406,
            Obs::ChunkLoaded { .. } => 0x0407,
            Obs::ChunkUnloaded { .. } => 0x040A,
            Obs::Conditions(_) => 0x0408,
            Obs::Port(_) => 0x0410,
            Obs::TrainArrived(_) => 0x0411,
            Obs::TrainDeparted(_) => 0x0412,
            Obs::Station(_) => 0x0413,
            Obs::Package(_) => 0x0414,
            Obs::ReconcileResult(_) => 0x0420,
            Obs::CarriageResult(_) => 0x0421,
            Obs::SpawnSample(_) => 0x0430,
            Obs::Fight(_) => 0x0431,
            Obs::Survey(_) => 0x0432,
            Obs::SurveyBlocks { .. } => 0x0433,
        }
    }

    /// The size column of docs/industry-boundary.md §4.
    fn spec_cmd_size(c: &Cmd) -> usize {
        match c {
            Cmd::ChunkReconcile(c) => 56 + 32 * c.deltas.len() + 16 * c.destroy.len(),
            Cmd::CarriageDelta(c) => 40 + 16 * c.entries.len(),
            Cmd::DepositDelta(_) => 48,
            Cmd::WritePackage(c) => 56 + 16 * c.goods.len(),
            Cmd::Ledger(c) => 56 + 24 * c.goods.len(),
            Cmd::ProbeSet(p) => (32 + 4 * p.nodes.len()).div_ceil(8) * 8,
        }
    }

    fn spec_cmd_kind(c: &Cmd) -> u16 {
        match c {
            Cmd::ChunkReconcile(_) => 0x8400,
            Cmd::CarriageDelta(_) => 0x8401,
            Cmd::DepositDelta(_) => 0x8403,
            Cmd::WritePackage(_) => 0x8404,
            Cmd::Ledger(_) => 0x8405,
            Cmd::ProbeSet(_) => 0x8406,
        }
    }

    fn le16(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }

    fn le32(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    fn le64(b: &[u8], o: usize) -> u64 {
        u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
    }

    fn enc_obs(o: Obs) -> Vec<u8> {
        let mut out = Vec::new();
        encode_obs(SEQ, &Timed { at: AT, obs: o }, &mut out).unwrap();
        out
    }

    fn enc_cmd(c: &Cmd) -> Vec<u8> {
        let mut out = Vec::new();
        encode_cmd(SEQ, c, &mut out).unwrap();
        out
    }

    #[test]
    fn every_obs_round_trips_at_the_spec_size() {
        let all = all_obs();
        let mut seen = std::collections::BTreeSet::new();
        for obs in all {
            let t = Timed { at: AT, obs };
            let mut out = vec![0xEE; 3];
            encode_obs(SEQ, &t, &mut out).unwrap();
            let rec = &out[3..];
            let kind = spec_obs_kind(&t.obs);
            seen.insert(kind);
            assert_eq!(le16(rec, 0), kind, "{t:?}");
            assert_eq!(usize::from(le16(rec, 2)), rec.len(), "{t:?}");
            assert_eq!(rec.len(), spec_obs_size(&t.obs), "{t:?}");
            assert_eq!(rec.len() % 8, 0);
            assert!(rec.len() <= MAX_RECORD);
            let journaled = kind >= 0x0400;
            if journaled {
                assert_eq!(le64(rec, 8), SEQ);
                assert_eq!(le64(rec, 16), AT.0);
            } else {
                assert_eq!(le64(rec, 8), AT.0);
            }
            let (seq, back) = decode_obs(rec).unwrap().expect("known kind");
            assert_eq!(seq, if journaled { SEQ } else { 0 }, "{t:?}");
            assert_eq!(back, t);
            // The encoding is canonical: decoding and encoding again gives the same bytes.
            let mut again = Vec::new();
            encode_obs(SEQ, &back, &mut again).unwrap();
            assert_eq!(again, rec);
        }
        assert_eq!(seen.len(), OBS_KINDS.len(), "a variant has no sample");
        let mut kinds = OBS_KINDS.to_vec();
        kinds.sort_unstable();
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), kinds);
    }

    #[test]
    fn every_cmd_round_trips_at_the_spec_size() {
        let mut seen = std::collections::BTreeSet::new();
        for cmd in all_cmds() {
            let mut out = vec![0xEE; 5];
            encode_cmd(SEQ, &cmd, &mut out).unwrap();
            let rec = &out[5..];
            let kind = spec_cmd_kind(&cmd);
            seen.insert(kind);
            assert_eq!(le16(rec, 0), kind, "{cmd:?}");
            assert_eq!(usize::from(le16(rec, 2)), rec.len());
            assert_eq!(rec.len(), spec_cmd_size(&cmd), "{cmd:?}");
            assert_eq!(rec.len() % 8, 0);
            assert_eq!(le64(rec, 8), SEQ);
            let (seq, back) = decode_cmd(rec).unwrap().expect("known kind");
            assert_eq!(seq, SEQ);
            assert_eq!(back, cmd);
            let mut again = Vec::new();
            encode_cmd(SEQ, &back, &mut again).unwrap();
            assert_eq!(again, rec);
        }
        let mut kinds = CMD_KINDS.to_vec();
        kinds.sort_unstable();
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), kinds);
    }

    #[test]
    fn kind_constants_match_the_spec_table() {
        let spec: [(u16, u16); 27] = [
            (IND_STOCK_SAMPLE, 0x0040),
            (IND_SWEEP_END, 0x0041),
            (IND_DEPOSIT_SAMPLE, 0x0042),
            (IND_CHUNK_UNLOADED, 0x040A),
            (IND_LEDGER_REQUEST, 0x0044),
            (IND_WATCHED, 0x0045),
            (IND_DISTRICT, 0x0400),
            (IND_DISTRICT_CHUNKS, 0x0401),
            (IND_INVENTORY, 0x0402),
            (IND_DISTRICT_REMOVED, 0x0403),
            (IND_EDIT, 0x0404),
            (IND_DISCARD, 0x0405),
            (IND_LOAD_STATE, 0x0406),
            (IND_CHUNK_LOADED, 0x0407),
            (IND_CONDITIONS, 0x0408),
            (IND_POWER_LINK, 0x0409),
            (IND_PORT, 0x0410),
            (IND_TRAIN_ARRIVED, 0x0411),
            (IND_TRAIN_DEPARTED, 0x0412),
            (IND_STATION, 0x0413),
            (IND_PACKAGE, 0x0414),
            (IND_RECONCILE_RESULT, 0x0420),
            (IND_CARRIAGE_RESULT, 0x0421),
            (IND_SPAWN_SAMPLE, 0x0430),
            (IND_FIGHT, 0x0431),
            (IND_SURVEY, 0x0432),
            (IND_SURVEY_BLOCKS, 0x0433),
        ];
        for (got, want) in spec {
            assert_eq!(got, want);
        }
        assert_eq!(IND_CHUNK_RECONCILE, 0x8400);
        assert_eq!(IND_CARRIAGE_DELTA, 0x8401);
        assert_eq!(IND_DEPOSIT_DELTA, 0x8403);
        assert_eq!(IND_WRITE_PACKAGE, 0x8404);
        assert_eq!(IND_LEDGER, 0x8405);
    }

    #[test]
    fn stock_sample_golden_bytes() {
        let rec = enc_obs(Obs::StockSample {
            district: DistrictId(0x0102_0304_0506_0708),
            inventory: 7,
            sweep: 9,
            goods: GoodVec::from_pairs([(Good(3), 100), (Good(5), -2)]),
        });
        // Latest-wins: gameTime at 8, no obsSeq.
        let mut want = vec![
            0x40, 0x00, // kind
            0x40, 0x00, // size 64
            0x02, 0x00, 0x00, 0x00, // aux: 2 entries
        ];
        want.extend_from_slice(&AT.0.to_le_bytes());
        want.extend_from_slice(&[8, 7, 6, 5, 4, 3, 2, 1]); // district
        want.extend_from_slice(&[7, 0, 0, 0]); // inventory
        want.extend_from_slice(&[9, 0, 0, 0]); // sweep
        want.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0, 0]); // good 3, reserved
        want.extend_from_slice(&100i64.to_le_bytes());
        want.extend_from_slice(&[5, 0, 0, 0, 0, 0, 0, 0]); // good 5, reserved
        want.extend_from_slice(&(-2i64).to_le_bytes());
        assert_eq!(rec, want);
    }

    #[test]
    fn chunk_reconcile_golden_offsets() {
        let cmd = Cmd::ChunkReconcile(ChunkReconcile {
            op: OpId((0x49 << 56) | 0x1234),
            district: DistrictId(0xA1A2_A3A4_A5A6_A7A8),
            epoch: 11,
            chunk: ChunkPos {
                dim: 2,
                x: -3,
                z: 4,
            },
            deltas: vec![
                InvDelta {
                    inventory: 5,
                    pos: BlockPos::new(-48, 70, 64),
                    good: Good(21),
                    delta: -250,
                },
                InvDelta {
                    inventory: 6,
                    pos: BlockPos::new(-47, 71, 65),
                    good: Good(22),
                    delta: 300,
                },
            ],
            destroy: vec![BlockDestroy {
                pos: BlockPos::new(-46, 72, 66),
                expected_state: 999,
            }],
        });
        let rec = enc_cmd(&cmd);
        assert_eq!(rec.len(), 56 + 2 * 32 + 16);
        assert_eq!(le16(&rec, 0), 0x8400);
        assert_eq!(usize::from(le16(&rec, 2)), rec.len());
        assert_eq!(le32(&rec, 4), 2, "aux is the delta count");
        assert_eq!(le64(&rec, 8), SEQ);
        assert_eq!(le64(&rec, 16), (0x49 << 56) | 0x1234);
        assert_eq!(le64(&rec, 24), 0xA1A2_A3A4_A5A6_A7A8);
        assert_eq!(le32(&rec, 32), 11, "epoch");
        assert_eq!(le32(&rec, 36), 2, "dim");
        assert_eq!(le32(&rec, 40) as i32, -3, "chunkX");
        assert_eq!(le32(&rec, 44), 4, "chunkZ");
        assert_eq!(le32(&rec, 48), 1, "destroyCount");
        assert_eq!(le32(&rec, 52), 0, "reserved");
        // Delta32 #0 at 56.
        assert_eq!(le32(&rec, 56), 5);
        assert_eq!(le32(&rec, 60), 21);
        assert_eq!(le32(&rec, 64) as i32, -48);
        assert_eq!(le32(&rec, 68), 70);
        assert_eq!(le32(&rec, 72), 64);
        assert_eq!(le32(&rec, 76), 0, "reserved");
        assert_eq!(le64(&rec, 80) as i64, -250);
        // Delta32 #1 at 88.
        assert_eq!(le32(&rec, 88), 6);
        assert_eq!(le64(&rec, 88 + 24) as i64, 300);
        // Destroy16 at 56 + 64 = 120.
        assert_eq!(le32(&rec, 120) as i32, -46);
        assert_eq!(le32(&rec, 124), 72);
        assert_eq!(le32(&rec, 128), 66);
        assert_eq!(le32(&rec, 132), 999);
    }

    #[test]
    fn journaled_golden_offsets() {
        let rec = enc_obs(Obs::ChunkLoaded {
            district: DistrictId(5),
            chunk: ChunkPos {
                dim: 1,
                x: -7,
                z: 12,
            },
            applied_epoch: 3,
        });
        assert_eq!(rec.len(), 48);
        assert_eq!(le16(&rec, 0), 0x0407);
        assert_eq!(le16(&rec, 2), 48);
        assert_eq!(le32(&rec, 4), 0);
        assert_eq!(le64(&rec, 8), SEQ, "obsSeq");
        assert_eq!(le64(&rec, 16), AT.0, "gameTime");
        assert_eq!(le64(&rec, 24), 5);
        assert_eq!(le32(&rec, 32), 1);
        assert_eq!(le32(&rec, 36) as i32, -7);
        assert_eq!(le32(&rec, 40), 12);
        assert_eq!(le32(&rec, 44), 3);

        // Result24's first field is the carriage index; Block24 and codes sit where §3.2 says.
        let rec = enc_obs(Obs::CarriageResult(CarriageResult {
            op: ENGINE_OP,
            train: TrainId(8),
            entries: vec![CarriageResultEntry {
                carriage: 6,
                good: Good(9),
                requested: -4,
                applied: -3,
            }],
        }));
        assert_eq!(rec.len(), 64);
        assert_eq!(le32(&rec, 4), 1);
        assert_eq!(le64(&rec, 24), ENGINE_OP.0);
        assert_eq!(le64(&rec, 32), 8);
        assert_eq!(le32(&rec, 40), 6);
        assert_eq!(le32(&rec, 44), 9);
        assert_eq!(le64(&rec, 48) as i64, -4);
        assert_eq!(le64(&rec, 56) as i64, -3);
    }

    #[test]
    fn enum_codes_and_flags_match_the_spec() {
        let sweep = |live, creative, overstressed| {
            le32(
                &enc_obs(Obs::SweepEnd(SweepEnd {
                    district: DistrictId(1),
                    sweep: 1,
                    records: 1,
                    inventories: 1,
                    live,
                    creative,
                    overstressed,
                })),
                4,
            )
        };
        assert_eq!(sweep(true, false, false), 1);
        assert_eq!(sweep(false, true, false), 2);
        assert_eq!(sweep(false, false, true), 4);
        assert_eq!(sweep(true, true, true), 7);

        for (i, p) in [
            DepositPluginId::OreExcavation,
            DepositPluginId::DieselOil,
            DepositPluginId::DestroyCrude,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(plugin_code(p), i as u32 + 1);
        }
        assert_eq!(
            [
                EditKind::Placed,
                EditKind::Broken,
                EditKind::Configured,
                EditKind::Command
            ]
            .map(edit_code),
            [1, 2, 3, 4]
        );
        assert_eq!(
            [
                DiscardReason::UnattributedSession,
                DiscardReason::CreativeBlock,
                DiscardReason::Overstressed,
                DiscardReason::ProgressiveContraption,
                DiscardReason::Other,
            ]
            .map(discard_code),
            [1, 2, 3, 4, 5]
        );
        assert_eq!(
            [LoadState::Unloaded, LoadState::Partial, LoadState::Live].map(load_code),
            [0, 1, 2]
        );
        assert_eq!(
            [
                PortKind::TrainCargo,
                PortKind::Package,
                PortKind::PlayerSession,
                PortKind::PlayerToss,
                PortKind::PlayerPickup,
                PortKind::StockBroken,
                PortKind::Reconcile,
                PortKind::Other,
            ]
            .map(port_code),
            [1, 2, 3, 4, 5, 6, 8, 9]
        );
        assert_eq!([Direction::In, Direction::Out].map(direction_code), [1, 2]);
        assert_eq!(ThreatClass::ALL.map(class_code), [0, 1, 2, 3]);
        assert_eq!(
            [CarriageReason::Exchange, CarriageReason::Loot].map(reason_code),
            [1, 2]
        );

        // Flag fields at their documented offsets.
        let dep = enc_obs(Obs::DepositSample(DepositSample {
            district: DistrictId(1),
            good: Good(2),
            plugin: DepositPluginId::DestroyCrude,
            chunk: ChunkPos {
                dim: 3,
                x: -4,
                z: 5,
            },
            remaining: 6,
            infinite: true,
            sweep: 7,
        }));
        assert_eq!(le32(&dep, 4), 3);
        assert_eq!(le32(&dep, 24), 2);
        assert_eq!(le32(&dep, 28), 7);
        assert_eq!(le64(&dep, 32), 6);
        assert_eq!(le32(&dep, 40), 3);
        assert_eq!(le32(&dep, 44) as i32, -4);
        assert_eq!(le32(&dep, 48), 5);
        assert_eq!(le32(&dep, 52), 1);

        let arrived = enc_obs(Obs::TrainArrived(TrainCall {
            train: TrainId(1),
            station: StationId(2),
            district: DistrictId(3),
            simulated: true,
            carriages: vec![],
        }));
        assert_eq!(arrived.len(), 56);
        assert_eq!(le32(&arrived, 48), 1);

        let survey = enc_obs(Obs::Survey(DefenceSurvey {
            district: DistrictId(1),
            revision: 2,
            mob_griefing: true,
            perimeter_columns: 3,
            perimeter_gaps: 4,
            dark_spawnable: 5,
            machine_blocks: 6,
            roofed_machines: 7,
            kill_machines_at_gaps: 8,
            turret_strength: [1.0, 2.0, 3.0, 4.0],
            guards: [10, 11, 12, 13],
            ammo_good: Some(Good(14)),
            ammo_per_engagement: 15,
        }));
        assert_eq!(le32(&survey, 4), 1);
        assert_eq!(le32(&survey, 60), 14);
        assert_eq!(f32::from_le_bytes(survey[64..68].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(survey[76..80].try_into().unwrap()), 4.0);
        assert_eq!(le32(&survey, 80), 10);
        assert_eq!(le32(&survey, 92), 13);
        assert_eq!(le32(&survey, 96), 15);
        assert_eq!(le32(&survey, 100), 0);

        let spawn = enc_obs(Obs::SpawnSample(SpawnSample {
            district: DistrictId(1),
            observed_ticks: 2,
            dark_blocks: 3,
            spawns: [4, 5, 6, 7],
        }));
        assert_eq!(le64(&spawn, 32), 2);
        assert_eq!(le64(&spawn, 40), 3);
        assert_eq!(le32(&spawn, 48), 4);
        assert_eq!(le32(&spawn, 60), 7);

        let fight = enc_obs(Obs::Fight(FightObserved {
            district: DistrictId(1),
            class: ThreatClass::Siege,
            attackers: 2,
            attacker_strength: 3.5,
            attackers_killed: 4,
            blocks_destroyed: 5,
            machines_destroyed: 6,
            guards_lost: 7,
            items_stolen: 8,
            duration_ticks: 9,
        }));
        assert_eq!(le32(&fight, 4), 3);
        assert_eq!(f32::from_le_bytes(fight[36..40].try_into().unwrap()), 3.5);
        assert_eq!(le64(&fight, 56), 8);
        assert_eq!(le64(&fight, 64), 9);

        // None encodes as 0 (or 0xFFFFFFFF for an inventory index); a ledger without a
        // breakpoint is -1.
        let port = enc_obs(Obs::Port(PortTransfer {
            district: DistrictId(1),
            kind: PortKind::Other,
            inventory: None,
            counterpart: 2,
            op: None,
            goods: GoodVec::new(),
        }));
        assert_eq!(le32(&port, 32), 9);
        assert_eq!(le32(&port, 36), 0xFFFF_FFFF);
        assert_eq!(le64(&port, 48), 0);
        let ledger = enc_cmd(&Cmd::Ledger(LedgerLine {
            district: DistrictId(1),
            state: 0,
            freeze: 0,
            next_breakpoint: None,
            next_kind: 0,
            next_good: None,
            since: Tick(0),
            goods: vec![],
        }));
        assert_eq!(le64(&ledger, 32) as i64, -1);
        assert_eq!(le32(&ledger, 44), 0);
    }

    #[test]
    fn unknown_kinds_are_skipped_not_errors() {
        let mut rec = vec![0u8; 16];
        for kind in [
            0x0001u16, 0x0043, 0x0047, 0x004E, 0x005F, 0x0060, 0x040C, 0x04FF, 0x0500, 0x8001,
            0x8402, 0x8407, 0x84FF,
        ] {
            rec[0..2].copy_from_slice(&kind.to_le_bytes());
            rec[2..4].copy_from_slice(&16u16.to_le_bytes());
            assert_eq!(decode_obs(&rec), Ok(None), "obs {kind:#06x}");
            assert_eq!(decode_cmd(&rec), Ok(None), "cmd {kind:#06x}");
        }
        // A real command is not an observation and the other way around.
        let c = enc_cmd(&all_cmds()[0]);
        assert_eq!(decode_obs(&c), Ok(None));
        let o = enc_obs(Obs::LedgerRequest {
            district: DistrictId(1),
        });
        assert_eq!(decode_cmd(&o), Ok(None));
    }

    #[test]
    fn decoding_rejects_malformed_records() {
        let stock = enc_obs(Obs::StockSample {
            district: DistrictId(1),
            inventory: 2,
            sweep: 3,
            goods: goods3(),
        });
        assert_eq!(stock.len(), 80);

        assert_eq!(decode_obs(&stock[..4]), Err(WireError::Truncated));
        assert_eq!(decode_obs(&stock[..72]), Err(WireError::Truncated));
        assert_eq!(decode_obs(&[]), Err(WireError::Truncated));

        // aux disagrees with size.
        let mut r = stock.clone();
        r[4] = 4;
        assert_eq!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: 0x0040,
                size: 80
            })
        );
        r[4] = 2;
        assert_eq!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: 0x0040,
                size: 80
            })
        );
        // A huge count does not wrap or allocate.
        r[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: 0x0040,
                size: 80
            })
        );

        // Size not a multiple of 8, or below the header.
        let mut r = stock.clone();
        r[2] = 12;
        assert_eq!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: 0x0040,
                size: 12
            })
        );
        r[2] = 0;
        assert_eq!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: 0x0040,
                size: 0
            })
        );

        // Bytes after the record are not part of it.
        let mut r = stock.clone();
        r.extend_from_slice(&[0xFF; 16]);
        assert_eq!(
            decode_obs(&r).unwrap().unwrap().1.obs,
            decode_obs(&stock).unwrap().unwrap().1.obs
        );

        // Unknown enum codes.
        let edit = enc_obs(Obs::Edit {
            district: DistrictId(1),
            kind: EditKind::Placed,
            player: (1, 2),
            pos: BlockPos::new(0, 0, 0),
        });
        for bad in [0u32, 5, 99] {
            let mut r = edit.clone();
            r[4..8].copy_from_slice(&bad.to_le_bytes());
            assert_eq!(
                decode_obs(&r),
                Err(WireError::BadCode {
                    kind: IND_EDIT,
                    field: "edit_kind",
                    value: u64::from(bad)
                })
            );
        }
        let fight = enc_obs(Obs::Fight(FightObserved {
            district: DistrictId(1),
            class: ThreatClass::Flying,
            attackers: 1,
            attacker_strength: 1.0,
            attackers_killed: 0,
            blocks_destroyed: 0,
            machines_destroyed: 0,
            guards_lost: 0,
            items_stolen: 0,
            duration_ticks: 0,
        }));
        let mut r = fight.clone();
        r[4] = 4;
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadCode {
                field: "threat_class",
                value: 4,
                ..
            })
        ));
        let port = enc_obs(Obs::Port(PortTransfer {
            district: DistrictId(1),
            kind: PortKind::Other,
            inventory: None,
            counterpart: 0,
            op: None,
            goods: GoodVec::new(),
        }));
        let mut r = port.clone();
        r[32] = 10;
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadCode {
                field: "port_kind",
                value: 10,
                ..
            })
        ));
        let mut r = port.clone();
        r[32] = 0;
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadCode {
                field: "port_kind",
                value: 0,
                ..
            })
        ));
        let dep = enc_cmd(&Cmd::DepositDelta(DepositDelta {
            op: ENGINE_OP,
            plugin: DepositPluginId::DieselOil,
            chunk: ChunkPos::default(),
            good: Good(1),
            delta: 1,
        }));
        let mut r = dep.clone();
        r[4] = 4;
        assert!(matches!(
            decode_cmd(&r),
            Err(WireError::BadCode {
                field: "plugin",
                value: 4,
                ..
            })
        ));

        // A chunk reconcile whose destroyCount disagrees with its size.
        let rc = enc_cmd(&all_cmds()[0]);
        let mut r = rc.clone();
        r[48] += 1;
        assert!(matches!(
            decode_cmd(&r),
            Err(WireError::BadSize {
                kind: IND_CHUNK_RECONCILE,
                ..
            })
        ));
        assert_eq!(decode_cmd(&rc[..rc.len() - 8]), Err(WireError::Truncated));

        // A carriage count that runs past the end, and one that leaves bytes over.
        let arrived = enc_obs(Obs::TrainArrived(TrainCall {
            train: TrainId(1),
            station: StationId(2),
            district: DistrictId(3),
            simulated: false,
            carriages: carriages(),
        }));
        let mut r = arrived.clone();
        r[4] = 4;
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: IND_TRAIN_ARRIVED,
                ..
            })
        ));
        r[4] = 2;
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: IND_TRAIN_ARRIVED,
                ..
            })
        ));
        let mut r = arrived.clone();
        r[56 + 16..56 + 20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            decode_obs(&r),
            Err(WireError::BadSize {
                kind: IND_TRAIN_ARRIVED,
                ..
            })
        ));

        // A ledger breakpoint below -1 is not a tick.
        let ledger = enc_cmd(&all_cmds()[5]);
        let mut r = ledger.clone();
        r[32..40].copy_from_slice(&(-2i64).to_le_bytes());
        assert!(matches!(
            decode_cmd(&r),
            Err(WireError::BadCode {
                field: "next_breakpoint",
                ..
            })
        ));
    }

    #[test]
    fn oversize_records_are_refused_and_leave_the_buffer_alone() {
        let big = |n: u32| Timed {
            at: AT,
            obs: Obs::StockSample {
                district: DistrictId(1),
                inventory: 1,
                sweep: 1,
                goods: (1..=n).map(|g| (Good(g), 1)).collect(),
            },
        };
        let mut out = vec![1, 2, 3];
        assert_eq!(
            encode_obs(SEQ, &big(5_000), &mut out),
            Err(WireError::TooBig {
                kind: IND_STOCK_SAMPLE,
                size: 32 + 16 * 5_000
            })
        );
        assert_eq!(out, [1, 2, 3]);

        // 4093 entries make 65,520 bytes; 4094 make 65,536.
        let mut out = Vec::new();
        encode_obs(SEQ, &big(4_093), &mut out).unwrap();
        assert_eq!(out.len(), 32 + 16 * 4_093);
        assert_eq!(decode_obs(&out).unwrap().unwrap().1, big(4_093));
        let mut out = Vec::new();
        assert_eq!(
            encode_obs(SEQ, &big(4_094), &mut out),
            Err(WireError::TooBig {
                kind: IND_STOCK_SAMPLE,
                size: 65_536
            })
        );
        assert!(out.is_empty());

        // A record of exactly MAX_RECORD bytes is allowed (40 + 8 * 8186).
        let chunks = |n: i32| Timed {
            at: AT,
            obs: Obs::DistrictChunks {
                district: DistrictId(1),
                revision: 1,
                part: 0,
                chunks: (0..n).map(|i| (i, -i)).collect(),
            },
        };
        let mut out = Vec::new();
        encode_obs(SEQ, &chunks(8_186), &mut out).unwrap();
        assert_eq!(out.len(), MAX_RECORD);
        assert_eq!(usize::from(le16(&out, 2)), MAX_RECORD);
        assert_eq!(decode_obs(&out).unwrap().unwrap().1, chunks(8_186));
        let mut out = Vec::new();
        assert_eq!(
            encode_obs(SEQ, &chunks(8_187), &mut out),
            Err(WireError::TooBig {
                kind: IND_DISTRICT_CHUNKS,
                size: MAX_RECORD + 8
            })
        );

        // Commands too.
        let cmd = Cmd::WritePackage(WritePackage {
            op: ENGINE_OP,
            district: DistrictId(1),
            inventory: 0,
            pos: BlockPos::new(0, 0, 0),
            address: 1,
            count: 1,
            goods: (1..=5_000).map(|g| (Good(g), 1)).collect(),
        });
        let mut out = vec![9];
        assert!(matches!(
            encode_cmd(SEQ, &cmd, &mut out),
            Err(WireError::TooBig {
                kind: IND_WRITE_PACKAGE,
                ..
            })
        ));
        assert_eq!(out, [9]);
    }

    #[test]
    fn ledger_breakpoint_must_fit_a_positive_i64() {
        let line = |bp| {
            Cmd::Ledger(LedgerLine {
                district: DistrictId(1),
                state: 0,
                freeze: 0,
                next_breakpoint: bp,
                next_kind: 0,
                next_good: None,
                since: Tick(u64::MAX),
                goods: vec![],
            })
        };
        let mut out = Vec::new();
        assert!(matches!(
            encode_cmd(SEQ, &line(Some(Tick(u64::MAX))), &mut out),
            Err(WireError::BadCode {
                field: "next_breakpoint",
                ..
            })
        ));
        assert!(out.is_empty());
        for bp in [None, Some(Tick(0)), Some(Tick(i64::MAX as u64))] {
            let c = line(bp);
            assert_eq!(decode_cmd(&enc_cmd(&c)).unwrap().unwrap().1, c);
        }
    }

    #[test]
    fn records_walks_mixed_buffers_and_stops_at_corruption() {
        let obs = all_obs();
        let cmds = all_cmds();
        let mut buf = Vec::new();
        let mut want_kinds = Vec::new();
        for (i, o) in obs.iter().take(12).enumerate() {
            encode_obs(
                i as u64,
                &Timed {
                    at: Tick(i as u64),
                    obs: o.clone(),
                },
                &mut buf,
            )
            .unwrap();
            want_kinds.push(spec_obs_kind(o));
        }
        for c in cmds.iter().take(4) {
            encode_cmd(1, c, &mut buf).unwrap();
            want_kinds.push(spec_cmd_kind(c));
        }
        // A kind outside the industry ranges is walked like any other.
        let foreign_at = buf.len();
        buf.extend_from_slice(&[0x02, 0x00, 0x10, 0x00, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        want_kinds.push(2);

        let walked: Vec<_> = Records::new(&buf).collect::<Result<_, _>>().unwrap();
        assert_eq!(walked.iter().map(|w| w.0).collect::<Vec<_>>(), want_kinds);
        assert_eq!(walked.iter().map(|w| w.1.len()).sum::<usize>(), buf.len());
        assert_eq!(
            walked.last().unwrap().1.as_ptr(),
            buf[foreign_at..].as_ptr()
        );
        for (i, (kind, rec)) in walked.iter().take(12).enumerate() {
            let (seq, t) = decode_obs(rec).unwrap().unwrap();
            assert_eq!(t.obs, obs[i]);
            assert_eq!(t.at, Tick(i as u64));
            assert_eq!(seq, if *kind >= 0x0400 { i as u64 } else { 0 });
        }
        for (i, (_, rec)) in walked[12..16].iter().enumerate() {
            assert_eq!(decode_cmd(rec).unwrap().unwrap().1, cmds[i]);
        }
        assert_eq!(Records::new(&[]).count(), 0);

        // Corrupt the third record's size: the first two are yielded, then one error, then nothing.
        let third = walked[0].1.len() + walked[1].1.len();
        for (bad, why) in [
            (12u16, "not a multiple of 8"),
            (0, "below the header"),
            (4, "below the header"),
            (u16::MAX - 7, "past the end"),
        ] {
            let mut corrupt = buf.clone();
            corrupt[third + 2..third + 4].copy_from_slice(&bad.to_le_bytes());
            let mut it = Records::new(&corrupt);
            assert!(it.next().unwrap().is_ok());
            assert!(it.next().unwrap().is_ok());
            assert!(
                matches!(it.next(), Some(Err(WireError::BadSize { size, .. })) if size == usize::from(bad)),
                "{why}"
            );
            assert!(it.next().is_none(), "{why}");
        }

        // A tail shorter than a header.
        let mut tail = buf[..walked[0].1.len()].to_vec();
        tail.extend_from_slice(&[0; 4]);
        let mut it = Records::new(&tail);
        assert!(it.next().unwrap().is_ok());
        assert_eq!(it.next(), Some(Err(WireError::Truncated)));
        assert!(it.next().is_none());
    }

    fn arb_goods() -> impl Strategy<Value = GoodVec> {
        // Amounts stay far from i64 limits so merging duplicate goods cannot overflow.
        let amount = prop_oneof![1i64..(1 << 40), -(1i64 << 40)..0];
        prop::collection::vec((1u32..500, amount), 0..120)
            .prop_map(|v| v.into_iter().map(|(g, a)| (Good(g), a)).collect())
    }

    fn arb_pos() -> impl Strategy<Value = BlockPos> {
        (any::<i32>(), any::<i32>(), any::<i32>()).prop_map(|(x, y, z)| BlockPos::new(x, y, z))
    }

    proptest! {
        #[test]
        fn stock_sample_round_trips(
            seq in any::<u64>(),
            at in any::<u64>(),
            district in any::<u64>(),
            inventory in any::<u32>(),
            sweep in any::<u32>(),
            goods in arb_goods(),
        ) {
            let t = Timed {
                at: Tick(at),
                obs: Obs::StockSample { district: DistrictId(district), inventory, sweep, goods: goods.clone() },
            };
            let mut out = Vec::new();
            encode_obs(seq, &t, &mut out).unwrap();
            prop_assert_eq!(out.len(), 32 + 16 * goods.len());
            prop_assert_eq!(le32(&out, 4) as usize, goods.len());
            let (got_seq, back) = decode_obs(&out).unwrap().unwrap();
            prop_assert_eq!(got_seq, 0);
            prop_assert_eq!(back, t);
        }

        #[test]
        fn chunk_reconcile_round_trips(
            seq in any::<u64>(),
            op in any::<u64>(),
            district in any::<u64>(),
            epoch in any::<u32>(),
            (dim, cx, cz) in (any::<u32>(), any::<i32>(), any::<i32>()),
            deltas in prop::collection::vec(
                (any::<u32>(), 1u32..500, arb_pos(), any::<i64>()),
                0..80,
            ),
            destroy in prop::collection::vec((arb_pos(), any::<u32>()), 0..80),
        ) {
            let cmd = Cmd::ChunkReconcile(ChunkReconcile {
                op: OpId(op),
                district: DistrictId(district),
                epoch,
                chunk: ChunkPos { dim, x: cx, z: cz },
                deltas: deltas
                    .into_iter()
                    .map(|(inventory, g, pos, delta)| InvDelta { inventory, pos, good: Good(g), delta })
                    .collect(),
                destroy: destroy
                    .into_iter()
                    .map(|(pos, expected_state)| BlockDestroy { pos, expected_state })
                    .collect(),
            });
            let mut out = Vec::new();
            encode_cmd(seq, &cmd, &mut out).unwrap();
            prop_assert_eq!(out.len(), spec_cmd_size(&cmd));
            let (got_seq, back) = decode_cmd(&out).unwrap().unwrap();
            prop_assert_eq!(got_seq, seq);
            prop_assert_eq!(back, cmd);
        }

        #[test]
        fn train_arrived_round_trips(
            carriages in prop::collection::vec(
                (any::<u32>(), any::<u32>(), any::<i64>(), arb_goods()),
                0..12,
            ),
            simulated in any::<bool>(),
        ) {
            let carriages = carriages
                .into_iter()
                .map(|(index, item_slots, fluid_capacity, goods)| Carriage { index, item_slots, fluid_capacity, goods })
                .collect();
            let t = Timed {
                at: AT,
                obs: Obs::TrainArrived(TrainCall {
                    train: TrainId(1),
                    station: StationId(2),
                    district: DistrictId(3),
                    simulated,
                    carriages,
                }),
            };
            let mut out = Vec::new();
            encode_obs(SEQ, &t, &mut out).unwrap();
            prop_assert_eq!(out.len(), spec_obs_size(&t.obs));
            let (seq, back) = decode_obs(&out).unwrap().unwrap();
            prop_assert_eq!(seq, SEQ);
            prop_assert_eq!(back, t);
        }

        /// Whatever the bytes, decoding returns a value or an error and never panics.
        #[test]
        fn decoding_arbitrary_bodies_never_panics(
            kind_ix in 0usize..31,
            aux in prop_oneof![0u32..40, any::<u32>()],
            body in prop::collection::vec(any::<u8>(), 0..400),
        ) {
            let kinds: Vec<u16> = OBS_KINDS.iter().chain(CMD_KINDS.iter()).copied().collect();
            let mut rec = Vec::new();
            rec.extend_from_slice(&kinds[kind_ix].to_le_bytes());
            rec.extend_from_slice(&[0, 0]);
            rec.extend_from_slice(&aux.to_le_bytes());
            rec.extend_from_slice(&body);
            rec.resize(rec.len().next_multiple_of(8), 0);
            let size = u16::try_from(rec.len()).unwrap();
            rec[2..4].copy_from_slice(&size.to_le_bytes());
            let _ = decode_obs(&rec);
            let _ = decode_cmd(&rec);
            let _ = Records::new(&rec).count();
        }
    }
}
