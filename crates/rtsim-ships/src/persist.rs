// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships engine's save (the persist pattern of docs/overnight-choices-persist.md, S1 and S8):
//! every table, wheel and index it holds, private fields included, so an engine read back with
//! [`Ships::load`] continues exactly as the one saved: the same [`Ships::state_hash`] after the same
//! inputs, bit for bit.
//!
//! **The network is stored data** (docks, lanes with their paths, services with their timetables,
//! and every ship's service, slot and clock offset), never rebuilt from the water mask on load:
//! a restart never moves a ship (unlike Veloren's boot migration, research §4). Only indexes that
//! are pure functions of the stored tables are rebuilt: lanes by region, lanes' hops, docks by
//! node, and legs by voyage.
//!
//! **Not written:** the tunables (the caller's, as the dragons' are) and the CLI's log. The ships'
//! legs, reports and captains live in the phase 1 world (its leg index and knowledge), saved
//! beside them with `World::save`.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::rng::fnv1a64;
use rtsim_kernel::wheel::TimingWheel;
use rtsim_trade::GoodId;
use rtsim_travel::Medium;
use rtsim_travel::legs::{Meeting, Stamp};

use crate::engine::{
    Booked, Ev, Haven, LaneThreat, Navy, PState, Pirate, PlayerView, Promoted, Role, Ship,
    ShipLedger, ShipStats, Ships, Status,
};
use crate::fight::Observed;
use crate::geom::Polyline;
use crate::network::{Dock, Hop, Lane, NetworkLints};
use crate::timetable::{Leg, Service, ServiceKind};
use crate::tunables::ShipTunables;

const MAGIC: u32 = u32::from_le_bytes(*b"RTS1");

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

fn put_medium(w: &mut Writer, m: Medium) {
    w.u8(m as u8);
}

fn get_medium(r: &mut Reader<'_>) -> codec::Result<Medium> {
    let v = r.u8()?;
    Medium::from_u8(v).ok_or_else(|| CodecError::Invalid(format!("medium {v}")))
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

fn put_opt_stamp(w: &mut Writer, s: Option<Stamp>) {
    match s {
        Some(s) => {
            w.u8(1);
            s.encode(w);
        }
        None => w.u8(0),
    }
}

fn get_opt_stamp(r: &mut Reader<'_>) -> codec::Result<Option<Stamp>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(Stamp::decode(r)?),
        t => return bad(format!("option tag {t}")),
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

fn put_dock(w: &mut Writer, d: &Dock) {
    put_usize(w, d.node);
    put_medium(w, d.medium);
    w.u16(d.frame);
    w.var_i64(i64::from(d.x));
    w.var_i64(i64::from(d.y));
    w.var_i64(i64::from(d.z));
    w.var_u64(u64::from(d.berths));
}

fn get_dock(r: &mut Reader<'_>) -> codec::Result<Dock> {
    Ok(Dock {
        node: get_usize(r)?,
        medium: get_medium(r)?,
        frame: r.u16()?,
        x: get_i32(r)?,
        y: get_i32(r)?,
        z: get_i32(r)?,
        berths: get_u32(r)?,
    })
}

fn put_lane(w: &mut Writer, l: &Lane) {
    w.var_u64(u64::from(l.a));
    w.var_u64(u64::from(l.b));
    put_medium(w, l.medium);
    w.u16(l.frame);
    put_usize(w, l.path.pts.len());
    for p in &l.path.pts {
        w.var_i64(i64::from(p.0));
        w.var_i64(i64::from(p.1));
    }
    w.var_u64(u64::from(l.edge));
}

fn get_lane(r: &mut Reader<'_>) -> codec::Result<Lane> {
    let a = get_u32(r)?;
    let b = get_u32(r)?;
    let medium = get_medium(r)?;
    let frame = r.u16()?;
    let n = r.len_prefix(2)?;
    let mut pts = Vec::with_capacity(n);
    for _ in 0..n {
        pts.push((get_i32(r)?, get_i32(r)?));
    }
    Ok(Lane {
        a,
        b,
        medium,
        frame,
        path: Polyline::new(pts),
        edge: get_u32(r)?,
    })
}

fn put_service(w: &mut Writer, s: &Service) {
    w.u8(match s.kind {
        ServiceKind::Merchant => 0,
        ServiceKind::Patrol => 1,
    });
    put_medium(w, s.medium);
    w.u16(s.frame);
    w.var_u64(s.period);
    w.var_i64(i64::from(s.cruise_y));
    w.var_u64(u64::from(s.ships));
    match s.navy {
        Some(n) => {
            w.u8(1);
            w.var_u64(u64::from(n));
        }
        None => w.u8(0),
    }
    put_usize(w, s.legs.len());
    for g in &s.legs {
        w.var_u64(u64::from(g.hop.lane));
        w.bool(g.hop.forward);
        w.var_u64(u64::from(g.from_dock));
        w.var_u64(u64::from(g.to_dock));
        for v in [
            g.start,
            g.dock,
            g.depart,
            g.cruise,
            g.approach,
            g.depart_blocks,
            g.approach_blocks,
        ] {
            w.var_u64(v);
        }
        w.u8(g.platform);
    }
}

fn get_service(r: &mut Reader<'_>) -> codec::Result<Service> {
    let kind = match r.u8()? {
        0 => ServiceKind::Merchant,
        1 => ServiceKind::Patrol,
        t => return bad(format!("service kind {t}")),
    };
    let medium = get_medium(r)?;
    let frame = r.u16()?;
    let period = r.var_u64()?;
    if period == 0 {
        return bad("a service with no period".into());
    }
    let cruise_y = get_i32(r)?;
    let ships = get_u32(r)?;
    let navy = match r.u8()? {
        0 => None,
        1 => Some(get_u32(r)?),
        t => return bad(format!("option tag {t}")),
    };
    let n = r.len_prefix(10)?;
    let mut legs = Vec::with_capacity(n);
    for _ in 0..n {
        let lane = get_u32(r)?;
        let forward = r.bool()?;
        let from_dock = get_u32(r)?;
        let to_dock = get_u32(r)?;
        let mut v = [0u64; 7];
        for x in &mut v {
            *x = r.var_u64()?;
        }
        legs.push(Leg {
            hop: Hop { lane, forward },
            from_dock,
            to_dock,
            start: v[0],
            dock: v[1],
            depart: v[2],
            cruise: v[3],
            approach: v[4],
            depart_blocks: v[5],
            approach_blocks: v[6],
            platform: r.u8()?,
        });
    }
    if legs.is_empty() {
        return bad("a service with no legs".into());
    }
    Ok(Service {
        kind,
        medium,
        frame,
        legs,
        period,
        cruise_y,
        ships,
        navy,
    })
}

fn put_role(w: &mut Writer, r: Role) {
    match r {
        Role::Merchant => w.u8(0),
        Role::Pirate { haven } => {
            w.u8(1);
            w.var_u64(u64::from(haven));
        }
        Role::Navy { navy } => {
            w.u8(2);
            w.var_u64(u64::from(navy));
        }
    }
}

fn get_role(r: &mut Reader<'_>) -> codec::Result<Role> {
    Ok(match r.u8()? {
        0 => Role::Merchant,
        1 => Role::Pirate { haven: get_u32(r)? },
        2 => Role::Navy { navy: get_u32(r)? },
        t => return bad(format!("role {t}")),
    })
}

fn put_pstate(w: &mut Writer, s: PState) {
    match s {
        PState::Resting { until } => {
            w.u8(0);
            w.u64(until.0);
        }
        PState::Outbound { lane, arrive } => {
            w.u8(1);
            w.var_u64(u64::from(lane));
            w.u64(arrive.0);
        }
        PState::Hunting { lane, since, until } => {
            w.u8(2);
            w.var_u64(u64::from(lane));
            w.u64(since.0);
            w.u64(until.0);
        }
        PState::Homebound { arrive } => {
            w.u8(3);
            w.u64(arrive.0);
        }
    }
}

fn get_pstate(r: &mut Reader<'_>) -> codec::Result<PState> {
    Ok(match r.u8()? {
        0 => PState::Resting {
            until: Tick(r.u64()?),
        },
        1 => PState::Outbound {
            lane: get_u32(r)?,
            arrive: Tick(r.u64()?),
        },
        2 => PState::Hunting {
            lane: get_u32(r)?,
            since: Tick(r.u64()?),
            until: Tick(r.u64()?),
        },
        3 => PState::Homebound {
            arrive: Tick(r.u64()?),
        },
        t => return bad(format!("pirate state {t}")),
    })
}

fn put_ship(w: &mut Writer, x: &Ship) {
    put_role(w, x.role);
    put_medium(w, x.medium);
    w.u16(x.frame);
    w.u64(x.seed);
    match x.service {
        Some(s) => {
            w.u8(1);
            w.var_u64(u64::from(s));
        }
        None => w.u8(0),
    }
    w.var_u64(u64::from(x.slot));
    w.var_u64(x.offset);
    w.var_u64(u64::from(x.crew));
    w.var_u64(u64::from(x.hull_milli));
    match x.status {
        Status::Active => w.u8(0),
        Status::Lost { since, back } => {
            w.u8(1);
            w.u64(since.0);
            w.u64(back.0);
        }
    }
    w.u32(x.epoch);
    w.bool(x.embodied);
    put_goods(w, &x.cargo);
    put_goods(w, &x.quotes);
    match x.promoted {
        Some(p) => {
            w.u8(1);
            w.var_u64(p.voyage);
            w.var_u64(u64::from(p.edge));
            w.u8(p.dir);
            put_usize(w, p.from);
            put_usize(w, p.to);
            w.u64(p.arrive.0);
        }
        None => w.u8(0),
    }
    w.u32(x.plan);
    match &x.pirate {
        Some(p) => {
            w.u8(1);
            w.u64(p.captain.to_bits());
            put_pstate(w, p.state);
            put_opt_stamp(w, p.leg);
            put_goods(w, &p.district_loot);
        }
        None => w.u8(0),
    }
}

fn get_ship(r: &mut Reader<'_>) -> codec::Result<Ship> {
    let role = get_role(r)?;
    let medium = get_medium(r)?;
    let frame = r.u16()?;
    let seed = r.u64()?;
    let service = match r.u8()? {
        0 => None,
        1 => Some(get_u32(r)?),
        t => return bad(format!("option tag {t}")),
    };
    let slot = get_u32(r)?;
    let offset = r.var_u64()?;
    let crew = get_u32(r)?;
    let hull_milli = get_u32(r)?;
    let status = match r.u8()? {
        0 => Status::Active,
        1 => Status::Lost {
            since: Tick(r.u64()?),
            back: Tick(r.u64()?),
        },
        t => return bad(format!("status {t}")),
    };
    let epoch = r.u32()?;
    let embodied = r.bool()?;
    let cargo = get_goods(r)?;
    let quotes = get_goods(r)?;
    let promoted = match r.u8()? {
        0 => None,
        1 => Some(Promoted {
            voyage: r.var_u64()?,
            edge: get_u32(r)?,
            dir: r.u8()?,
            from: get_usize(r)?,
            to: get_usize(r)?,
            arrive: Tick(r.u64()?),
        }),
        t => return bad(format!("option tag {t}")),
    };
    let plan = r.u32()?;
    let pirate = match r.u8()? {
        0 => None,
        1 => Some(Pirate {
            captain: handle(r)?,
            state: get_pstate(r)?,
            leg: get_opt_stamp(r)?,
            district_loot: get_goods(r)?,
        }),
        t => return bad(format!("option tag {t}")),
    };
    Ok(Ship {
        role,
        medium,
        frame,
        seed,
        service,
        slot,
        offset,
        crew,
        hull_milli,
        status,
        epoch,
        embodied,
        cargo,
        quotes,
        promoted,
        plan,
        pirate,
    })
}

fn put_ev(w: &mut Writer, e: &Ev) {
    match *e {
        Ev::Seen {
            ship,
            voyage,
            player,
            meeting,
        } => {
            w.u8(0);
            w.u64(ship.to_bits());
            w.var_u64(voyage);
            w.u64(player.to_bits());
            put_meeting(w, &meeting);
        }
        Ev::SeenPirate {
            ship,
            plan,
            player,
            meeting,
        } => {
            w.u8(1);
            w.u64(ship.to_bits());
            w.u32(plan);
            w.u64(player.to_bits());
            put_meeting(w, &meeting);
        }
        Ev::Unbook { ship, voyage } => {
            w.u8(2);
            w.u64(ship.to_bits());
            w.var_u64(voyage);
        }
        Ev::NextLeg { ship, epoch } => {
            w.u8(3);
            w.u64(ship.to_bits());
            w.u32(epoch);
        }
        Ev::Arrive { ship, voyage } => {
            w.u8(4);
            w.u64(ship.to_bits());
            w.var_u64(voyage);
        }
        Ev::Raid {
            pirate,
            victim,
            voyage,
            plan,
            player,
        } => {
            w.u8(5);
            w.u64(pirate.to_bits());
            w.u64(victim.to_bits());
            w.var_u64(voyage);
            w.u32(plan);
            w.u64(player.to_bits());
        }
        Ev::Decide { ship, plan } => {
            w.u8(6);
            w.u64(ship.to_bits());
            w.u32(plan);
        }
        Ev::ReachLane { ship, plan } => {
            w.u8(7);
            w.u64(ship.to_bits());
            w.u32(plan);
        }
        Ev::Bored { ship, plan, reason } => {
            w.u8(8);
            w.u64(ship.to_bits());
            w.u32(plan);
            w.u32(reason);
        }
        Ev::Home { ship, plan } => {
            w.u8(9);
            w.u64(ship.to_bits());
            w.u32(plan);
        }
        Ev::Respawn { ship } => {
            w.u8(10);
            w.u64(ship.to_bits());
        }
        Ev::Rebuild { haven, medium } => {
            w.u8(11);
            w.var_u64(u64::from(haven));
            put_medium(w, medium);
        }
    }
}

fn get_ev(r: &mut Reader<'_>) -> codec::Result<Ev> {
    Ok(match r.u8()? {
        0 => Ev::Seen {
            ship: handle(r)?,
            voyage: r.var_u64()?,
            player: handle(r)?,
            meeting: get_meeting(r)?,
        },
        1 => Ev::SeenPirate {
            ship: handle(r)?,
            plan: r.u32()?,
            player: handle(r)?,
            meeting: get_meeting(r)?,
        },
        2 => Ev::Unbook {
            ship: handle(r)?,
            voyage: r.var_u64()?,
        },
        3 => Ev::NextLeg {
            ship: handle(r)?,
            epoch: r.u32()?,
        },
        4 => Ev::Arrive {
            ship: handle(r)?,
            voyage: r.var_u64()?,
        },
        5 => Ev::Raid {
            pirate: handle(r)?,
            victim: handle(r)?,
            voyage: r.var_u64()?,
            plan: r.u32()?,
            player: handle(r)?,
        },
        6 => Ev::Decide {
            ship: handle(r)?,
            plan: r.u32()?,
        },
        7 => Ev::ReachLane {
            ship: handle(r)?,
            plan: r.u32()?,
        },
        8 => Ev::Bored {
            ship: handle(r)?,
            plan: r.u32()?,
            reason: r.u32()?,
        },
        9 => Ev::Home {
            ship: handle(r)?,
            plan: r.u32()?,
        },
        10 => Ev::Respawn { ship: handle(r)? },
        11 => Ev::Rebuild {
            haven: get_u32(r)?,
            medium: get_medium(r)?,
        },
        t => return bad(format!("event tag {t}")),
    })
}

fn put_cmd(w: &mut Writer, c: &crate::boundary::Cmd) {
    let mut b = Vec::new();
    crate::boundary::encode_cmd(0, c, &mut b);
    w.bytes(&b);
}

fn get_cmd(r: &mut Reader<'_>) -> codec::Result<crate::boundary::Cmd> {
    let b = r.bytes()?;
    crate::boundary::decode_cmd(b)
        .map(|(_, _, c)| c)
        .map_err(|e| CodecError::Invalid(format!("a kept command: {e:?}")))
}

fn put_stats(w: &mut Writer, s: &ShipStats) {
    for v in [
        s.events,
        s.stale,
        s.interest_bookings,
        s.voyages_booked,
        s.legs_booked,
        s.seen,
        s.seen_stale,
        s.seen_pirates,
        s.materialized,
        s.promotions,
        s.landings,
        s.hunts,
        s.fruitless,
        s.hazard_changes,
        s.raids_observed,
        s.raids_abstract,
        s.fights_observed,
        s.captures,
        s.sinkings,
        s.loot_events,
        s.coast_raids,
        s.respawns,
        s.rebuilds,
        s.captains_died,
        s.bored,
    ] {
        w.var_u64(v);
    }
}

fn get_stats(r: &mut Reader<'_>) -> codec::Result<ShipStats> {
    let mut v = [0u64; 25];
    for x in &mut v {
        *x = r.var_u64()?;
    }
    Ok(ShipStats {
        events: v[0],
        stale: v[1],
        interest_bookings: v[2],
        voyages_booked: v[3],
        legs_booked: v[4],
        seen: v[5],
        seen_stale: v[6],
        seen_pirates: v[7],
        materialized: v[8],
        promotions: v[9],
        landings: v[10],
        hunts: v[11],
        fruitless: v[12],
        hazard_changes: v[13],
        raids_observed: v[14],
        raids_abstract: v[15],
        fights_observed: v[16],
        captures: v[17],
        sinkings: v[18],
        loot_events: v[19],
        coast_raids: v[20],
        respawns: v[21],
        rebuilds: v[22],
        captains_died: v[23],
        bored: v[24],
    })
}

fn put_ledger(w: &mut Writer, l: &ShipLedger) {
    for m in [
        &l.looted_by_pirates,
        &l.landed_at_havens,
        &l.landed_at_ports,
        &l.taken_by_players,
        &l.sunk,
        &l.from_districts,
        &l.districts_landed,
        &l.districts_lost,
    ] {
        put_big(w, m);
    }
    put_usize(w, l.districts_unmapped.len());
    for (k, v) in &l.districts_unmapped {
        w.bytes(k.as_bytes());
        w.var_i64(*v);
    }
}

fn get_ledger(r: &mut Reader<'_>) -> codec::Result<ShipLedger> {
    let mut l = ShipLedger {
        looted_by_pirates: get_big(r)?,
        landed_at_havens: get_big(r)?,
        landed_at_ports: get_big(r)?,
        taken_by_players: get_big(r)?,
        sunk: get_big(r)?,
        from_districts: get_big(r)?,
        districts_landed: get_big(r)?,
        districts_lost: get_big(r)?,
        districts_unmapped: BTreeMap::new(),
    };
    for _ in 0..r.len_prefix(2)? {
        let k = String::from_utf8(r.bytes()?.to_vec())
            .map_err(|_| CodecError::Invalid("a good name not UTF-8".into()))?;
        l.districts_unmapped.insert(k, r.var_i64()?);
    }
    Ok(l)
}

impl Ships {
    /// The whole engine as bytes (see the module docs for what is left out).
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.ships.len() * 64);
        w.u32(MAGIC);
        w.u64(self.seed);
        w.u64(self.now.0);
        w.bool(self.built);
        put_usize(&mut w, self.docks.len());
        self.docks.iter().for_each(|d| put_dock(&mut w, d));
        put_usize(&mut w, self.lanes.len());
        self.lanes.iter().for_each(|l| put_lane(&mut w, l));
        put_usize(&mut w, self.services.len());
        self.services.iter().for_each(|s| put_service(&mut w, s));
        self.ships.encode_with(&mut w, |x, w| put_ship(w, x));
        for v in &self.service_ships {
            put_usize(&mut w, v.len());
            v.iter().for_each(|s| w.u64(s.to_bits()));
        }
        put_usize(&mut w, self.havens.len());
        for h in &self.havens {
            put_usize(&mut w, h.node);
            w.u16(h.frame);
            for v in [h.dock.0, h.dock.1, h.dock.2] {
                w.var_i64(i64::from(v));
            }
            w.var_u64(u64::from(h.media));
            w.var_u64(u64::from(h.crew_pool));
            put_usize(&mut w, h.ships.len());
            h.ships.iter().for_each(|s| w.u64(s.to_bits()));
            put_usize(&mut w, h.lanes.len());
            h.lanes.iter().for_each(|l| w.var_u64(u64::from(*l)));
        }
        put_usize(&mut w, self.navies.len());
        for n in &self.navies {
            put_usize(&mut w, n.node);
            w.var_u64(u64::from(n.dock));
            put_medium(&mut w, n.medium);
            w.var_u64(u64::from(n.service));
            put_usize(&mut w, n.ships.len());
            n.ships.iter().for_each(|s| w.u64(s.to_bits()));
        }
        put_usize(&mut w, self.threats.len());
        for (l, t) in &self.threats {
            w.var_u64(u64::from(*l));
            match t.haven {
                Some(h) => {
                    w.u8(1);
                    w.var_u64(u64::from(h));
                }
                None => w.u8(0),
            }
            put_usize(&mut w, t.hunters.len());
            t.hunters.iter().for_each(|s| w.u64(s.to_bits()));
            w.var_i64(t.patrol_milli);
            put_usize(&mut w, t.escorts.len());
            for (k, v) in &t.escorts {
                w.u64(*k);
                w.var_i64(*v);
            }
            w.var_i64(t.share);
        }
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        put_usize(&mut w, self.players.len());
        for (a, p) in &self.players {
            w.u64(a.to_bits());
            w.u64(p.uuid.0);
            w.u64(p.uuid.1);
            put_opt_stamp(&mut w, p.interest);
        }
        put_usize(&mut w, self.booked.len());
        for ((s, v), b) in &self.booked {
            w.u64(s.to_bits());
            w.var_u64(*v);
            put_usize(&mut w, b.legs.len());
            b.legs.iter().for_each(|l| w.u64(l.to_bits()));
            w.u64(b.end.0);
        }
        put_usize(&mut w, self.seen_by.len());
        for (s, v, p) in &self.seen_by {
            w.u64(s.to_bits());
            w.var_u64(*v);
            w.u64(p.to_bits());
        }
        for v in &self.observed {
            put_usize(&mut w, v.len());
            for o in v {
                w.f64(o.strength);
                w.f64(o.survived);
            }
        }
        put_ledger(&mut w, &self.ledger);
        put_stats(&mut w, &self.stats);
        let l = &self.lints;
        for v in [l.too_long, l.no_water_path, l.too_roundabout, l.over_degree] {
            w.var_u64(u64::from(v));
        }
        put_usize(&mut w, l.docks_not_on_water.len());
        l.docks_not_on_water
            .iter()
            .for_each(|d| w.var_u64(u64::from(*d)));
        put_usize(&mut w, self.over_berths.len());
        for (d, k) in &self.over_berths {
            w.var_u64(u64::from(*d));
            w.var_u64(u64::from(*k));
        }
        put_usize(&mut w, self.outbox.len());
        self.outbox.iter().for_each(|c| put_cmd(&mut w, c));
        w.into_bytes()
    }

    /// Reads an engine [`Ships::save`] wrote, with the tunables it ran with.
    pub fn load(tun: ShipTunables, bytes: &[u8]) -> codec::Result<Ships> {
        let mut r = Reader::new(bytes);
        let r = &mut r;
        if r.u32()? != MAGIC {
            return bad("not a ships save".into());
        }
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let built = r.bool()?;
        let docks = (0..r.len_prefix(6)?)
            .map(|_| get_dock(r))
            .collect::<codec::Result<Vec<_>>>()?;
        let lanes = (0..r.len_prefix(6)?)
            .map(|_| get_lane(r))
            .collect::<codec::Result<Vec<_>>>()?;
        let services = (0..r.len_prefix(8)?)
            .map(|_| get_service(r))
            .collect::<codec::Result<Vec<_>>>()?;
        let ships: Arena<Ship> = Arena::decode_with(r, get_ship)?;
        let mut service_ships = Vec::with_capacity(services.len());
        for _ in 0..services.len() {
            let n = r.len_prefix(8)?;
            service_ships.push((0..n).map(|_| handle(r)).collect::<codec::Result<Vec<_>>>()?);
        }
        let mut havens = Vec::new();
        for _ in 0..r.len_prefix(6)? {
            let node = get_usize(r)?;
            let frame = r.u16()?;
            let dock = (get_i32(r)?, get_i32(r)?, get_i32(r)?);
            let media = get_u32(r)?;
            let crew_pool = get_u32(r)?;
            let n = r.len_prefix(8)?;
            let hs = (0..n).map(|_| handle(r)).collect::<codec::Result<Vec<_>>>()?;
            let n = r.len_prefix(1)?;
            let hl = (0..n).map(|_| get_u32(r)).collect::<codec::Result<Vec<_>>>()?;
            havens.push(Haven {
                node,
                frame,
                dock,
                media,
                crew_pool,
                ships: hs,
                lanes: hl,
            });
        }
        let mut navies = Vec::new();
        for _ in 0..r.len_prefix(4)? {
            let node = get_usize(r)?;
            let dock = get_u32(r)?;
            let medium = get_medium(r)?;
            let service = get_u32(r)?;
            let n = r.len_prefix(8)?;
            let ns = (0..n).map(|_| handle(r)).collect::<codec::Result<Vec<_>>>()?;
            navies.push(Navy {
                node,
                dock,
                medium,
                service,
                ships: ns,
            });
        }
        let mut threats = BTreeMap::new();
        for _ in 0..r.len_prefix(3)? {
            let l = get_u32(r)?;
            let haven = match r.u8()? {
                0 => None,
                1 => Some(get_u32(r)?),
                t => return bad(format!("option tag {t}")),
            };
            let n = r.len_prefix(8)?;
            let hunters = (0..n)
                .map(|_| handle(r))
                .collect::<codec::Result<BTreeSet<_>>>()?;
            let patrol_milli = r.var_i64()?;
            let mut escorts = BTreeMap::new();
            for _ in 0..r.len_prefix(9)? {
                escorts.insert(r.u64()?, r.var_i64()?);
            }
            let share = r.var_i64()?;
            threats.insert(
                l,
                LaneThreat {
                    haven,
                    hunters,
                    patrol_milli,
                    escorts,
                    share,
                },
            );
        }
        let wheel: TimingWheel<Ev> = TimingWheel::decode_with(r, get_ev)?;
        let mut players = BTreeMap::new();
        for _ in 0..r.len_prefix(25)? {
            let a = handle(r)?;
            let uuid = (r.u64()?, r.u64()?);
            let interest = get_opt_stamp(r)?;
            players.insert(a, PlayerView { uuid, interest });
        }
        let mut booked = BTreeMap::new();
        let mut leg_voyage = BTreeMap::new();
        for _ in 0..r.len_prefix(10)? {
            let s = handle(r)?;
            let v = r.var_u64()?;
            let n = r.len_prefix(8)?;
            let legs = (0..n).map(|_| handle(r)).collect::<codec::Result<Vec<_>>>()?;
            let end = Tick(r.u64()?);
            for &l in &legs {
                leg_voyage.insert(l, (s, v));
            }
            booked.insert((s, v), Booked { legs, end });
        }
        let mut seen_by = BTreeSet::new();
        for _ in 0..r.len_prefix(17)? {
            seen_by.insert((handle(r)?, r.var_u64()?, handle(r)?));
        }
        let mut observed: [Vec<Observed>; 2] = Default::default();
        for v in &mut observed {
            for _ in 0..r.len_prefix(16)? {
                v.push(Observed {
                    strength: r.f64()?,
                    survived: r.f64()?,
                });
            }
        }
        let ledger = get_ledger(r)?;
        let stats = get_stats(r)?;
        let mut lints = NetworkLints {
            too_long: get_u32(r)?,
            no_water_path: get_u32(r)?,
            too_roundabout: get_u32(r)?,
            over_degree: get_u32(r)?,
            docks_not_on_water: Vec::new(),
        };
        for _ in 0..r.len_prefix(1)? {
            lints.docks_not_on_water.push(get_u32(r)?);
        }
        let mut over_berths = Vec::new();
        for _ in 0..r.len_prefix(2)? {
            over_berths.push((get_u32(r)?, get_u32(r)?));
        }
        let mut outbox = Vec::new();
        for _ in 0..r.len_prefix(1)? {
            outbox.push(get_cmd(r)?);
        }
        if !r.is_empty() {
            return bad(format!("{} trailing bytes", r.remaining()));
        }
        let mut s = Ships::new(seed, tun, now);
        s.docks = docks;
        s.lanes = lanes;
        s.services = services;
        s.ships = ships;
        s.service_ships = service_ships;
        s.havens = havens;
        s.navies = navies;
        s.threats = threats;
        s.wheel = wheel;
        s.players = players;
        s.booked = booked;
        s.leg_voyage = leg_voyage;
        s.seen_by = seen_by;
        s.observed = observed;
        s.ledger = ledger;
        s.stats = stats;
        s.lints = lints;
        s.over_berths = over_berths;
        s.outbox = outbox;
        s.built = built;
        s.rebuild_indexes()?;
        Ok(s)
    }

    /// The indexes that are pure functions of the stored tables.
    fn rebuild_indexes(&mut self) -> codec::Result<()> {
        let nl = self.lanes.len();
        self.lane_hops = vec![Vec::new(); nl];
        self.lane_cells.clear();
        for l in 0..nl as u32 {
            self.index_lane(l);
        }
        for (s, sv) in self.services.iter().enumerate() {
            for (j, g) in sv.legs.iter().enumerate() {
                let Some(v) = self.lane_hops.get_mut(g.hop.lane as usize) else {
                    return bad(format!("service {s} sails lane {}", g.hop.lane));
                };
                v.push((s as u32, j as u32));
            }
        }
        self.dock_of.clear();
        for (i, d) in self.docks.iter().enumerate() {
            self.dock_of.insert((d.node, d.medium), i as u32);
        }
        for (_, x) in self.ships.iter() {
            if x.service.is_some_and(|s| s as usize >= self.services.len()) {
                return bad("a ship on a service that does not exist".into());
            }
        }
        Ok(())
    }

    /// A hash of everything the engine holds (its save's bytes): equal hashes mean the same
    /// state, whoever read what when.
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
    }
}
