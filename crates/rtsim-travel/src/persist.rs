// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving flows, caravans, legs and news (the phase 1 world's snapshot,
//! docs/overnight-choices-persist.md). Every field is written, private ones included, so what is
//! read back continues exactly as what was saved.
//!
//! News snapshots are shared (`Arc`): [`SnapEnc`] writes each one once, where it is first met,
//! and refers to it by number after that; [`SnapDec`] gives every reference the same `Arc` back.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_knowledge::board::BoardCursor;
use rtsim_settle::Culture;
use rtsim_trade::line::Level;
use rtsim_trade::persist::{get_opt_good, put_opt_good};
use rtsim_trade::{Amount, GoodId};

use crate::caravan::{Body, Caravan, CaravanKey, GroupKind, Leg, Member, Stage};
use crate::edge::{Dir, DirRef, Edge, Medium, Predation};
use crate::lattice::{Lattice, Period};
use crate::legs::{LegIndex, LegRow, LegStats, Owner, Path, Stamp};
use crate::news::{GoodNews, NewsRing, Snapshot};
use crate::pipeline::{Hole, Pipeline, Seg};
use crate::roads::RoadIndex;
use crate::vecmap::VecMap;

fn bad<T>(what: String) -> codec::Result<T> {
    Err(CodecError::Invalid(what))
}

/// A sorted map of goods to amounts.
pub fn put_goods(w: &mut Writer, m: &BTreeMap<GoodId, Amount>) {
    w.var_u64(m.len() as u64);
    for (g, a) in m {
        w.u16(g.0);
        w.var_i64(*a);
    }
}

pub fn get_goods(r: &mut Reader<'_>) -> codec::Result<BTreeMap<GoodId, Amount>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(3)? {
        m.insert(GoodId(r.u16()?), r.var_i64()?);
    }
    Ok(m)
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

// ---- pipelines, lattices, edges --------------------------------------------------------------------

impl Encode for Pipeline {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.delay);
        w.var_u64(self.segs.len() as u64);
        for s in &self.segs {
            w.var_u64(s.start);
            w.var_i64(s.rate);
            w.var_i64(s.quote);
        }
        w.var_u64(self.holes.len() as u64);
        for h in &self.holes {
            w.var_u64(h.from);
            w.var_u64(h.to);
        }
    }
}

impl Decode for Pipeline {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let delay = r.var_u64()?;
        if delay == 0 {
            return bad("a pipeline of delay 0".into());
        }
        let n = r.len_prefix(3)?;
        let mut segs = VecDeque::with_capacity(n);
        for _ in 0..n {
            segs.push_back(Seg {
                start: r.var_u64()?,
                rate: r.var_i64()?,
                quote: r.var_i64()?,
            });
        }
        let n = r.len_prefix(2)?;
        let mut holes = Vec::with_capacity(n);
        for _ in 0..n {
            holes.push(Hole {
                from: r.var_u64()?,
                to: r.var_u64()?,
            });
        }
        Ok(Pipeline { delay, segs, holes })
    }
}

impl Encode for Lattice {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.segs.len() as u64);
        for p in &self.segs {
            w.var_u64(p.start);
            w.var_u64(p.period);
        }
    }
}

impl Decode for Lattice {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let n = r.len_prefix(2)?;
        if n == 0 {
            return bad("an empty lattice".into());
        }
        let mut segs = VecDeque::with_capacity(n);
        for _ in 0..n {
            let start = r.var_u64()?;
            let period = r.var_u64()?;
            if period == 0 {
                return bad("a lattice period of 0".into());
            }
            segs.push_back(Period { start, period });
        }
        Ok(Lattice { segs })
    }
}

fn put_pipes(w: &mut Writer, m: &VecMap<GoodId, Pipeline>) {
    w.var_u64(m.items.len() as u64);
    for (g, p) in &m.items {
        w.u16(g.0);
        p.encode(w);
    }
}

fn get_pipes(r: &mut Reader<'_>) -> codec::Result<VecMap<GoodId, Pipeline>> {
    let n = r.len_prefix(4)?;
    let mut items: Vec<(GoodId, Pipeline)> = Vec::with_capacity(n);
    for _ in 0..n {
        let g = GoodId(r.u16()?);
        if items.last().is_some_and(|(last, _)| *last >= g) {
            return bad("pipelines out of order".into());
        }
        items.push((g, Pipeline::decode(r)?));
    }
    Ok(VecMap { items })
}

impl Encode for Dir {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.from as u64);
        w.var_u64(self.to as u64);
        put_pipes(w, &self.goods);
        put_pipes(w, &self.pay);
        put_pipes(w, &self.raid);
        match &self.predation {
            Some(p) => {
                w.u8(1);
                w.var_u64(p.haven as u64);
                w.var_i64(p.share_milli);
                w.var_u64(p.delay);
            }
            None => w.u8(0),
        }
        self.lattice.encode(w);
        w.var_u64(self.board_cursor.0);
        w.u64(self.relayed_through.0);
        match self.pending {
            Some((a, b)) => {
                w.u8(1);
                w.var_u64(a);
                w.u32(b);
            }
            None => w.u8(0),
        }
    }
}

fn get_usize(r: &mut Reader<'_>) -> codec::Result<usize> {
    let v = r.var_u64()?;
    usize::try_from(v).map_err(|_| CodecError::Invalid(format!("index {v}")))
}

impl Decode for Dir {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let from = get_usize(r)?;
        let to = get_usize(r)?;
        let goods = get_pipes(r)?;
        let pay = get_pipes(r)?;
        let raid = get_pipes(r)?;
        let predation = match r.u8()? {
            0 => None,
            1 => Some(Predation {
                haven: get_usize(r)?,
                share_milli: r.var_i64()?,
                delay: r.var_u64()?,
            }),
            t => return bad(format!("predation tag {t}")),
        };
        let lattice = Lattice::decode(r)?;
        let board_cursor = BoardCursor(r.var_u64()?);
        let relayed_through = Tick(r.u64()?);
        let pending = match r.u8()? {
            0 => None,
            1 => Some((r.var_u64()?, r.u32()?)),
            t => return bad(format!("pending tag {t}")),
        };
        Ok(Dir {
            from,
            to,
            goods,
            pay,
            raid,
            predation,
            lattice,
            board_cursor,
            relayed_through,
            pending,
        })
    }
}

impl Encode for Edge {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.a as u64);
        w.var_u64(self.b as u64);
        w.var_u64(self.length_blocks);
        w.var_u64(self.d);
        w.var_u64(self.dock);
        w.u64(self.seed);
        w.bool(self.border);
        self.dirs[0].encode(w);
        self.dirs[1].encode(w);
        w.var_u64(self.stops.len() as u64);
        self.stops.iter().for_each(|s| w.var_u64(*s as u64));
        w.u8(self.medium as u8);
        w.var_i64(self.cap_slot_milli);
    }
}

impl Decode for Edge {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let a = get_usize(r)?;
        let b = get_usize(r)?;
        let length_blocks = r.var_u64()?;
        let d = r.var_u64()?;
        let dock = r.var_u64()?;
        let seed = r.u64()?;
        let border = r.bool()?;
        let dirs = [Dir::decode(r)?, Dir::decode(r)?];
        let n = r.len_prefix(1)?;
        let stops = (0..n).map(|_| get_usize(r)).collect::<codec::Result<_>>()?;
        let m = r.u8()?;
        let medium =
            Medium::from_u8(m).ok_or_else(|| CodecError::Invalid(format!("medium {m}")))?;
        let cap_slot_milli = r.var_i64()?;
        Ok(Edge {
            a,
            b,
            length_blocks,
            d,
            dock,
            seed,
            border,
            dirs,
            stops,
            medium,
            cap_slot_milli,
        })
    }
}

impl Encode for DirRef {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(u64::from(self.edge));
        w.u8(self.dir);
    }
}

impl Decode for DirRef {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let edge = u32::try_from(r.var_u64()?)
            .map_err(|_| CodecError::Invalid("an edge index over u32".into()))?;
        let dir = r.u8()?;
        if dir > 1 {
            return bad(format!("direction {dir}"));
        }
        Ok(DirRef { edge, dir })
    }
}

// ---- caravans ------------------------------------------------------------------------------------

impl Encode for CaravanKey {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(u64::from(self.edge));
        w.u8(self.dir);
        w.var_u64(self.departure);
    }
}

impl Decode for CaravanKey {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(CaravanKey {
            edge: u32::try_from(r.var_u64()?)
                .map_err(|_| CodecError::Invalid("an edge index over u32".into()))?,
            dir: r.u8()?,
            departure: r.var_u64()?,
        })
    }
}

impl Encode for Body {
    fn encode(&self, w: &mut Writer) {
        w.u8(match self {
            Body::Trader => 0,
            Body::Llama => 1,
            Body::RibbitMerchant => 2,
            Body::RibbitMaster => 3,
            Body::RibbitMusician => 4,
            Body::RibbitResident => 5,
        });
    }
}

impl Decode for Body {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(match r.u8()? {
            0 => Body::Trader,
            1 => Body::Llama,
            2 => Body::RibbitMerchant,
            3 => Body::RibbitMaster,
            4 => Body::RibbitMusician,
            5 => Body::RibbitResident,
            t => return bad(format!("body {t}")),
        })
    }
}

impl Encode for GroupKind {
    fn encode(&self, w: &mut Writer) {
        w.u8(match self {
            GroupKind::HumanCaravan => 0,
            GroupKind::RibbitCaravan => 1,
        });
    }
}

impl Decode for GroupKind {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(match r.u8()? {
            0 => GroupKind::HumanCaravan,
            1 => GroupKind::RibbitCaravan,
            t => return bad(format!("group kind {t}")),
        })
    }
}

impl Encode for Caravan {
    fn encode(&self, w: &mut Writer) {
        self.key.encode(w);
        w.u64(self.seed);
        w.var_u64(self.home as u64);
        self.group.encode(w);
        w.var_u64(self.members.len() as u64);
        for m in &self.members {
            m.body.encode(w);
            w.bool(m.alive);
            w.bool(m.present);
            put_goods(w, &m.cargo);
        }
        w.u8(match self.stage {
            Stage::Outbound => 0,
            Stage::Returning => 1,
        });
        let l = &self.leg;
        w.var_u64(l.from as u64);
        w.var_u64(l.to as u64);
        w.u64(l.start.0);
        w.u64(l.end.0);
        w.u32(l.generation);
        w.u64(self.holder.to_bits());
        w.u64(self.notable.map_or(0, |n| n.to_bits()));
        put_goods(w, &self.quotes);
        put_goods(w, &self.purse);
    }
}

impl Decode for Caravan {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let key = CaravanKey::decode(r)?;
        let seed = r.u64()?;
        let home = get_usize(r)?;
        let group = GroupKind::decode(r)?;
        let n = r.len_prefix(4)?;
        let mut members = Vec::with_capacity(n);
        for _ in 0..n {
            members.push(Member {
                body: Body::decode(r)?,
                alive: r.bool()?,
                present: r.bool()?,
                cargo: get_goods(r)?,
            });
        }
        let stage = match r.u8()? {
            0 => Stage::Outbound,
            1 => Stage::Returning,
            t => return bad(format!("stage {t}")),
        };
        let leg = Leg {
            from: get_usize(r)?,
            to: get_usize(r)?,
            start: Tick(r.u64()?),
            end: Tick(r.u64()?),
            generation: r.u32()?,
        };
        let holder = Handle::from_bits(r.u64()?);
        let notable = Handle::from_bits(r.u64()?);
        Ok(Caravan {
            key,
            seed,
            home,
            group,
            members,
            stage,
            leg,
            holder,
            notable: (!notable.is_null()).then_some(notable),
            quotes: get_goods(r)?,
            purse: get_goods(r)?,
        })
    }
}

// ---- the leg index and the road index -------------------------------------------------------------

impl Encode for Path {
    fn encode(&self, w: &mut Writer) {
        w.u16(self.frame);
        for v in [self.x0, self.z0, self.x1, self.z1] {
            w.var_i64(i64::from(v));
        }
        w.u64(self.t0.0);
        w.u64(self.t1.0);
        w.var_u64(u64::from(self.radius));
    }
}

impl Decode for Path {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Path {
            frame: r.u16()?,
            x0: i32_of(r.var_i64()?)?,
            z0: i32_of(r.var_i64()?)?,
            x1: i32_of(r.var_i64()?)?,
            z1: i32_of(r.var_i64()?)?,
            t0: Tick(r.u64()?),
            t1: Tick(r.u64()?),
            radius: u32::try_from(r.var_u64()?)
                .map_err(|_| CodecError::Invalid("a radius over u32".into()))?,
        })
    }
}

impl Encode for Stamp {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.leg.to_bits());
        w.u32(self.generation);
    }
}

impl Decode for Stamp {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Stamp {
            leg: Handle::from_bits(r.u64()?),
            generation: r.u32()?,
        })
    }
}

impl Encode for LegIndex {
    fn encode(&self, w: &mut Writer) {
        self.legs.encode_with(w, |row, w| {
            row.path.encode(w);
            w.u64(row.owner.0);
            w.u32(row.generation);
        });
        w.var_u64(self.cells.len() as u64);
        for (&(win, frame, x, z), v) in &self.cells {
            w.var_u64(win);
            w.u16(frame);
            w.var_i64(x);
            w.var_i64(z);
            w.var_u64(v.len() as u64);
            v.iter().for_each(|s| s.encode(w));
        }
        w.var_u64(self.window);
        w.var_u64(self.stats.booked_cells);
        w.var_u64(self.stats.stale_dropped);
    }
}

impl Decode for LegIndex {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let legs = Arena::decode_with(r, |r| {
            Ok(LegRow {
                path: Path::decode(r)?,
                owner: Owner(r.u64()?),
                generation: r.u32()?,
            })
        })?;
        let mut cells = BTreeMap::new();
        for _ in 0..r.len_prefix(5)? {
            let cell = (r.var_u64()?, r.u16()?, r.var_i64()?, r.var_i64()?);
            let n = r.len_prefix(12)?;
            let v = (0..n)
                .map(|_| Stamp::decode(r))
                .collect::<codec::Result<Vec<_>>>()?;
            cells.insert(cell, v);
        }
        let window = r.var_u64()?.max(1);
        let stats = LegStats {
            booked_cells: r.var_u64()?,
            stale_dropped: r.var_u64()?,
        };
        Ok(LegIndex {
            legs,
            cells,
            window,
            stats,
            // Session state, handed over again by the engine host (topology choice TP6).
            topo: rtsim_space::topology::all_flat(),
        })
    }
}

impl Encode for RoadIndex {
    fn encode(&self, w: &mut Writer) {
        w.var_u64(self.cells.len() as u64);
        for (&(frame, x, z), v) in &self.cells {
            w.u16(frame);
            w.var_i64(x);
            w.var_i64(z);
            w.var_u64(v.len() as u64);
            v.iter().for_each(|e| w.var_u64(u64::from(*e)));
        }
    }
}

impl Decode for RoadIndex {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let mut cells = BTreeMap::new();
        for _ in 0..r.len_prefix(4)? {
            let cell = (r.u16()?, r.var_i64()?, r.var_i64()?);
            let n = r.len_prefix(1)?;
            let v = (0..n)
                .map(|_| {
                    u32::try_from(r.var_u64()?)
                        .map_err(|_| CodecError::Invalid("an edge index over u32".into()))
                })
                .collect::<codec::Result<Vec<_>>>()?;
            cells.insert(cell, v);
        }
        Ok(RoadIndex { cells })
    }
}

// ---- news ----------------------------------------------------------------------------------------

fn put_level(w: &mut Writer, l: Level) {
    w.u8(match l {
        Level::Empty => 0,
        Level::Below => 1,
        Level::At => 2,
        Level::Above => 3,
        Level::Glut => 4,
    });
}

fn get_level(r: &mut Reader<'_>) -> codec::Result<Level> {
    Ok(match r.u8()? {
        0 => Level::Empty,
        1 => Level::Below,
        2 => Level::At,
        3 => Level::Above,
        4 => Level::Glut,
        t => return bad(format!("level {t}")),
    })
}

fn put_snapshot(w: &mut Writer, s: &Snapshot) {
    w.var_u64(s.market as u64);
    w.u64(s.key);
    w.u64(s.at.0);
    w.u8(match s.culture {
        None => 0,
        Some(Culture::Human) => 1,
        Some(Culture::Ribbit) => 2,
    });
    put_opt_good(w, s.currency);
    w.bool(s.is_base);
    w.bool(s.can_pay);
    w.bool(s.refuses_cash);
    w.var_u64(s.goods.len() as u64);
    for g in &s.goods {
        w.var_i64(g.price);
        w.var_i64(g.anchor);
        w.var_i64(g.stock);
        w.var_i64(g.target);
        w.var_i64(g.absorbable);
        put_level(w, g.level);
        w.u16(g.suppliers);
        w.bool(g.currency);
    }
    w.var_u64(s.inflows.len() as u64);
    for (a, g, amt) in &s.inflows {
        w.var_u64(u64::from(*a));
        w.u16(g.0);
        w.var_i64(*amt);
    }
}

fn get_snapshot(r: &mut Reader<'_>) -> codec::Result<Snapshot> {
    let market = get_usize(r)?;
    let key = r.u64()?;
    let at = Tick(r.u64()?);
    let culture = match r.u8()? {
        0 => None,
        1 => Some(Culture::Human),
        2 => Some(Culture::Ribbit),
        t => return bad(format!("culture tag {t}")),
    };
    let currency = get_opt_good(r)?;
    let is_base = r.bool()?;
    let can_pay = r.bool()?;
    let refuses_cash = r.bool()?;
    let n = r.len_prefix(10)?;
    let mut goods = Vec::with_capacity(n);
    for _ in 0..n {
        goods.push(GoodNews {
            price: r.var_i64()?,
            anchor: r.var_i64()?,
            stock: r.var_i64()?,
            target: r.var_i64()?,
            absorbable: r.var_i64()?,
            level: get_level(r)?,
            suppliers: r.u16()?,
            currency: r.bool()?,
        });
    }
    let n = r.len_prefix(4)?;
    let mut inflows = Vec::with_capacity(n);
    for _ in 0..n {
        let a = u32::try_from(r.var_u64()?)
            .map_err(|_| CodecError::Invalid("an inflow source over u32".into()))?;
        inflows.push((a, GoodId(r.u16()?), r.var_i64()?));
    }
    Ok(Snapshot {
        market,
        key,
        at,
        culture,
        currency,
        is_base,
        can_pay,
        refuses_cash,
        goods,
        inflows,
    })
}

/// Writes shared news snapshots once each: the first reference carries the snapshot, later ones
/// its number.
#[derive(Default)]
pub struct SnapEnc {
    ids: HashMap<*const Snapshot, u32>,
}

impl SnapEnc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, w: &mut Writer, s: &Arc<Snapshot>) {
        let p = Arc::as_ptr(s);
        match self.ids.get(&p) {
            Some(&id) => {
                w.u8(0);
                w.var_u64(u64::from(id));
            }
            None => {
                let id = self.ids.len() as u32;
                self.ids.insert(p, id);
                w.u8(1);
                put_snapshot(w, s);
            }
        }
    }

    pub fn put_opt(&mut self, w: &mut Writer, s: &Option<Arc<Snapshot>>) {
        match s {
            Some(s) => self.put(w, s),
            None => w.u8(2),
        }
    }
}

/// Reads what [`SnapEnc`] wrote, sharing one `Arc` per snapshot again.
#[derive(Default)]
pub struct SnapDec {
    seen: Vec<Arc<Snapshot>>,
}

impl SnapDec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_opt(&mut self, r: &mut Reader<'_>) -> codec::Result<Option<Arc<Snapshot>>> {
        match r.u8()? {
            0 => {
                let id = r.var_u64()?;
                match self.seen.get(id as usize) {
                    Some(s) => Ok(Some(Arc::clone(s))),
                    None => bad(format!("news snapshot {id} before it was written")),
                }
            }
            1 => {
                let s = Arc::new(get_snapshot(r)?);
                self.seen.push(Arc::clone(&s));
                Ok(Some(s))
            }
            2 => Ok(None),
            t => bad(format!("snapshot tag {t}")),
        }
    }

    pub fn get(&mut self, r: &mut Reader<'_>) -> codec::Result<Arc<Snapshot>> {
        self.get_opt(r)?
            .ok_or_else(|| CodecError::Invalid("a snapshot is missing".into()))
    }
}

impl NewsRing {
    pub fn save(&self, w: &mut Writer, snaps: &mut SnapEnc) {
        w.var_u64(self.cap as u64);
        w.var_u64(self.entries.len() as u64);
        for (src, (s, at)) in &self.entries {
            w.var_u64(*src as u64);
            snaps.put(w, s);
            w.u64(at.0);
        }
    }

    pub fn load(r: &mut Reader<'_>, snaps: &mut SnapDec) -> codec::Result<Self> {
        let cap = get_usize(r)?.max(1);
        let mut entries = BTreeMap::new();
        for _ in 0..r.len_prefix(10)? {
            let src = get_usize(r)?;
            let s = snaps.get(r)?;
            entries.insert(src, (s, Tick(r.u64()?)));
        }
        Ok(NewsRing { cap, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(market: usize, at: u64) -> Snapshot {
        Snapshot {
            market,
            key: 1000 + market as u64,
            at: Tick(at),
            culture: Some(Culture::Ribbit),
            currency: Some(GoodId(7)),
            is_base: false,
            can_pay: true,
            refuses_cash: false,
            goods: vec![GoodNews {
                price: 1_500_000,
                anchor: 1_000_000,
                stock: 40_000,
                target: 30_000,
                absorbable: -5,
                level: Level::Above,
                suppliers: 2,
                currency: false,
            }],
            inflows: vec![(3, GoodId(1), 250)],
        }
    }

    #[test]
    fn shared_snapshots_are_written_once_and_shared_again() {
        let a = Arc::new(snap(1, 10));
        let b = Arc::new(snap(2, 20));
        let mut ring = NewsRing::new(4);
        ring.entries.insert(1, (Arc::clone(&a), Tick(11)));
        ring.entries.insert(2, (Arc::clone(&b), Tick(21)));
        let mut w = Writer::new();
        let mut enc = SnapEnc::new();
        ring.save(&mut w, &mut enc);
        enc.put_opt(&mut w, &Some(Arc::clone(&a)));
        enc.put_opt(&mut w, &None);
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        let mut dec = SnapDec::new();
        let back = NewsRing::load(&mut r, &mut dec).unwrap();
        let again = dec.get_opt(&mut r).unwrap().unwrap();
        assert!(dec.get_opt(&mut r).unwrap().is_none());
        r.finish().unwrap();
        assert_eq!(back.cap, 4);
        assert_eq!(*back.entries[&1].0, *a);
        assert_eq!(*back.entries[&2].0, *b);
        assert!(Arc::ptr_eq(&back.entries[&1].0, &again));
    }

    #[test]
    fn pipelines_lattices_and_caravans_read_back_equal() {
        let mut p = Pipeline::new(3);
        p.segs.push_back(Seg {
            start: 5,
            rate: 7,
            quote: 9,
        });
        p.holes.push(Hole { from: 6, to: 8 });
        assert_eq!(Pipeline::from_bytes(&p.to_bytes()).unwrap(), p);
        let mut l = Lattice::new(4, 12);
        l.segs.push_back(Period {
            start: 40,
            period: 6,
        });
        assert_eq!(Lattice::from_bytes(&l.to_bytes()).unwrap(), l);
        let mut cargo = BTreeMap::new();
        cargo.insert(GoodId(2), 3_000);
        let c = Caravan {
            key: CaravanKey {
                edge: 4,
                dir: 1,
                departure: 77,
            },
            seed: 0xdead_beef,
            home: 3,
            group: GroupKind::RibbitCaravan,
            members: vec![
                Member {
                    body: Body::RibbitMerchant,
                    alive: true,
                    present: false,
                    cargo: cargo.clone(),
                },
                Member {
                    body: Body::Llama,
                    alive: false,
                    present: true,
                    cargo: BTreeMap::new(),
                },
            ],
            stage: Stage::Returning,
            leg: Leg {
                from: 1,
                to: 3,
                start: Tick(100),
                end: Tick(900),
                generation: 5,
            },
            holder: Handle::from_parts(9, 2),
            notable: None,
            quotes: cargo.clone(),
            purse: cargo,
        };
        assert_eq!(Caravan::from_bytes(&c.to_bytes()).unwrap(), c);
    }
}
