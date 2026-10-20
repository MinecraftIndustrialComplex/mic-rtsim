// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the knowledge store (the phase 1 world's snapshot, docs/overnight-choices-persist.md).
//! The state hash's [`rtsim_kernel::codec::Encode`] for [`Knowledge`] leaves the wheels and the
//! derived orders out; [`Knowledge::save`] writes everything, so a store read back with
//! [`Knowledge::load`] expires reports, kills notables and evicts counters exactly as the saved
//! one would.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::wheel::TimingWheel;

use crate::actor::{Actor, ActorId, ActorKind, Actors};
use crate::board::Noticeboard;
use crate::gossip::{GossipType, VanillaGossip};
use crate::knowledge::{Counters, Holder, HolderKind, Knowledge};
use crate::notable::{MemorySlot, Notable};
use crate::report::{ReportEntry, ReportKind, ReportRow, ReportTable};
use crate::sentiment::{SentimentEntry, Sentiments};
use crate::tunables::Tunables;

fn bad<T>(what: String) -> codec::Result<T> {
    Err(CodecError::Invalid(what))
}

fn id(r: &mut Reader<'_>) -> codec::Result<Handle<Actor>> {
    Ok(Handle::from_bits(r.u64()?))
}

fn actor_kind(v: u8) -> codec::Result<ActorKind> {
    ActorKind::from_u8(v).map_or_else(|| bad(format!("actor kind {v}")), Ok)
}

fn report_kind(v: u16) -> codec::Result<ReportKind> {
    ReportKind::from_u16(v).map_or_else(|| bad(format!("report kind {v}")), Ok)
}

fn gossip_type(v: u8) -> codec::Result<GossipType> {
    Ok(match v {
        1 => GossipType::MajorNegative,
        2 => GossipType::MinorNegative,
        3 => GossipType::MinorPositive,
        4 => GossipType::MajorPositive,
        5 => GossipType::Trading,
        _ => return bad(format!("gossip type {v}")),
    })
}

fn put_board(w: &mut Writer, b: &Noticeboard) {
    w.var_u64(b.by_seq.len() as u64);
    for (seq, r) in &b.by_seq {
        w.var_u64(*seq);
        w.u64(r.to_bits());
    }
    w.var_u64(b.next_seq);
    w.u32(b.cap);
}

fn get_board(r: &mut Reader<'_>) -> codec::Result<Noticeboard> {
    let mut by_seq = BTreeMap::new();
    let mut seq_of = BTreeMap::new();
    for _ in 0..r.len_prefix(9)? {
        let seq = r.var_u64()?;
        let rep = Handle::from_bits(r.u64()?);
        by_seq.insert(seq, rep);
        seq_of.insert(rep, seq);
    }
    if seq_of.len() != by_seq.len() {
        return bad("a noticeboard holds one report twice".into());
    }
    Ok(Noticeboard {
        by_seq,
        seq_of,
        next_seq: r.var_u64()?,
        cap: r.u32()?,
    })
}

fn put_sentiments(w: &mut Writer, s: &Sentiments) {
    w.u64(s.holder.to_bits());
    w.u64(s.anchor.0);
    w.u16(s.cap);
    w.var_u64(s.entries.len() as u64);
    s.entries.iter().for_each(|e| w.raw(&e.0));
}

fn get_sentiments(r: &mut Reader<'_>) -> codec::Result<Sentiments> {
    let holder = id(r)?;
    let anchor = Tick(r.u64()?);
    let cap = r.u16()?;
    let n = r.len_prefix(8)?;
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        entries.push(SentimentEntry(r.take(8)?.try_into().expect("8 bytes")));
    }
    Ok(Sentiments {
        holder,
        anchor,
        cap,
        entries,
    })
}

fn put_report_table(w: &mut Writer, t: &ReportTable) {
    t.arena.encode_with(w, |e, w| {
        w.raw(&e.row.0);
        w.u64(e.expires.0);
        w.var_u64(e.knowers.len() as u64);
        e.knowers.iter().for_each(|k| w.u64(k.to_bits()));
    });
    w.var_u64(t.order.len() as u64);
    for &(player, at, severity, bits) in &t.order {
        w.bool(player);
        w.u64(at);
        w.u8(severity);
        w.u64(bits);
    }
    t.expiry.encode_with(w, |r, w| w.u64(r.to_bits()));
    w.u32(t.cap);
}

fn get_report_table(r: &mut Reader<'_>) -> codec::Result<ReportTable> {
    let arena = Arena::decode_with(r, |r| {
        let row = ReportRow(r.take(32)?.try_into().expect("32 bytes"));
        let expires = Tick(r.u64()?);
        let n = r.len_prefix(8)?;
        let knowers = (0..n).map(|_| id(r)).collect::<codec::Result<_>>()?;
        Ok(ReportEntry {
            row,
            expires,
            knowers,
        })
    })?;
    let mut order = BTreeSet::new();
    for _ in 0..r.len_prefix(18)? {
        order.insert((r.bool()?, r.u64()?, r.u8()?, r.u64()?));
    }
    let expiry = TimingWheel::decode_with(r, |r| Ok(Handle::from_bits(r.u64()?)))?;
    Ok(ReportTable {
        arena,
        order,
        expiry,
        cap: r.u32()?,
    })
}

fn put_notable(w: &mut Writer, n: &Notable) {
    w.u64(n.seed);
    w.u64(n.home.to_bits());
    w.u64(n.heir.to_bits());
    w.u64(n.born.0);
    w.u64(n.dies.0);
    w.var_u64(n.memories.len() as u64);
    n.memories.iter().for_each(|m| w.raw(&m.0));
    w.u8(n.slots);
}

fn get_notable(r: &mut Reader<'_>) -> codec::Result<Notable> {
    let seed = r.u64()?;
    let home = id(r)?;
    let heir = id(r)?;
    let born = Tick(r.u64()?);
    let dies = Tick(r.u64()?);
    let n = r.len_prefix(8)?;
    let mut memories = Vec::with_capacity(n);
    for _ in 0..n {
        memories.push(MemorySlot(r.take(8)?.try_into().expect("8 bytes")));
    }
    Ok(Notable {
        seed,
        home,
        heir,
        born,
        dies,
        memories,
        slots: r.u8()?,
    })
}

fn put_key(w: &mut Writer, k: &(ActorId, ActorId, ReportKind)) {
    w.u64(k.0.to_bits());
    w.u64(k.1.to_bits());
    w.u16(k.2 as u16);
}

fn get_key(r: &mut Reader<'_>) -> codec::Result<(ActorId, ActorId, ReportKind)> {
    Ok((id(r)?, id(r)?, report_kind(r.u16()?)?))
}

impl Knowledge {
    /// Everything, wheels and derived orders included (the tunables are the caller's: they come
    /// from the world's configuration).
    pub fn save(&self, w: &mut Writer) {
        w.u64(self.world_seed);
        self.actors.arena.encode_with(w, |a, w| {
            w.u8(a.kind as u8);
            w.u64(a.external);
        });
        w.var_u64(self.actors.index.len() as u64);
        for (&(k, ext), a) in &self.actors.index {
            w.u8(k as u8);
            w.u64(ext);
            w.u64(a.to_bits());
        }
        put_report_table(w, &self.reports);
        w.var_u64(self.holders.len() as u64);
        for (a, h) in &self.holders {
            w.u64(a.to_bits());
            w.u8(h.kind as u8);
            put_board(w, &h.board);
            put_sentiments(w, &h.sentiment);
        }
        w.var_u64(self.notables.len() as u64);
        for (a, n) in &self.notables {
            w.u64(a.to_bits());
            put_notable(w, n);
        }
        self.deaths.encode_with(w, |a, w| w.u64(a.to_bits()));
        let c = &self.counters;
        w.var_u64(c.map.len() as u64);
        for (k, v) in &c.map {
            put_key(w, k);
            w.u32(*v);
        }
        w.var_u64(c.by_count.len() as u64);
        for (v, k) in &c.by_count {
            w.u32(*v);
            put_key(w, k);
        }
        w.u32(c.cap);
        w.var_u64(self.vanilla.len() as u64);
        for g in &self.vanilla {
            w.u64(g.village.to_bits());
            w.u64(g.player.to_bits());
            w.u8(g.gossip as u8);
            w.var_i64(i64::from(g.amount));
            w.u64(g.report.to_bits());
            w.u16(g.kind as u16);
            w.u64(g.at.0);
            w.bool(g.first_hand);
        }
    }

    pub fn load(r: &mut Reader<'_>, tunables: Tunables) -> codec::Result<Knowledge> {
        let world_seed = r.u64()?;
        let arena = Arena::decode_with(r, |r| {
            Ok(Actor {
                kind: actor_kind(r.u8()?)?,
                external: r.u64()?,
            })
        })?;
        let mut index = BTreeMap::new();
        for _ in 0..r.len_prefix(17)? {
            let k = actor_kind(r.u8()?)?;
            let ext = r.u64()?;
            index.insert((k, ext), id(r)?);
        }
        let actors = Actors { arena, index };
        let reports = get_report_table(r)?;
        let mut holders = BTreeMap::new();
        for _ in 0..r.len_prefix(9)? {
            let a = id(r)?;
            let kind = r.u8()?;
            let kind =
                HolderKind::from_u8(kind).map_or_else(|| bad(format!("holder kind {kind}")), Ok)?;
            holders.insert(
                a,
                Holder {
                    kind,
                    board: get_board(r)?,
                    sentiment: get_sentiments(r)?,
                },
            );
        }
        let mut notables = BTreeMap::new();
        for _ in 0..r.len_prefix(41)? {
            let a = id(r)?;
            notables.insert(a, get_notable(r)?);
        }
        let deaths = TimingWheel::decode_with(r, id)?;
        let mut map = BTreeMap::new();
        for _ in 0..r.len_prefix(22)? {
            let k = get_key(r)?;
            map.insert(k, r.u32()?);
        }
        let mut by_count = BTreeSet::new();
        for _ in 0..r.len_prefix(22)? {
            let v = r.u32()?;
            by_count.insert((v, get_key(r)?));
        }
        let counters = Counters {
            map,
            by_count,
            cap: r.u32()?,
        };
        let n = r.len_prefix(36)?;
        let mut vanilla = Vec::with_capacity(n);
        for _ in 0..n {
            vanilla.push(VanillaGossip {
                village: id(r)?,
                player: id(r)?,
                gossip: gossip_type(r.u8()?)?,
                amount: i32::try_from(r.var_i64()?)
                    .map_err(|_| CodecError::Invalid("a gossip amount over i32".into()))?,
                report: Handle::from_bits(r.u64()?),
                kind: report_kind(r.u16()?)?,
                at: Tick(r.u64()?),
                first_hand: r.bool()?,
            });
        }
        Ok(Knowledge {
            world_seed,
            tunables,
            actors,
            reports,
            holders,
            notables,
            deaths,
            counters,
            vanilla,
        })
    }
}
