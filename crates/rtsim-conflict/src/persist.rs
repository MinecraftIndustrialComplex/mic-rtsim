// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine's save (the persist pattern of docs/overnight-choices-persist.md, S1 and
//! S8): every table, wheel and index it holds, private fields included, so an engine read back with
//! [`Conflict::load`] or [`Conflict::open_paged`] continues exactly as the one saved: the same
//! [`Conflict::state_hash`] after the same inputs, bit for bit (grievance anchors are stored as
//! they are, never recomputed).
//!
//! Two forms (docs/choices-paging-modules.md, section Conflict):
//!
//! - **The blob** ([`Conflict::save`], [`Conflict::load`]): the resident part, then each row
//!   table ([`crate::tables`]) in key order. What rtsim-jni writes as page 0x4300/0 while it does
//!   not page the conflict, and what [`Conflict::state_hash`] hashes.
//! - **Paged** ([`Conflict::save_paged`], [`Conflict::open_paged`]): the resident part and the row
//!   tables' counts as one meta page, and each row table's dirty pages.
//!
//! **Not written:** the tunables and the cultures (the caller's data; a fingerprint of the
//! cultures is saved and checked, so a save is never read with other data), and the log. The
//! parties' legs, the chiefs and heirs, and the faction actors live in the phase 1 world (its leg
//! index and knowledge), saved beside this with `World::save`. Nothing is rebuilt on load by
//! walking the rows: the resident indexes (factions by key and by cell, sources by key) are saved.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rtsim_industry::ids::DistrictId;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::page::{PageError, PageKey, Result as PageResult, SaveBatch};
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::rng::fnv1a64;
use rtsim_kernel::rows::{PageSource, PagedRows, RowStats};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::ActorId;
use rtsim_settle::Pos;
use rtsim_trade::GoodId;
use rtsim_travel::legs::{Meeting, Stamp};

use crate::boundary;
use crate::culture::{Cultures, PartyKind, SignalKind, Step};
use crate::engine::{
    Conflict, ConflictLedger, ConflictStats, Demand, Ev, Faction, Feud, Focus, Party, Phase,
    Source, Target,
};
use crate::grievance::{Grievance, K};
use crate::presence::{ChunkKey, ChunkTime, Presence, Stay};
use crate::tables::{
    FACTION_TABLE, FACTIONS_PER_PAGE, FactionRow, PLACE_TABLE, PLACES_PER_PAGE, PlaceRow,
    SOURCE_TABLE, SOURCES_PER_PAGE, SourceRef, SourceRow,
};
use crate::tunables::ConflictTunables;

/// The blob ([`Conflict::save`]). RTC2: rows by `PagedRows::encode_all` (RTC1 held a vector of
/// factions and maps of feuds and sources).
const MAGIC: u32 = u32::from_le_bytes(*b"RTC2");
/// The meta page of a paged save ([`Conflict::save_paged`]).
const META_MAGIC: u32 = u32::from_le_bytes(*b"RTCM");
const META_VERSION: u16 = 1;

fn bad<T>(what: String) -> codec::Result<T> {
    Err(CodecError::Invalid(what))
}

fn put_usize(w: &mut Writer, v: usize) {
    w.var_u64(v as u64);
}

fn get_usize(r: &mut Reader<'_>) -> codec::Result<usize> {
    let v = r.var_u64()?;
    usize::try_from(v).map_err(|_| CodecError::Invalid(format!("index {v}")))
}

fn get_u32(r: &mut Reader<'_>) -> codec::Result<u32> {
    let v = r.var_u64()?;
    u32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} over u32")))
}

fn get_i32(r: &mut Reader<'_>) -> codec::Result<i32> {
    let v = r.var_i64()?;
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} over i32")))
}

fn handle<T>(r: &mut Reader<'_>) -> codec::Result<Handle<T>> {
    Ok(Handle::from_bits(r.u64()?))
}

fn put_chunk(w: &mut Writer, c: ChunkKey) {
    w.u16(c.0);
    w.var_i64(i64::from(c.1));
    w.var_i64(i64::from(c.2));
}

fn get_chunk(r: &mut Reader<'_>) -> codec::Result<ChunkKey> {
    Ok((r.u16()?, get_i32(r)?, get_i32(r)?))
}

/// P14's time-spent counter, in key order (its touch index is rebuilt).
fn put_presence(w: &mut Writer, p: &Presence) {
    put_usize(w, p.chunks.len());
    for (k, ct) in &p.chunks {
        put_chunk(w, *k);
        w.u64(ct.touched.0);
        put_usize(w, ct.stays.len());
        for s in &ct.stays {
            w.u64(s.player.to_bits());
            w.f64(s.v);
            w.u64(s.at.0);
        }
    }
    put_usize(w, p.last.len());
    for (a, (k, t)) in &p.last {
        w.u64(a.to_bits());
        put_chunk(w, *k);
        w.u64(t.0);
    }
}

fn get_presence(r: &mut Reader<'_>, p: &mut Presence) -> codec::Result<()> {
    let n = r.len_prefix(12)?;
    for _ in 0..n {
        let k = get_chunk(r)?;
        let touched = Tick(r.u64()?);
        let m = r.len_prefix(24)?;
        let stays = (0..m)
            .map(|_| {
                Ok(Stay {
                    player: handle(r)?,
                    v: r.f64()?,
                    at: Tick(r.u64()?),
                })
            })
            .collect::<codec::Result<Vec<_>>>()?;
        p.by_touch.insert((touched, k));
        p.chunks.insert(k, ChunkTime { stays, touched });
    }
    let n = r.len_prefix(20)?;
    for _ in 0..n {
        let a = handle(r)?;
        let k = get_chunk(r)?;
        p.last.insert(a, (k, Tick(r.u64()?)));
    }
    Ok(())
}

fn put_pos(w: &mut Writer, p: Pos) {
    w.u16(p.frame);
    w.var_i64(i64::from(p.x));
    w.var_i64(i64::from(p.z));
}

fn get_pos(r: &mut Reader<'_>) -> codec::Result<Pos> {
    Ok(Pos::new(r.u16()?, get_i32(r)?, get_i32(r)?))
}

fn put_opt<T>(w: &mut Writer, v: &Option<T>, f: impl FnOnce(&mut Writer, &T)) {
    match v {
        Some(x) => {
            w.u8(1);
            f(w, x);
        }
        None => w.u8(0),
    }
}

fn get_opt<T>(
    r: &mut Reader<'_>,
    f: impl FnOnce(&mut Reader<'_>) -> codec::Result<T>,
) -> codec::Result<Option<T>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(f(r)?),
        t => return bad(format!("option tag {t}")),
    })
}

fn put_goods(w: &mut Writer, m: &BTreeMap<GoodId, i64>) {
    put_usize(w, m.len());
    for (g, a) in m {
        w.u16(g.0);
        w.var_i64(*a);
    }
}

fn get_goods(r: &mut Reader<'_>) -> codec::Result<BTreeMap<GoodId, i64>> {
    let n = r.len_prefix(2)?;
    let mut m = BTreeMap::new();
    for _ in 0..n {
        m.insert(GoodId(r.u16()?), r.var_i64()?);
    }
    Ok(m)
}

fn put_big(w: &mut Writer, m: &BTreeMap<GoodId, i128>) {
    put_usize(w, m.len());
    for (g, a) in m {
        w.u16(g.0);
        w.i128(*a);
    }
}

fn get_big(r: &mut Reader<'_>) -> codec::Result<BTreeMap<GoodId, i128>> {
    let n = r.len_prefix(2)?;
    let mut m = BTreeMap::new();
    for _ in 0..n {
        m.insert(GoodId(r.u16()?), r.i128()?);
    }
    Ok(m)
}

fn put_stacks(w: &mut Writer, v: &[(u8, u32)]) {
    put_usize(w, v.len());
    for (u, n) in v {
        w.u8(*u);
        w.var_u64(u64::from(*n));
    }
}

fn get_stacks(r: &mut Reader<'_>) -> codec::Result<Vec<(u8, u32)>> {
    let n = r.len_prefix(2)?;
    (0..n).map(|_| Ok((r.u8()?, get_u32(r)?))).collect()
}

fn get_kind(r: &mut Reader<'_>) -> codec::Result<SignalKind> {
    let v = r.u8()?;
    SignalKind::from_u32(u32::from(v)).ok_or_else(|| CodecError::Invalid(format!("signal {v}")))
}

fn put_grievance(w: &mut Writer, g: &Grievance) {
    w.u64(g.anchor.0);
    for v in g.level.iter().chain(g.rate.iter()) {
        w.f64(*v);
    }
}

fn get_grievance(r: &mut Reader<'_>) -> codec::Result<Grievance> {
    let anchor = Tick(r.u64()?);
    let mut level = [0.0; K];
    let mut rate = [0.0; K];
    for v in level.iter_mut().chain(rate.iter_mut()) {
        *v = r.f64()?;
    }
    Ok(Grievance {
        anchor,
        level,
        rate,
    })
}

fn put_faction(w: &mut Writer, x: &Faction) {
    w.u64(x.key);
    w.u16(x.culture);
    put_usize(w, x.node);
    put_pos(w, x.pos);
    w.var_u64(u64::from(x.radius));
    w.u64(x.actor.to_bits());
    w.u64(x.chief.to_bits());
    w.u64(x.chief_seed);
    w.u64(x.heir.to_bits());
    w.u64(x.heir_seed);
    w.var_u64(u64::from(x.chiefs));
    w.u64(x.pool_at.0);
    w.f64(x.pool);
    w.u64(x.cowed_until.0);
    w.var_u64(u64::from(x.plunders));
    w.var_u64(u64::from(x.plunder_stamp));
    put_usize(w, x.observed.len());
    for (s, v) in &x.observed {
        w.f64(*s);
        w.f64(*v);
    }
    w.bool(x.chief_out);
}

fn get_faction(r: &mut Reader<'_>) -> codec::Result<Faction> {
    Ok(Faction {
        key: r.u64()?,
        culture: r.u16()?,
        node: get_usize(r)?,
        pos: get_pos(r)?,
        radius: get_u32(r)?,
        actor: handle(r)?,
        chief: handle(r)?,
        chief_seed: r.u64()?,
        heir: handle(r)?,
        heir_seed: r.u64()?,
        chiefs: get_u32(r)?,
        pool_at: Tick(r.u64()?),
        pool: r.f64()?,
        cowed_until: Tick(r.u64()?),
        plunders: get_u32(r)?,
        plunder_stamp: get_u32(r)?,
        observed: {
            let n = r.len_prefix(16)?;
            (0..n)
                .map(|_| Ok((r.f64()?, r.f64()?)))
                .collect::<codec::Result<_>>()?
        },
        chief_out: r.bool()?,
    })
}

fn put_focus(w: &mut Writer, f: &Focus) {
    put_pos(w, f.pos);
    put_opt(w, &f.district, |w, d| w.u64(d.0));
    w.f64(f.weight);
}

fn get_focus(r: &mut Reader<'_>) -> codec::Result<Focus> {
    Ok(Focus {
        pos: get_pos(r)?,
        district: get_opt(r, |r| Ok(DistrictId(r.u64()?)))?,
        weight: r.f64()?,
    })
}

fn put_demand(w: &mut Writer, d: &Demand) {
    w.u8(d.kind as u8);
    w.u16(d.good.0);
    w.var_i64(d.tribute);
    w.var_i64(d.paid);
    w.u64(d.made.0);
    w.u64(d.deadline.0);
    w.f64(d.after);
    put_opt(w, &d.envoy, |w, p| w.u64(p.to_bits()));
}

fn get_demand(r: &mut Reader<'_>) -> codec::Result<Demand> {
    Ok(Demand {
        kind: get_kind(r)?,
        good: GoodId(r.u16()?),
        tribute: r.var_i64()?,
        paid: r.var_i64()?,
        made: Tick(r.u64()?),
        deadline: Tick(r.u64()?),
        after: r.f64()?,
        envoy: get_opt(r, handle)?,
    })
}

fn put_feud(w: &mut Writer, x: &Feud) {
    put_grievance(w, &x.g);
    for v in &x.raw {
        w.f64(*v);
    }
    w.u8(x.step as u8);
    w.u64(x.since.0);
    put_opt(w, &x.demand, put_demand);
    put_opt(w, &x.party, |w, p| w.u64(p.to_bits()));
    w.var_u64(u64::from(x.stamp));
    put_opt(w, &x.focus, put_focus);
    w.u64(x.wait_until.0);
    w.var_u64(u64::from(x.raids));
}

fn get_feud(r: &mut Reader<'_>) -> codec::Result<Feud> {
    let g = get_grievance(r)?;
    let mut raw = [0.0; K];
    for v in &mut raw {
        *v = r.f64()?;
    }
    let s = r.u8()?;
    Ok(Feud {
        g,
        raw,
        step: Step::from_u8(s).ok_or_else(|| CodecError::Invalid(format!("step {s}")))?,
        since: Tick(r.u64()?),
        demand: get_opt(r, get_demand)?,
        party: get_opt(r, handle)?,
        stamp: get_u32(r)?,
        focus: get_opt(r, get_focus)?,
        wait_until: Tick(r.u64()?),
        raids: get_u32(r)?,
    })
}

fn put_target(w: &mut Writer, t: &Target) {
    match t {
        Target::District(d, p) => {
            w.u8(0);
            w.u64(d.0);
            put_pos(w, *p);
        }
        Target::Node(i) => {
            w.u8(1);
            put_usize(w, *i);
        }
        Target::Pos(p) => {
            w.u8(2);
            put_pos(w, *p);
        }
    }
}

fn get_target(r: &mut Reader<'_>) -> codec::Result<Target> {
    Ok(match r.u8()? {
        0 => Target::District(DistrictId(r.u64()?), get_pos(r)?),
        1 => Target::Node(get_usize(r)?),
        2 => Target::Pos(get_pos(r)?),
        t => return bad(format!("target tag {t}")),
    })
}

fn put_party(w: &mut Writer, x: &Party) {
    w.var_u64(u64::from(x.faction));
    w.u8(x.kind as u8);
    put_opt(w, &x.feud, |w, a| w.u64(a.to_bits()));
    put_stacks(w, &x.units);
    put_stacks(w, &x.spawned);
    put_stacks(w, &x.recalled);
    w.var_u64(u64::from(x.start_units));
    put_target(w, &x.target);
    w.u16(x.frame);
    put_usize(w, x.route.len());
    for (px, pz) in &x.route {
        w.var_i64(i64::from(*px));
        w.var_i64(i64::from(*pz));
    }
    put_usize(w, x.times.len());
    for t in &x.times {
        w.u64(t.0);
    }
    w.var_u64(u64::from(x.seg));
    w.u8(x.phase as u8);
    w.u64(x.until.0);
    w.u64(x.bored_at.0);
    w.var_u64(u64::from(x.plan));
    put_opt(w, &x.leg, |w, s| s.encode(w));
    w.var_u64(u64::from(x.epoch));
    w.bool(x.embodied);
    put_opt(w, &x.seen_by, |w, a| w.u64(a.to_bits()));
    w.u16(x.wave);
    put_goods(w, &x.loot);
    put_goods(w, &x.from_players);
    w.bool(x.chief);
    put_opt(w, &x.breaker, |w, a| w.u64(a.to_bits()));
    w.f64(x.fight_strength);
}

fn get_party(r: &mut Reader<'_>) -> codec::Result<Party> {
    let faction = get_u32(r)?;
    let k = r.u8()?;
    let kind = PartyKind::from_u8(k).ok_or_else(|| CodecError::Invalid(format!("party {k}")))?;
    let feud = get_opt(r, handle)?;
    let units = get_stacks(r)?;
    let spawned = get_stacks(r)?;
    let recalled = get_stacks(r)?;
    let start_units = get_u32(r)?;
    let target = get_target(r)?;
    let frame = r.u16()?;
    let n = r.len_prefix(2)?;
    let route = (0..n)
        .map(|_| Ok((get_i32(r)?, get_i32(r)?)))
        .collect::<codec::Result<Vec<_>>>()?;
    let n = r.len_prefix(8)?;
    let times = (0..n)
        .map(|_| Ok(Tick(r.u64()?)))
        .collect::<codec::Result<Vec<_>>>()?;
    if route.len() < 2 || times.len() != route.len() {
        return bad("a party's route and times disagree".into());
    }
    let seg = get_u32(r)?;
    let ph = r.u8()?;
    Ok(Party {
        faction,
        kind,
        feud,
        units,
        spawned,
        recalled,
        start_units,
        target,
        frame,
        route,
        times,
        seg,
        phase: Phase::from_u8(ph).ok_or_else(|| CodecError::Invalid(format!("phase {ph}")))?,
        until: Tick(r.u64()?),
        bored_at: Tick(r.u64()?),
        plan: get_u32(r)?,
        leg: get_opt(r, Stamp::decode)?,
        epoch: get_u32(r)?,
        embodied: r.bool()?,
        seen_by: get_opt(r, handle)?,
        wave: r.u16()?,
        loot: get_goods(r)?,
        from_players: get_goods(r)?,
        chief: r.bool()?,
        breaker: get_opt(r, handle)?,
        fight_strength: r.f64()?,
    })
}

fn put_meeting(w: &mut Writer, m: &Meeting) {
    w.u64(m.at.0);
    m.a.encode(w);
    m.b.encode(w);
}

fn get_meeting(r: &mut Reader<'_>) -> codec::Result<Meeting> {
    Ok(Meeting {
        at: Tick(r.u64()?),
        a: Stamp::decode(r)?,
        b: Stamp::decode(r)?,
    })
}

fn put_ev(w: &mut Writer, e: &Ev) {
    match *e {
        Ev::Feud {
            faction,
            actor,
            stamp,
        } => {
            w.u8(0);
            w.var_u64(u64::from(faction));
            w.u64(actor.to_bits());
            w.var_u64(u64::from(stamp));
        }
        Ev::Party { party, plan } => {
            w.u8(1);
            w.u64(party.to_bits());
            w.var_u64(u64::from(plan));
        }
        Ev::Seen {
            party,
            player,
            meeting,
        } => {
            w.u8(2);
            w.u64(party.to_bits());
            w.u64(player.to_bits());
            put_meeting(w, &meeting);
        }
        Ev::Unseen { party, epoch } => {
            w.u8(3);
            w.u64(party.to_bits());
            w.var_u64(u64::from(epoch));
        }
        Ev::Wave { party, plan } => {
            w.u8(4);
            w.u64(party.to_bits());
            w.var_u64(u64::from(plan));
        }
        Ev::Ambush { party, plan, train } => {
            w.u8(5);
            w.u64(party.to_bits());
            w.var_u64(u64::from(plan));
            w.u64(train);
        }
        Ev::Plunder { faction, stamp } => {
            w.u8(6);
            w.var_u64(u64::from(faction));
            w.var_u64(u64::from(stamp));
        }
    }
}

fn get_ev(r: &mut Reader<'_>) -> codec::Result<Ev> {
    Ok(match r.u8()? {
        0 => Ev::Feud {
            faction: get_u32(r)?,
            actor: handle(r)?,
            stamp: get_u32(r)?,
        },
        1 => Ev::Party {
            party: handle(r)?,
            plan: get_u32(r)?,
        },
        2 => Ev::Seen {
            party: handle(r)?,
            player: handle(r)?,
            meeting: get_meeting(r)?,
        },
        3 => Ev::Unseen {
            party: handle(r)?,
            epoch: get_u32(r)?,
        },
        4 => Ev::Wave {
            party: handle(r)?,
            plan: get_u32(r)?,
        },
        5 => Ev::Ambush {
            party: handle(r)?,
            plan: get_u32(r)?,
            train: r.u64()?,
        },
        6 => Ev::Plunder {
            faction: get_u32(r)?,
            stamp: get_u32(r)?,
        },
        t => return bad(format!("event tag {t}")),
    })
}

fn put_ledger(w: &mut Writer, l: &ConflictLedger) {
    for m in [
        &l.from_villages,
        &l.from_players,
        &l.landed,
        &l.landed_from_players,
        &l.tribute,
        &l.gifts,
    ] {
        put_big(w, m);
    }
    put_usize(w, l.unmapped.len());
    for (k, v) in &l.unmapped {
        w.bytes(k.as_bytes());
        w.var_i64(*v);
    }
}

fn get_ledger(r: &mut Reader<'_>) -> codec::Result<ConflictLedger> {
    let mut l = ConflictLedger {
        from_villages: get_big(r)?,
        from_players: get_big(r)?,
        landed: get_big(r)?,
        landed_from_players: get_big(r)?,
        tribute: get_big(r)?,
        gifts: get_big(r)?,
        unmapped: BTreeMap::new(),
    };
    let n = r.len_prefix(2)?;
    for _ in 0..n {
        let k = String::from_utf8(r.bytes()?.to_vec())
            .map_err(|_| CodecError::Invalid("a good name not UTF-8".into()))?;
        l.unmapped.insert(k, r.var_i64()?);
    }
    Ok(l)
}

fn stats_fields(s: &ConflictStats) -> Vec<u64> {
    let mut v = vec![s.events, s.feud_wakes, s.stale, s.signals, s.emissions];
    v.extend(s.escalations);
    v.extend([s.calmed, s.demands, s.tributes, s.cuts, s.expired, s.gifts]);
    v.extend(s.parties);
    v.extend([
        s.raids_district,
        s.raids_village,
        s.raids_watched,
        s.raids_skipped,
        s.ambushes,
        s.casualties,
        s.routs,
        s.bored,
        s.chiefs_killed,
        s.successions,
        s.seen,
        s.materialized,
        s.waves,
        s.recalls,
        s.items_from_districts,
        s.items_from_villages,
        s.blocks_destroyed,
        s.feuds_forgotten,
        s.reattributed,
    ]);
    v
}

fn stats_from(v: &[u64]) -> ConflictStats {
    let mut it = v.iter().copied();
    let mut n = || it.next().unwrap_or(0);
    let mut s = ConflictStats {
        events: n(),
        feud_wakes: n(),
        stale: n(),
        signals: n(),
        emissions: n(),
        ..ConflictStats::default()
    };
    for e in &mut s.escalations {
        *e = n();
    }
    s.calmed = n();
    s.demands = n();
    s.tributes = n();
    s.cuts = n();
    s.expired = n();
    s.gifts = n();
    for p in &mut s.parties {
        *p = n();
    }
    s.raids_district = n();
    s.raids_village = n();
    s.raids_watched = n();
    s.raids_skipped = n();
    s.ambushes = n();
    s.casualties = n();
    s.routs = n();
    s.bored = n();
    s.chiefs_killed = n();
    s.successions = n();
    s.seen = n();
    s.materialized = n();
    s.waves = n();
    s.recalls = n();
    s.items_from_districts = n();
    s.items_from_villages = n();
    s.blocks_destroyed = n();
    s.feuds_forgotten = n();
    s.reattributed = n();
    s
}

/// A continuous source's fields (its key and kind are the row's).
fn put_source(w: &mut Writer, s: &Source) {
    put_pos(w, s.pos);
    put_opt(w, &s.district, |w, d| w.u64(d.0));
    w.f64(s.rate);
    put_usize(w, s.affects.len());
    for (f, fall) in &s.affects {
        w.var_u64(u64::from(*f));
        w.f64(*fall);
    }
    w.u64(s.actor.to_bits());
    put_usize(w, s.chunks.len());
    for c in &s.chunks {
        put_chunk(w, *c);
    }
}

fn get_source(r: &mut Reader<'_>) -> codec::Result<Source> {
    let pos = get_pos(r)?;
    let district = get_opt(r, |r| Ok(DistrictId(r.u64()?)))?;
    let rate = r.f64()?;
    let m = r.len_prefix(9)?;
    let affects = (0..m)
        .map(|_| Ok((get_u32(r)?, r.f64()?)))
        .collect::<codec::Result<Vec<_>>>()?;
    let actor = handle(r)?;
    // A chunk is at least a u16 and two one-byte varints.
    let m = r.len_prefix(4)?;
    let chunks = (0..m)
        .map(|_| get_chunk(r))
        .collect::<codec::Result<Vec<_>>>()?;
    Ok(Source {
        pos,
        district,
        rate,
        affects,
        actor,
        chunks,
    })
}

impl Encode for FactionRow {
    fn encode(&self, w: &mut Writer) {
        put_faction(w, &self.x);
        put_usize(w, self.feuds.len());
        for (a, x) in &self.feuds {
            w.u64(a.to_bits());
            put_feud(w, x);
        }
        put_usize(w, self.reach.len());
        for (k, id) in &self.reach {
            w.u8(*k as u8);
            w.var_u64(u64::from(*id));
        }
    }
}

impl Decode for FactionRow {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let x = get_faction(r)?;
        let n = r.len_prefix(8)?;
        let mut feuds = Vec::with_capacity(n);
        for _ in 0..n {
            let a: ActorId = handle(r)?;
            if feuds.last().is_some_and(|l: &(ActorId, Feud)| l.0 >= a) {
                return bad("a faction's feuds out of order".into());
            }
            feuds.push((a, get_feud(r)?));
        }
        let n = r.len_prefix(2)?;
        let mut reach = Vec::with_capacity(n);
        for _ in 0..n {
            let e = (get_kind(r)?, get_u32(r)?);
            if reach.last().is_some_and(|l| *l >= e) {
                return bad("a faction's sources out of order".into());
            }
            reach.push(e);
        }
        Ok(FactionRow { x, feuds, reach })
    }
}

impl Encode for SourceRow {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.key);
        w.u8(self.kind as u8);
        put_source(w, &self.s);
    }
}

impl Decode for SourceRow {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(SourceRow {
            key: r.u64()?,
            kind: get_kind(r)?,
            s: get_source(r)?,
        })
    }
}

impl Encode for PlaceRow {
    fn encode(&self, w: &mut Writer) {
        put_usize(w, self.entries.len());
        for (c, id) in &self.entries {
            put_chunk(w, *c);
            w.var_u64(u64::from(*id));
        }
    }
}

impl Decode for PlaceRow {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let n = r.len_prefix(4)?;
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let e = (get_chunk(r)?, get_u32(r)?);
            if entries.last().is_some_and(|l| *l >= e) {
                return bad("a place row out of order".into());
            }
            entries.push(e);
        }
        Ok(PlaceRow { entries })
    }
}

/// What a paged save hands back: one ticket per row table, for [`Conflict::saved`] once the
/// commit is durable or [`Conflict::save_failed`] if it failed.
#[derive(Debug)]
pub struct ConflictTickets {
    factions: SaveTicket,
    sources: SaveTicket,
    places: SaveTicket,
}

impl ConflictTickets {
    /// Row pages the save wrote (the meta page aside).
    pub fn pages(&self) -> usize {
        self.factions.pages() + self.sources.pages() + self.places.pages()
    }
}

/// The budget's split between the row tables (PM-C7): factions half, sources and places a
/// quarter each.
fn split(budget: usize) -> (usize, usize, usize) {
    let f = budget / 2;
    let s = budget / 4;
    (f, s, budget - f - s)
}

impl Conflict {
    /// Everything resident (PM-C1): the header, the resident indexes, the row tables' counters,
    /// parties, the wheel, players, ops, the dictionary, the ledger, the stats, the outbox and
    /// presence. The meta page and the blob both start with it.
    fn put_resident(&self, w: &mut Writer) {
        w.u64(self.cultures.fingerprint());
        w.u64(self.seed);
        w.u64(self.now.0);
        put_usize(w, self.by_key.len());
        for (k, f) in &self.by_key {
            w.u64(*k);
            w.var_u64(u64::from(*f));
        }
        put_usize(w, self.cells.len());
        for ((frame, cx, cz), f) in &self.cells {
            w.u16(*frame);
            w.var_i64(i64::from(*cx));
            w.var_i64(i64::from(*cz));
            w.var_u64(u64::from(*f));
        }
        w.var_u64(self.feud_count);
        put_usize(w, self.sources.ids.len());
        for ((k, kind), r) in &self.sources.ids {
            w.u64(*k);
            w.u8(*kind as u8);
            w.var_u64(u64::from(r.id));
            w.f64(r.rate);
        }
        w.var_u64(u64::from(self.sources.next));
        self.parties.encode_with(w, |x, w| put_party(w, x));
        self.wheel.encode_with(w, |e, w| put_ev(w, e));
        put_usize(w, self.players.len());
        for (a, u) in &self.players {
            w.u64(a.to_bits());
            w.u64(u.0);
            w.u64(u.1);
        }
        put_usize(w, self.applied_ops.len());
        self.applied_ops.iter().for_each(|o| w.u64(*o));
        w.u64(self.settlers.to_bits());
        put_usize(w, self.java_dict.len());
        for ((k, id), n) in &self.java_dict {
            w.u8(*k);
            w.var_u64(u64::from(*id));
            w.bytes(n.as_bytes());
        }
        put_ledger(w, &self.ledger);
        let st = stats_fields(&self.stats);
        put_usize(w, st.len());
        st.iter().for_each(|v| w.var_u64(*v));
        put_usize(w, self.outbox.len());
        for c in &self.outbox {
            let mut b = Vec::new();
            boundary::encode_cmd(0, c, &mut b);
            w.bytes(&b);
        }
        put_presence(w, &self.presence);
    }

    /// An engine from what [`Conflict::put_resident`] wrote, its row tables empty and resident.
    fn get_resident(
        r: &mut Reader<'_>,
        tun: ConflictTunables,
        cultures: Cultures,
    ) -> codec::Result<Conflict> {
        if r.u64()? != cultures.fingerprint() {
            return bad("the save was made with other culture data".into());
        }
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let mut c = Conflict::new(seed, tun, cultures, now);
        let n = r.len_prefix(9)?;
        for _ in 0..n {
            let k = r.u64()?;
            c.by_key.insert(k, get_u32(r)?);
        }
        let n = r.len_prefix(5)?;
        for _ in 0..n {
            let cell = (r.u16()?, get_i32(r)?, get_i32(r)?);
            c.cells.insert((cell, get_u32(r)?));
        }
        c.feud_count = r.var_u64()?;
        let n = r.len_prefix(18)?;
        for _ in 0..n {
            let k = r.u64()?;
            let kind = get_kind(r)?;
            let id = get_u32(r)?;
            let rate = r.f64()?;
            c.sources.ids.insert((k, kind), SourceRef { id, rate });
        }
        c.sources.next = get_u32(r)?;
        c.parties = Arena::decode_with(r, get_party)?;
        c.wheel = TimingWheel::decode_with(r, get_ev)?;
        let n = r.len_prefix(24)?;
        for _ in 0..n {
            let a = handle(r)?;
            c.players.insert(a, (r.u64()?, r.u64()?));
        }
        let n = r.len_prefix(8)?;
        let mut ops = BTreeSet::new();
        for _ in 0..n {
            ops.insert(r.u64()?);
        }
        c.applied_ops = ops;
        c.settlers = handle(r)?;
        let n = r.len_prefix(3)?;
        for _ in 0..n {
            let k = r.u8()?;
            let id = get_u32(r)?;
            let name = String::from_utf8(r.bytes()?.to_vec())
                .map_err(|_| CodecError::Invalid("a dictionary name not UTF-8".into()))?;
            c.java_dict.insert((k, id), name);
        }
        c.ledger = get_ledger(r)?;
        let n = r.len_prefix(1)?;
        let st = (0..n)
            .map(|_| r.var_u64())
            .collect::<codec::Result<Vec<_>>>()?;
        c.stats = stats_from(&st);
        let n = r.len_prefix(1)?;
        for _ in 0..n {
            let b = r.bytes()?;
            let (_, cmd, _) = boundary::decode_cmd(b)
                .map_err(|e| CodecError::Invalid(format!("a queued command: {e:?}")))?;
            c.outbox.push(cmd);
        }
        get_presence(r, &mut c.presence)?;
        Ok(c)
    }

    /// The resident indexes agree with the row tables' counts (no row is read).
    fn check_indexes(&self) -> codec::Result<()> {
        let n = self.factions.len();
        if self.by_key.len() != n || self.cells.len() != n {
            return bad(format!(
                "{n} factions, {} by key, {} by cell",
                self.by_key.len(),
                self.cells.len()
            ));
        }
        if self.by_key.values().any(|f| *f as usize >= n)
            || self.cells.iter().any(|e| e.1 as usize >= n)
        {
            return bad("an index names a faction past the table".into());
        }
        if self.sources.rows.len() != self.sources.ids.len() as u64 {
            return bad("source rows and their index disagree".into());
        }
        if self.sources.ids.values().any(|r| r.id >= self.sources.next) {
            return bad("a source id at or past the next".into());
        }
        Ok(())
    }

    /// The whole engine as one blob (see the module docs for what is left out): the resident
    /// part, then every row table in key order (`PagedRows::encode_all`). Equal engines give
    /// equal bytes, paged or not.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.feud_count as usize * 160);
        w.u32(MAGIC);
        self.put_resident(&mut w);
        self.factions.rows.encode_all(&mut w);
        self.sources.rows.encode_all(&mut w);
        self.places.rows.encode_all(&mut w);
        w.into_bytes()
    }

    /// Reads an engine [`Conflict::save`] wrote, with the tunables and cultures it ran with. The
    /// engine is resident (no page store).
    pub fn load(
        tun: ConflictTunables,
        cultures: Cultures,
        bytes: &[u8],
    ) -> codec::Result<Conflict> {
        let r = &mut Reader::new(bytes);
        if r.u32()? != MAGIC {
            return bad("not a conflict save".into());
        }
        let mut c = Self::get_resident(r, tun, cultures)?;
        c.factions.rows = PagedRows::decode_all(FACTION_TABLE, FACTIONS_PER_PAGE, r)?;
        c.sources.rows = PagedRows::decode_all(SOURCE_TABLE, SOURCES_PER_PAGE, r)?;
        c.places.rows = PagedRows::decode_all(PLACE_TABLE, PLACES_PER_PAGE, r)?;
        r.finish_check()?;
        c.check_indexes()?;
        let cultures = c.cultures.list.len();
        let mut feuds = 0u64;
        let mut bad_culture = None;
        c.factions.rows.for_each(|k, x| {
            feuds += x.feuds.len() as u64;
            if usize::from(x.x.culture) >= cultures {
                bad_culture = Some((k, x.x.culture));
            }
        });
        if let Some((k, cu)) = bad_culture {
            return bad(format!("faction {k}: culture {cu} out of range"));
        }
        if feuds != c.feud_count {
            return bad(format!("{feuds} feuds, the count says {}", c.feud_count));
        }
        let mut ids = BTreeSet::new();
        c.sources.rows.for_each(|k, _| {
            ids.insert(k as u32);
        });
        if c.sources.ids.values().any(|r| !ids.contains(&r.id)) {
            return bad("a source's row is missing".into());
        }
        Ok(c)
    }

    /// A hash of everything the engine holds (its save's bytes): equal hashes mean the same
    /// state, whoever read what when. A whole-engine pass: tests and tools only. Paged, it loads
    /// every page (kept until the next budget check).
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
    }

    // ---- paged (A16) ------------------------------------------------------------------------

    /// Starts paging the row tables to `source` under `budget` bytes (split between them,
    /// PM-C7). Every row stays resident until a save writes it ([`Conflict::save_paged`]).
    pub fn attach_store(&mut self, source: Arc<dyn PageSource>, budget: usize) {
        let (f, s, p) = split(budget);
        self.factions.rows.attach(source.clone(), f);
        self.sources.rows.attach(source.clone(), s);
        self.places.rows.attach(source.clone(), p);
        self.store = Some(source);
        self.budget_mark = None;
    }

    pub fn is_paged(&self) -> bool {
        self.store.is_some()
    }

    /// The row tables' budget, bytes.
    pub fn budget(&self) -> usize {
        self.factions
            .rows
            .budget()
            .saturating_add(self.sources.rows.budget())
            .saturating_add(self.places.rows.budget())
    }

    /// Whether the row tables are over their budget with nothing left to evict (at a step
    /// boundary, after [`Conflict::enforce_budget`]): every resident page changed since the last
    /// save. The host should save (an extra, pending save is a generation like any other, P29;
    /// PM7, PM-C10): the pages it writes can go.
    pub fn needs_save(&self) -> bool {
        self.is_paged() && self.resident_bytes() > self.budget()
    }

    pub fn set_budget(&mut self, budget: usize) {
        self.budget_mark = None;
        let (f, s, p) = split(budget);
        self.factions.rows.set_budget(f);
        self.sources.rows.set_budget(s);
        self.places.rows.set_budget(p);
    }

    /// Evicts least-recently-used row pages whose state is on disk until each table fits its
    /// share of the budget. A step boundary: [`Conflict::run_until`] calls it after every event;
    /// a caller that reads or changes rows outside `run_until` (records, emissions) calls it at
    /// its own step boundary. Unpaged it only remeasures. Returns how many pages went.
    pub fn enforce_budget(&mut self) -> usize {
        // After a check that ended over budget with every page dirty, only a load (a clean page)
        // or a save (pages unpinned) gives the next one something to evict: skip until then, so
        // a step does not walk every dirty page after every event (PM-C9).
        let loads = self.row_stats().loads;
        if self.budget_mark == Some(loads) {
            return 0;
        }
        let n = self.factions.rows.enforce_budget()
            + self.sources.rows.enforce_budget()
            + self.places.rows.enforce_budget();
        let over = self.factions.rows.resident_bytes() > self.factions.rows.budget()
            || self.sources.rows.resident_bytes() > self.sources.rows.budget()
            || self.places.rows.resident_bytes() > self.places.rows.budget();
        self.budget_mark = (self.store.is_some() && over).then_some(loads);
        n
    }

    /// The paged save: the meta page (everything resident, and each row table's count) under
    /// `meta_key`, and every row table's dirty pages, into `batch`. The pages stay pinned until
    /// the tickets come back through [`Conflict::saved`] (the commit is durable) or
    /// [`Conflict::save_failed`].
    pub fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> ConflictTickets {
        let mut w = Writer::with_capacity(4096);
        w.u32(META_MAGIC);
        w.u16(META_VERSION);
        self.put_resident(&mut w);
        w.var_u64(self.factions.rows.len());
        w.var_u64(self.sources.rows.len());
        w.var_u64(self.places.rows.len());
        batch.put(meta_key, w.as_bytes());
        ConflictTickets {
            factions: self.factions.rows.save_dirty(batch),
            sources: self.sources.rows.save_dirty(batch),
            places: self.places.rows.save_dirty(batch),
        }
    }

    pub fn saved(&mut self, t: ConflictTickets) {
        self.budget_mark = None;
        self.factions.rows.saved(t.factions);
        self.sources.rows.saved(t.sources);
        self.places.rows.saved(t.places);
    }

    pub fn save_failed(&mut self, t: ConflictTickets) {
        self.budget_mark = None;
        self.factions.rows.save_failed(t.factions);
        self.sources.rows.save_failed(t.sources);
        self.places.rows.save_failed(t.places);
    }

    /// Opens an engine a paged save wrote: reads the meta page and opens the row tables with
    /// nothing resident. `None` if the store has no meta page under `meta_key`.
    pub fn open_paged(
        source: Arc<dyn PageSource>,
        budget: usize,
        meta_key: PageKey,
        tun: ConflictTunables,
        cultures: Cultures,
    ) -> PageResult<Option<Conflict>> {
        let Some(bytes) = source.read(meta_key)? else {
            return Ok(None);
        };
        let fmt = |e: CodecError| PageError::Format(format!("the conflict's meta page: {e}"));
        let r = &mut Reader::new(&bytes);
        let head = (|| -> codec::Result<()> {
            if r.u32()? != META_MAGIC {
                return bad("not a conflict meta page".into());
            }
            let v = r.u16()?;
            if v != META_VERSION {
                return bad(format!(
                    "meta page version {v}, this build reads {META_VERSION}"
                ));
            }
            Ok(())
        })();
        head.map_err(fmt)?;
        let mut c = Self::get_resident(r, tun, cultures).map_err(fmt)?;
        let (nf, ns, np) = (|| -> codec::Result<(u64, u64, u64)> {
            Ok((r.var_u64()?, r.var_u64()?, r.var_u64()?))
        })()
        .map_err(fmt)?;
        r.finish_check().map_err(fmt)?;
        let (bf, bs, bp) = split(budget);
        c.factions.rows = PagedRows::open(FACTION_TABLE, FACTIONS_PER_PAGE, source.clone(), bf, nf);
        c.sources.rows = PagedRows::open(SOURCE_TABLE, SOURCES_PER_PAGE, source.clone(), bs, ns);
        c.places.rows = PagedRows::open(PLACE_TABLE, PLACES_PER_PAGE, source.clone(), bp, np);
        c.store = Some(source);
        c.check_indexes().map_err(fmt)?;
        Ok(Some(c))
    }

    /// The row tables' counters together (peaks summed: an upper bound on the joint peak).
    pub fn row_stats(&self) -> RowStats {
        let mut out = RowStats::default();
        for s in [
            self.factions.rows.stats(),
            self.sources.rows.stats(),
            self.places.rows.stats(),
        ] {
            out.loads += s.loads;
            out.load_bytes += s.load_bytes;
            out.evictions += s.evictions;
            out.over_budget += s.over_budget;
            out.peak_resident_bytes += s.peak_resident_bytes;
        }
        out
    }

    pub fn reset_row_stats(&mut self) {
        self.factions.rows.reset_stats();
        self.sources.rows.reset_stats();
        self.places.rows.reset_stats();
    }
}

trait FinishCheck {
    fn finish_check(&mut self) -> codec::Result<()>;
}

impl FinishCheck for Reader<'_> {
    fn finish_check(&mut self) -> codec::Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            bad(format!("{} bytes left over", self.remaining()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;

    #[test]
    fn a_truncated_or_foreign_save_is_an_error() {
        let (mut w, _) = synth::world(3);
        let mut c = synth::conflict(3, &w);
        c.add_faction(&mut w, synth::CAMP, "goblin", synth::CAMP_POS, Tick::ZERO);
        let b = c.save();
        for cut in [0, 4, b.len() / 2, b.len() - 1] {
            assert!(Conflict::load(c.tun.clone(), c.cultures.clone(), &b[..cut]).is_err());
        }
        let other = Cultures::from_json(&[crate::culture::GOBLIN_JSON]).unwrap();
        assert!(Conflict::load(c.tun.clone(), other, &b).is_err());
        let back = Conflict::load(c.tun.clone(), c.cultures.clone(), &b).unwrap();
        assert_eq!(back.state_hash(), c.state_hash());
        assert_eq!(back.save(), b);
    }
}
