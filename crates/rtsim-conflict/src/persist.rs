// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine's save (the persist pattern of docs/overnight-choices-persist.md, S1 and
//! S8): every table, wheel and index it holds, private fields included, so an engine read back with
//! [`Conflict::load`] continues exactly as the one saved: the same [`Conflict::state_hash`] after
//! the same inputs, bit for bit (grievance anchors are stored as they are, never recomputed).
//!
//! **Not written:** the tunables and the cultures (the caller's data; a fingerprint of the
//! cultures is saved and checked, so a save is never read with other data), and the log. The
//! parties' legs, the chiefs and heirs, and the faction actors live in the phase 1 world (its leg
//! index and knowledge), saved beside this with `World::save`. Indexes that are pure functions of
//! the stored tables (factions by key, factions by cell) are rebuilt.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_industry::ids::DistrictId;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::rng::fnv1a64;
use rtsim_kernel::wheel::TimingWheel;
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
use crate::tunables::ConflictTunables;

const MAGIC: u32 = u32::from_le_bytes(*b"RTC1");

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
    let mut v = vec![
        s.events,
        s.feud_wakes,
        s.stale,
        s.signals,
        s.emissions,
    ];
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
    s
}

impl Conflict {
    /// The whole engine as bytes (see the module docs for what is left out).
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.feuds.len() * 160);
        w.u32(MAGIC);
        w.u64(self.cultures.fingerprint());
        w.u64(self.seed);
        w.u64(self.now.0);
        put_usize(&mut w, self.factions.len());
        for f in &self.factions {
            put_faction(&mut w, f);
        }
        put_usize(&mut w, self.feuds.len());
        for ((f, a), x) in &self.feuds {
            w.var_u64(u64::from(*f));
            w.u64(a.to_bits());
            put_feud(&mut w, x);
        }
        put_usize(&mut w, self.sources.len());
        for ((k, kind), s) in &self.sources {
            w.u64(*k);
            w.u8(*kind as u8);
            put_pos(&mut w, s.pos);
            put_opt(&mut w, &s.district, |w, d| w.u64(d.0));
            w.f64(s.rate);
            put_usize(&mut w, s.affects.len());
            for (f, fall) in &s.affects {
                w.var_u64(u64::from(*f));
                w.f64(*fall);
            }
        }
        self.parties.encode_with(&mut w, |x, w| put_party(w, x));
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        put_usize(&mut w, self.players.len());
        for (a, u) in &self.players {
            w.u64(a.to_bits());
            w.u64(u.0);
            w.u64(u.1);
        }
        put_usize(&mut w, self.applied_ops.len());
        self.applied_ops.iter().for_each(|o| w.u64(*o));
        w.u64(self.settlers.to_bits());
        put_usize(&mut w, self.java_dict.len());
        for ((k, id), n) in &self.java_dict {
            w.u8(*k);
            w.var_u64(u64::from(*id));
            w.bytes(n.as_bytes());
        }
        put_ledger(&mut w, &self.ledger);
        let st = stats_fields(&self.stats);
        put_usize(&mut w, st.len());
        st.iter().for_each(|v| w.var_u64(*v));
        put_usize(&mut w, self.outbox.len());
        for c in &self.outbox {
            let mut b = Vec::new();
            boundary::encode_cmd(0, c, &mut b);
            w.bytes(&b);
        }
        w.into_bytes()
    }

    /// Reads an engine [`Conflict::save`] wrote, with the tunables and cultures it ran with.
    pub fn load(tun: ConflictTunables, cultures: Cultures, bytes: &[u8]) -> codec::Result<Conflict> {
        let r = &mut Reader::new(bytes);
        if r.u32()? != MAGIC {
            return bad("not a conflict save".into());
        }
        if r.u64()? != cultures.fingerprint() {
            return bad("the save was made with other culture data".into());
        }
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let mut c = Conflict::new(seed, tun, cultures, now);
        let n = r.len_prefix(32)?;
        for _ in 0..n {
            let f = get_faction(r)?;
            if usize::from(f.culture) >= c.cultures.list.len() {
                return bad(format!("culture {} out of range", f.culture));
            }
            c.factions.push(f);
        }
        for (i, f) in c.factions.iter().enumerate() {
            c.by_key.insert(f.key, i as u32);
            let cell = (
                f.pos.frame,
                f.pos.x.div_euclid(c.tun.cell_blocks),
                f.pos.z.div_euclid(c.tun.cell_blocks),
            );
            c.cells.entry(cell).or_default().push(i as u32);
        }
        let n = r.len_prefix(8)?;
        for _ in 0..n {
            let f = get_u32(r)?;
            if f as usize >= c.factions.len() {
                return bad(format!("feud of faction {f}"));
            }
            let a = handle(r)?;
            let x = get_feud(r)?;
            c.feuds.insert((f, a), x);
        }
        let n = r.len_prefix(8)?;
        for _ in 0..n {
            let k = r.u64()?;
            let kind = get_kind(r)?;
            let pos = get_pos(r)?;
            let district = get_opt(r, |r| Ok(DistrictId(r.u64()?)))?;
            let rate = r.f64()?;
            let m = r.len_prefix(9)?;
            let affects = (0..m)
                .map(|_| Ok((get_u32(r)?, r.f64()?)))
                .collect::<codec::Result<Vec<_>>>()?;
            c.sources.insert(
                (k, kind),
                Source {
                    pos,
                    district,
                    rate,
                    affects,
                },
            );
        }
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
        let st = (0..n).map(|_| r.var_u64()).collect::<codec::Result<Vec<_>>>()?;
        c.stats = stats_from(&st);
        let n = r.len_prefix(1)?;
        for _ in 0..n {
            let b = r.bytes()?;
            let (_, cmd, _) = boundary::decode_cmd(b)
                .map_err(|e| CodecError::Invalid(format!("a queued command: {e:?}")))?;
            c.outbox.push(cmd);
        }
        r.finish_check()?;
        Ok(c)
    }

    /// A hash of everything the engine holds (its save's bytes): equal hashes mean the same
    /// state, whoever read what when.
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
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
