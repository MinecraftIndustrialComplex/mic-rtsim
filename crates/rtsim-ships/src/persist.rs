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
//!
//! **Havens** carry their crew pool (thousandths, as of a tick) and their pressure, and a lost
//! pirate's refit event its plan (EQUILIBRIUM.md §2.4.6, slice S8, 2026-10-22), in the same `RTS2`
//! format: no saved world holds a haven or a pirate yet (no plugin sends HAVEN_FOUND), and saves
//! from before a release are not migrated. Slice S8b (§2.4.6a, 2026-10-22) adds to each haven row
//! the tick it starves from, whether it is shut (since, when its site looks again, broken), the
//! stamp of its one live look and its lifecycle counters, and the look itself (`Ev::Refound`,
//! tag 12) to the wheel: still `RTS2`, for the same reason.
//!
//! # Two forms (docs/choices-paging-modules.md, PM-S rows)
//!
//! - **One blob** ([`Ships::save`], [`Ships::load`]): the row tables written whole, rows in key
//!   order ([`PagedArena::encode_all`]), then the resident state. What the resident tests and
//!   tools save, and what [`Ships::state_hash`] hashes: equal for equal engines, paged or not
//!   (rtsim-jni pages the ships, from page 0x5300/0).
//! - **Paged** ([`Ships::save_paged`], [`Ships::open_paged`]): a meta page with the resident state
//!   and each row table's layout, count and next index, beside the row tables' dirty pages (ships
//!   0x5310, lane threats 0x5311). The port network (docks, lanes, services, the flow graph that
//!   stays resident and grows with the world) is resident state too, but it changes only when a
//!   port, a haven's network or a navy joins, so it has its own page ([`NETWORK_KEY`], 0x5312),
//!   written by the saves after it changed and not by the others: a save's bytes are what changed
//!   plus a meta page that does not grow with the network. Opening reads the meta and network
//!   pages and leaves every row page on disk until something reads it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::page::{PageError, PageKey, SaveBatch};
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::rng::fnv1a64;
use rtsim_kernel::rows::{PageSource, PagedArena, PagedRows, Row, RowStats};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_trade::GoodId;
use rtsim_travel::Medium;
use rtsim_travel::legs::{Meeting, Stamp};

use crate::engine::{
    Booked, Ev, Haven, HavenCounts, LaneThreat, NETWORK_KEY, Navy, PState, Pirate, PlayerView,
    Promoted, Role, SHIP_ROWS_PER_PAGE, SHIP_TABLE, Ship, ShipLedger, ShipRange, ShipStats,
    ShipTickets, Ships, Shut, Status, THREAT_ROWS_PER_PAGE, THREAT_TABLE,
};
use crate::fight::Observed;
use crate::geom::Polyline;
use crate::network::{Dock, Hop, Lane, NetworkLints};
use crate::timetable::{Leg, Service, ServiceKind};
use crate::tunables::ShipTunables;

/// RTS2 since the rows moved to paged tables (RTS1 wrote an `Arena` and lists of ships per
/// service). No reader of RTS1 is kept: nothing is released (no backward compatibility before a
/// release).
const MAGIC: u32 = u32::from_le_bytes(*b"RTS2");
/// The row tables follow whole, in key order ([`Ships::save`]).
const ROWS_INLINE: u8 = 0;
/// The row tables are in their own pages; only their layouts and counts follow
/// ([`Ships::save_paged`]).
const ROWS_PAGED: u8 = 1;
/// The network's own page ([`NETWORK_KEY`]).
const NETWORK: u8 = 2;

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

/// Heap bytes of a small B-tree map of 10-byte entries: a leaf node of eleven slots (128 bytes)
/// per eleven entries. An estimate for the row budget.
fn small_map_bytes(n: usize) -> usize {
    n.div_ceil(11) * 128
}

impl Encode for Ship {
    fn encode(&self, w: &mut Writer) {
        put_ship(w, self);
    }
}

impl Decode for Ship {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        get_ship(r)
    }
}

impl Row for Ship {
    fn heap_bytes(&self) -> usize {
        small_map_bytes(self.cargo.len())
            + small_map_bytes(self.quotes.len())
            // The pirate is inline (counted in the row's size); only its loot map is heap.
            + self
                .pirate
                .as_ref()
                .map_or(0, |p| small_map_bytes(p.district_loot.len()))
    }
}

impl Encode for LaneThreat {
    fn encode(&self, w: &mut Writer) {
        match self.haven {
            Some(h) => {
                w.u8(1);
                w.var_u64(u64::from(h));
            }
            None => w.u8(0),
        }
        put_usize(w, self.hunters.len());
        self.hunters.iter().for_each(|s| w.u64(s.to_bits()));
        w.var_i64(self.patrol_milli);
        put_usize(w, self.escorts.len());
        for (k, v) in &self.escorts {
            w.u64(*k);
            w.var_i64(*v);
        }
        w.var_i64(self.share);
    }
}

impl Decode for LaneThreat {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let haven = match r.u8()? {
            0 => None,
            1 => Some(get_u32(r)?),
            t => return bad(format!("option tag {t}")),
        };
        let n = r.len_prefix(8)?;
        let hunters = (0..n)
            .map(|_| handle(r))
            .collect::<codec::Result<Vec<_>>>()?;
        if !hunters.is_sorted_by(|a, b| a < b) {
            return bad("a lane's hunters out of order".into());
        }
        let patrol_milli = r.var_i64()?;
        let n = r.len_prefix(9)?;
        let mut escorts: Vec<(u64, i64)> = Vec::with_capacity(n);
        for _ in 0..n {
            escorts.push((r.u64()?, r.var_i64()?));
        }
        if !escorts.is_sorted_by(|a, b| a.0 < b.0) {
            return bad("a lane's escorts out of order".into());
        }
        let share = r.var_i64()?;
        Ok(LaneThreat {
            haven,
            hunters,
            patrol_milli,
            escorts,
            share,
        })
    }
}

impl Row for LaneThreat {
    fn heap_bytes(&self) -> usize {
        self.hunters.capacity() * 8 + self.escorts.capacity() * 16
    }
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
        Ev::Refit { ship, plan } => {
            w.u8(11);
            w.u64(ship.to_bits());
            w.u32(plan);
        }
        Ev::Refound { haven, epoch } => {
            w.u8(12);
            w.u32(haven);
            w.u32(epoch);
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
        11 => Ev::Refit {
            ship: handle(r)?,
            plan: r.u32()?,
        },
        12 => Ev::Refound {
            haven: r.u32()?,
            epoch: r.u32()?,
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
        s.port_raids,
        s.items_from_ports,
        s.port_raid_lost_milli,
    ] {
        w.var_u64(v);
    }
}

fn get_stats(r: &mut Reader<'_>) -> codec::Result<ShipStats> {
    let mut v = [0u64; 28];
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
        port_raids: v[25],
        items_from_ports: v[26],
        port_raid_lost_milli: v[27],
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
        &l.looted_from_ports,
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
        looted_from_ports: get_big(r)?,
        districts_unmapped: BTreeMap::new(),
    };
    for _ in 0..r.len_prefix(2)? {
        let k = String::from_utf8(r.bytes()?.to_vec())
            .map_err(|_| CodecError::Invalid("a good name not UTF-8".into()))?;
        l.districts_unmapped.insert(k, r.var_i64()?);
    }
    Ok(l)
}

fn put_ranges(w: &mut Writer, v: &[ShipRange]) {
    put_usize(w, v.len());
    for r in v {
        w.var_u64(u64::from(r.first));
        w.var_u64(u64::from(r.n));
    }
}

fn get_ranges(r: &mut Reader<'_>, services: usize) -> codec::Result<Vec<ShipRange>> {
    let n = r.len_prefix(2)?;
    if n != services {
        return bad(format!("{n} ship ranges for {services} services"));
    }
    (0..n)
        .map(|_| {
            Ok(ShipRange {
                first: get_u32(r)?,
                n: get_u32(r)?,
            })
        })
        .collect()
}

fn page_format(what: &str) -> impl Fn(CodecError) -> PageError + '_ {
    move |e| PageError::Format(format!("{what}: {e}"))
}

/// How [`Ships::attach_store`] and [`Ships::open_paged`] split one budget between the row
/// tables: the lane threats get an eighth (a threat row is a few dozen bytes, and only hunted,
/// patrolled or escorted lanes have one), the ships the rest.
fn split_budget(budget: usize) -> (usize, usize) {
    let threats = budget / 8;
    (budget - threats, threats)
}

impl Ships {
    /// The whole engine as bytes, the row tables written whole in key order (see the module
    /// docs for what is left out). Works paged or not, with the same bytes; paged, it loads every
    /// row page through `&self` (a whole-table pass: saves of the unpaged form, `state_hash`,
    /// tools and tests).
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.ships.len() * 64);
        w.u32(MAGIC);
        w.u8(ROWS_INLINE);
        self.ships.encode_all(&mut w);
        self.threats.encode_all(&mut w);
        self.put_network(&mut w);
        self.put_resident(&mut w);
        w.into_bytes()
    }

    /// The network's page of a paged save.
    fn save_network(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.lanes.len() * 64);
        w.u32(MAGIC);
        w.u8(NETWORK);
        self.put_network(&mut w);
        w.into_bytes()
    }

    /// The meta page of a paged save: the rows' layouts, counts and next index, and everything
    /// resident.
    fn save_meta(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096);
        w.u32(MAGIC);
        w.u8(ROWS_PAGED);
        let (len, next) = self.ships.meta();
        w.var_u64(self.ships.rows().rows_per_page());
        w.var_u64(len);
        w.u32(next);
        w.var_u64(self.threats.rows_per_page());
        w.var_u64(self.threats.len());
        self.put_resident(&mut w);
        w.into_bytes()
    }

    /// Everything but the row tables.
    /// The port network: docks, lanes, services with their ships' key ranges, and the lints and
    /// berth counts its building found.
    fn put_network(&self, w: &mut Writer) {
        put_usize(w, self.docks.len());
        self.docks.iter().for_each(|d| put_dock(w, d));
        put_usize(w, self.lanes.len());
        self.lanes.iter().for_each(|l| put_lane(w, l));
        put_usize(w, self.services.len());
        self.services.iter().for_each(|s| put_service(w, s));
        put_ranges(w, &self.service_ships);
        let l = &self.lints;
        for v in [l.too_long, l.no_water_path, l.too_roundabout, l.over_degree] {
            w.var_u64(u64::from(v));
        }
        put_usize(w, l.docks_not_on_water.len());
        l.docks_not_on_water
            .iter()
            .for_each(|d| w.var_u64(u64::from(*d)));
        put_usize(w, self.over_berths.len());
        for (d, k) in &self.over_berths {
            w.var_u64(u64::from(*d));
            w.var_u64(u64::from(*k));
        }
    }

    /// What [`Ships::put_network`] wrote, into `self`.
    fn get_network(&mut self, r: &mut Reader<'_>) -> codec::Result<()> {
        self.docks = (0..r.len_prefix(6)?)
            .map(|_| get_dock(r))
            .collect::<codec::Result<Vec<_>>>()?;
        self.lanes = (0..r.len_prefix(6)?)
            .map(|_| get_lane(r))
            .collect::<codec::Result<Vec<_>>>()?;
        self.services = (0..r.len_prefix(8)?)
            .map(|_| get_service(r))
            .collect::<codec::Result<Vec<_>>>()?;
        self.service_ships = get_ranges(r, self.services.len())?;
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
        self.lints = lints;
        let mut over_berths = Vec::new();
        for _ in 0..r.len_prefix(2)? {
            over_berths.push((get_u32(r)?, get_u32(r)?));
        }
        self.over_berths = over_berths;
        Ok(())
    }

    /// Everything resident but the network and the row tables.
    fn put_resident(&self, w: &mut Writer) {
        let w = &mut *w;
        w.u64(self.seed);
        w.u64(self.now.0);
        w.bool(self.built);
        put_usize(w, self.havens.len());
        for h in &self.havens {
            put_usize(w, h.node);
            w.u16(h.frame);
            for v in [h.dock.0, h.dock.1, h.dock.2] {
                w.var_i64(i64::from(v));
            }
            w.var_u64(u64::from(h.media));
            w.var_u64(u64::from(h.pool_milli));
            w.u64(h.pool_at.0);
            let (p, at) = h.pressure.parts();
            w.f32(p);
            w.u32(at);
            w.u64(h.fed_until.0);
            match h.shut {
                None => w.u8(0),
                Some(sh) => {
                    w.u8(1);
                    w.u64(sh.since.0);
                    w.u64(sh.back.0);
                    w.bool(sh.broken);
                }
            }
            w.u32(h.epoch);
            let c = h.counts;
            for v in [c.toll_milli, c.laid_up, c.shuts, c.refounds] {
                w.var_u64(v);
            }
            put_usize(w, h.ships.len());
            h.ships.iter().for_each(|s| w.u64(s.to_bits()));
            put_usize(w, h.lanes.len());
            h.lanes.iter().for_each(|l| w.var_u64(u64::from(*l)));
        }
        put_usize(w, self.navies.len());
        for n in &self.navies {
            put_usize(w, n.node);
            w.var_u64(u64::from(n.dock));
            put_medium(w, n.medium);
            w.var_u64(u64::from(n.service));
            put_usize(w, n.ships.len());
            n.ships.iter().for_each(|s| w.u64(s.to_bits()));
        }
        self.wheel.encode_with(w, |e, w| put_ev(w, e));
        put_usize(w, self.players.len());
        for (a, p) in &self.players {
            w.u64(a.to_bits());
            w.u64(p.uuid.0);
            w.u64(p.uuid.1);
            put_opt_stamp(w, p.interest);
        }
        put_usize(w, self.booked.len());
        for ((s, v), b) in &self.booked {
            w.u64(s.to_bits());
            w.var_u64(*v);
            put_usize(w, b.legs.len());
            b.legs.iter().for_each(|l| w.u64(l.to_bits()));
            w.u64(b.end.0);
        }
        put_usize(w, self.applied_ops.len());
        self.applied_ops.iter().for_each(|o| w.u64(*o));
        put_usize(w, self.seen_by.len());
        for (s, v, p) in &self.seen_by {
            w.u64(s.to_bits());
            w.var_u64(*v);
            w.u64(p.to_bits());
        }
        put_usize(w, self.embodied.len());
        self.embodied.iter().for_each(|s| w.u64(s.to_bits()));
        for v in &self.observed {
            put_usize(w, v.len());
            for o in v {
                w.f64(o.strength);
                w.f64(o.survived);
            }
        }
        put_ledger(w, &self.ledger);
        put_stats(w, &self.stats);
        put_usize(w, self.outbox.len());
        self.outbox.iter().for_each(|c| put_cmd(w, c));
    }

    /// Reads an engine [`Ships::save`] wrote, with the tunables it ran with. Its row tables are
    /// resident until [`Ships::attach_store`].
    pub fn load(tun: ShipTunables, bytes: &[u8]) -> codec::Result<Ships> {
        let mut r = Reader::new(bytes);
        let r = &mut r;
        if r.u32()? != MAGIC {
            return bad("not a ships save".into());
        }
        if r.u8()? != ROWS_INLINE {
            return bad("a ships meta page, not a whole save".into());
        }
        let ships: PagedArena<Ship> = PagedArena::decode_all(SHIP_TABLE, SHIP_ROWS_PER_PAGE, r)?;
        let threats: PagedRows<LaneThreat> =
            PagedRows::decode_all(THREAT_TABLE, THREAT_ROWS_PER_PAGE, r)?;
        let mut net = Ships::new(0, tun.clone(), Tick::ZERO);
        net.get_network(r)?;
        let mut s = Self::get_resident(r, tun)?;
        s.take_network(net);
        s.ships = ships;
        s.threats = threats;
        s.rebuild_indexes()?;
        // The whole table is in memory already: check every row's service.
        for (_, x) in s.ships.iter() {
            if x.service.is_some_and(|v| v as usize >= s.services.len()) {
                return bad("a ship on a service that does not exist".into());
            }
        }
        Ok(s)
    }

    /// The resident state [`Ships::put_resident`] wrote, with an empty network and empty row
    /// tables.
    fn get_resident(r: &mut Reader<'_>, tun: ShipTunables) -> codec::Result<Ships> {
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let built = r.bool()?;
        let mut havens = Vec::new();
        for _ in 0..r.len_prefix(6)? {
            let node = get_usize(r)?;
            let frame = r.u16()?;
            let dock = (get_i32(r)?, get_i32(r)?, get_i32(r)?);
            let media = get_u32(r)?;
            let pool_milli = get_u32(r)?;
            let pool_at = Tick(r.u64()?);
            let (p, at) = (r.f32()?, r.u32()?);
            let fed_until = Tick(r.u64()?);
            let shut = match r.u8()? {
                0 => None,
                1 => Some(Shut {
                    since: Tick(r.u64()?),
                    back: Tick(r.u64()?),
                    broken: r.bool()?,
                }),
                t => return bad(format!("haven shut tag {t}")),
            };
            let epoch = r.u32()?;
            let counts = HavenCounts {
                toll_milli: r.var_u64()?,
                laid_up: r.var_u64()?,
                shuts: r.var_u64()?,
                refounds: r.var_u64()?,
            };
            let n = r.len_prefix(8)?;
            let hs = (0..n)
                .map(|_| handle(r))
                .collect::<codec::Result<Vec<_>>>()?;
            let n = r.len_prefix(1)?;
            let hl = (0..n)
                .map(|_| get_u32(r))
                .collect::<codec::Result<Vec<_>>>()?;
            havens.push(Haven {
                node,
                frame,
                dock,
                media,
                pool_milli,
                pool_at,
                pressure: Pressure::from_parts(p, at),
                ships: hs,
                lanes: hl,
                fed_until,
                shut,
                epoch,
                counts,
            });
        }
        let mut navies = Vec::new();
        for _ in 0..r.len_prefix(4)? {
            let node = get_usize(r)?;
            let dock = get_u32(r)?;
            let medium = get_medium(r)?;
            let service = get_u32(r)?;
            let n = r.len_prefix(8)?;
            let ns = (0..n)
                .map(|_| handle(r))
                .collect::<codec::Result<Vec<_>>>()?;
            navies.push(Navy {
                node,
                dock,
                medium,
                service,
                ships: ns,
            });
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
            let legs = (0..n)
                .map(|_| handle(r))
                .collect::<codec::Result<Vec<_>>>()?;
            let end = Tick(r.u64()?);
            for &l in &legs {
                leg_voyage.insert(l, (s, v));
            }
            booked.insert((s, v), Booked { legs, end });
        }
        let mut applied_ops = std::collections::VecDeque::new();
        for _ in 0..r.len_prefix(8)? {
            applied_ops.push_back(r.u64()?);
        }
        let mut seen_by = BTreeSet::new();
        for _ in 0..r.len_prefix(17)? {
            seen_by.insert((handle(r)?, r.var_u64()?, handle(r)?));
        }
        let mut embodied = BTreeSet::new();
        for _ in 0..r.len_prefix(8)? {
            embodied.insert(handle(r)?);
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
        let mut outbox = Vec::new();
        for _ in 0..r.len_prefix(1)? {
            outbox.push(get_cmd(r)?);
        }
        if !r.is_empty() {
            return bad(format!("{} trailing bytes", r.remaining()));
        }
        let mut s = Ships::new(seed, tun, now);
        s.havens = havens;
        s.navies = navies;
        s.wheel = wheel;
        s.players = players;
        s.booked = booked;
        s.leg_voyage = leg_voyage;
        s.seen_by = seen_by;
        s.embodied = embodied;
        s.applied_set = applied_ops.iter().copied().collect();
        s.applied_ops = applied_ops;
        s.observed = observed;
        s.ledger = ledger;
        s.stats = stats;
        s.outbox = outbox;
        s.built = built;
        Ok(s)
    }

    /// Moves `from`'s network into this engine.
    fn take_network(&mut self, from: Ships) {
        self.docks = from.docks;
        self.lanes = from.lanes;
        self.services = from.services;
        self.service_ships = from.service_ships;
        self.lints = from.lints;
        self.over_berths = from.over_berths;
    }

    /// The indexes that are pure functions of the resident tables (the network), and the
    /// services' key ranges checked against the ship table's next index. Never a pass over the
    /// rows.
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
        let next = u64::from(self.ships.meta().1);
        for (s, r) in self.service_ships.iter().enumerate() {
            if u64::from(r.first) + u64::from(r.n) > next {
                return bad(format!(
                    "service {s}'s ships {r:?} past the table's end {next}"
                ));
            }
        }
        Ok(())
    }

    /// A hash of everything the engine holds (its save's bytes): equal hashes mean the same
    /// state, whoever read what when, paged or not. A whole-table pass (tests and tools).
    pub fn state_hash(&self) -> u64 {
        fnv1a64(&self.save())
    }

    // ---- paging (A16; docs/choices-paging-modules.md) -------------------------------------------

    /// Starts paging the row tables to `source` (the page store the saves commit to) under
    /// `budget` resident bytes, split between the ships and the lane threats. Their pages stay
    /// until a [`Ships::save_paged`] writes them and its commit is durable.
    pub fn attach_store(&mut self, source: Arc<dyn PageSource>, budget: usize) {
        let (s, t) = split_budget(budget);
        self.ships.rows_mut().attach(source.clone(), s);
        self.threats.attach(source, t);
        self.network_dirty = true;
    }

    pub fn is_paged(&self) -> bool {
        self.ships.rows().is_paged()
    }

    /// The row tables' budget (both together).
    pub fn set_budget(&mut self, budget: usize) {
        let (s, t) = split_budget(budget);
        self.ships.rows_mut().set_budget(s);
        self.threats.set_budget(t);
    }

    /// The row pages per table: for tests and the bench, before paging starts (a resident table
    /// is rewritten under the new layout). Saved in the meta page, so [`Ships::open_paged`]
    /// reads the same layout back.
    pub fn set_page_layout(&mut self, ships_per_page: u64, threats_per_page: u64) {
        assert!(
            !self.is_paged(),
            "the page layout is set before paging starts"
        );
        let mut w = Writer::new();
        self.ships.encode_all(&mut w);
        self.threats.encode_all(&mut w);
        let b = w.into_bytes();
        let mut r = Reader::new(&b);
        self.ships = PagedArena::decode_all(SHIP_TABLE, ships_per_page, &mut r).expect("ours");
        self.threats = PagedRows::decode_all(THREAT_TABLE, threats_per_page, &mut r).expect("ours");
    }

    /// One paged save: the meta page at `meta_key` (everything resident but the network, and
    /// each row table's layout, count and next index), the network's page ([`NETWORK_KEY`]) if
    /// the network changed since it was last written, and every row page changed since it was
    /// last written, into `batch`. Hand the tickets back with [`Ships::saved`] once the commit is
    /// durable, or [`Ships::save_failed`]. `meta_key` must not be in the ships' own tables
    /// (0x5310 to 0x5312).
    pub fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> ShipTickets {
        assert!(
            ![SHIP_TABLE, THREAT_TABLE, NETWORK_KEY.table()].contains(&meta_key.table()),
            "the ships' meta page in one of their own tables"
        );
        batch.put(meta_key, &self.save_meta());
        let network = std::mem::take(&mut self.network_dirty);
        if network {
            batch.put(NETWORK_KEY, &self.save_network());
        }
        ShipTickets {
            ships: self.ships.rows_mut().save_dirty(batch),
            threats: self.threats.save_dirty(batch),
            network,
        }
    }

    /// The commit holding these tickets' pages is durable: they may be evicted.
    pub fn saved(&mut self, t: ShipTickets) {
        self.ships.rows_mut().saved(t.ships);
        self.threats.saved(t.threats);
    }

    /// The commit failed: the tickets' pages are dirty again.
    pub fn save_failed(&mut self, t: ShipTickets) {
        self.ships.rows_mut().save_failed(t.ships);
        self.threats.save_failed(t.threats);
        self.network_dirty |= t.network;
    }

    /// An engine saved by [`Ships::save_paged`] at `meta_key`: the meta page is read, the row
    /// tables open with nothing resident under `budget`, and continue exactly as the saved one.
    pub fn open_paged(
        source: Arc<dyn PageSource>,
        budget: usize,
        meta_key: PageKey,
        tun: ShipTunables,
    ) -> rtsim_kernel::page::Result<Ships> {
        let raw = source
            .read(meta_key)?
            .ok_or_else(|| PageError::Format("no ships meta page".into()))?;
        let fmt = page_format("the ships' meta page");
        let mut r = Reader::new(&raw);
        let r = &mut r;
        if r.u32().map_err(&fmt)? != MAGIC {
            return Err(PageError::Format("not a ships meta page".into()));
        }
        if r.u8().map_err(&fmt)? != ROWS_PAGED {
            return Err(PageError::Format(
                "a whole ships save, not a meta page".into(),
            ));
        }
        let per = |r: &mut Reader<'_>| -> codec::Result<u64> {
            let v = r.var_u64()?;
            if v == 0 || v > 1 << 32 {
                return bad(format!("{v} rows per page"));
            }
            Ok(v)
        };
        let ship_per = per(r).map_err(&fmt)?;
        let ship_len = r.var_u64().map_err(&fmt)?;
        let ship_next = r.u32().map_err(&fmt)?;
        let threat_per = per(r).map_err(&fmt)?;
        let threat_len = r.var_u64().map_err(&fmt)?;
        let mut s = Self::get_resident(r, tun.clone()).map_err(&fmt)?;
        let raw = source
            .read(NETWORK_KEY)?
            .ok_or_else(|| PageError::Format("no ships network page".into()))?;
        let fmt = page_format("the ships' network page");
        let mut r = Reader::new(&raw);
        if r.u32().map_err(&fmt)? != MAGIC || r.u8().map_err(&fmt)? != NETWORK {
            return Err(PageError::Format("not a ships network page".into()));
        }
        s.get_network(&mut r).map_err(&fmt)?;
        r.finish().map_err(&fmt)?;
        let (sb, tb) = split_budget(budget);
        s.ships = PagedArena::open(
            SHIP_TABLE,
            ship_per,
            source.clone(),
            sb,
            (ship_len, ship_next),
        );
        s.threats = PagedRows::open(THREAT_TABLE, threat_per, source, tb, threat_len);
        s.rebuild_indexes().map_err(&fmt)?;
        Ok(s)
    }

    /// Evicts row pages whose state is on disk until the tables fit their budget. Called at the
    /// end of every [`Ships::run_until`] (a step boundary); a caller that reads or ingests a lot
    /// between runs may call it too. Returns the pages evicted.
    pub fn enforce_budget(&mut self) -> usize {
        self.ships.rows_mut().enforce_budget() + self.threats.enforce_budget()
    }

    /// The row tables' resident page bytes now (what the budget bounds).
    pub fn rows_resident_bytes(&self) -> usize {
        self.ships.rows().resident_bytes() + self.threats.resident_bytes()
    }

    /// The row budget over both tables (`usize::MAX` resident).
    pub fn rows_budget(&self) -> usize {
        self.ships
            .rows()
            .budget()
            .saturating_add(self.threats.budget())
    }

    /// Whether the rows are over their budget with nothing left to evict (read it after
    /// [`Ships::enforce_budget`], which [`Ships::run_until`] ends with): every resident page has
    /// changed since the last save. The host should save, an extra pending save like an autosave
    /// (P29, as herds do, PM7); the pages it writes can then go.
    pub fn needs_save(&self) -> bool {
        self.is_paged() && self.rows_resident_bytes() > self.rows_budget()
    }

    /// Both row tables' counters, summed (peaks are summed: at least the peak of the sum).
    pub fn row_stats(&self) -> RowStats {
        let (a, b) = (self.ships.rows().stats(), self.threats.stats());
        RowStats {
            loads: a.loads + b.loads,
            load_bytes: a.load_bytes + b.load_bytes,
            evictions: a.evictions + b.evictions,
            over_budget: a.over_budget + b.over_budget,
            peak_resident_bytes: a.peak_resident_bytes + b.peak_resident_bytes,
        }
    }

    pub fn reset_row_stats(&mut self) {
        self.ships.rows_mut().reset_stats();
        self.threats.reset_stats();
    }
}
