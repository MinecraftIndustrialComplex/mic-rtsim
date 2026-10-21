// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the dragons (docs/overnight-choices-persist.md S8 to S10): [`Dragons::save`] writes
//! every dragon, lair, event, player view, home counter, ledger line and dictionary mapping, so
//! [`Dragons::load`] gives back an engine that continues exactly as the saved one (the same
//! `state_hash`, the same events in the same order).
//!
//! **The world is saved apart.** Dragons are notables in the phase 1 world's knowledge, fly in its
//! leg index and take its stock: their memories, feelings (grudges) and legs are the world's, and
//! the caller saves the world (`World::save`) in the same instant.
//!
//! **Not saved:** the tunables (the caller's), and the CLI's diagnostics (`log`, `hunts`).
//!
//! # One blob or pages (A16)
//!
//! Both ways write the same two parts: the **meta** (everything resident: the wheel, the lair
//! indexes, players, homes, the ledger, the dictionaries) and the **row tables** (dragons, page
//! table 0x4410; lairs, 0x4411).
//!
//! - [`Dragons::save`] / [`Dragons::load`]: one blob, the meta then every row in key order
//!   (`PagedRows::encode_all`), for a host without a page store (the resident tests and tools;
//!   rtsim-jni pages the dragons, from page 0x4400/0).
//! - [`Dragons::save_paged`] / [`Dragons::open_paged`]: the meta as one page (with each table's
//!   row count and next index) and each table's dirty pages, in the caller's batch; opening reads
//!   the meta page and leaves every row page on disk until something reads it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rtsim_industry::ids::{Good, TrainId};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_kernel::meta_pages::{MapTicket, PagedMap, PagedWheel, WheelTicket};
use rtsim_kernel::page::{PageError, PageKey, SaveBatch};
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::rows::{PageSource, PagedArena, PagedRows, Row, RowStats};
use rtsim_settle::Pos;
use rtsim_trade::GoodId;
use rtsim_travel::legs::{Meeting, Path, Stamp};

use crate::boundary::{self, SpeciesDict};
use crate::dragon::{Dragon, FlightLeg, Lair, LairKind, Lairs, Purpose, State, Target};
use crate::engine::{
    Chest, DRAGON_TABLE, DRAGONS_PER_PAGE, DragonStats, Dragons, Ev, LAIR_CELL_TABLE,
    LAIR_KEY_TABLE, Placing, PlayerView, ROAD_CELL_TABLE, TributeRow, VILLAGE_CELL_TABLE,
    WHEEL_TABLE,
};
use crate::homes::{Entry, Homes, PlayerHome};
use crate::index::PreyIndex;
use crate::prey::{Item, PreyLedger};
use crate::species::{Gender, Species};
use crate::tunables::{DragonTunables, PreyKind};

/// "RTDG" was the format before the indexes moved behind the meta (PF9), "RTD2" before tribute
/// (SINK.md §4.6); no older save opens.
const MAGIC: u32 = u32::from_le_bytes(*b"RTD3");
/// The meta page of a paged save ("RTDM" before PF9, "RTN2" before tribute).
const META_MAGIC: u32 = u32::from_le_bytes(*b"RTN3");

fn bad<T>(what: String) -> codec::Result<T> {
    Err(CodecError::Invalid(what))
}

fn h<T>(r: &mut Reader<'_>) -> codec::Result<Handle<T>> {
    Ok(Handle::from_bits(r.u64()?))
}

fn opt_h<T>(w: &mut Writer, x: Option<Handle<T>>) {
    w.u64(x.map_or(0, |h| h.to_bits()));
}

fn get_opt_h<T>(r: &mut Reader<'_>) -> codec::Result<Option<Handle<T>>> {
    let x: Handle<T> = h(r)?;
    Ok((!x.is_null()).then_some(x))
}

fn put_opt_tick(w: &mut Writer, t: Option<Tick>) {
    match t {
        Some(t) => {
            w.u8(1);
            w.u64(t.0);
        }
        None => w.u8(0),
    }
}

fn get_opt_tick(r: &mut Reader<'_>) -> codec::Result<Option<Tick>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some(Tick(r.u64()?)),
        t => return bad(format!("option tag {t}")),
    })
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

fn u32_of(v: u64) -> codec::Result<u32> {
    u32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} over u32")))
}

fn species(r: &mut Reader<'_>) -> codec::Result<Species> {
    let v = r.u8()?;
    Species::from_index(u32::from(v)).map_or_else(|| bad(format!("species {v}")), Ok)
}

fn gender(r: &mut Reader<'_>) -> codec::Result<Gender> {
    Ok(match r.u8()? {
        0 => Gender::Female,
        1 => Gender::Male,
        t => return bad(format!("gender {t}")),
    })
}

fn put_gender(w: &mut Writer, g: Gender) {
    w.u8(match g {
        Gender::Female => 0,
        Gender::Male => 1,
    });
}

fn put_item(w: &mut Writer, i: Item) {
    match i {
        Item::World(g) => {
            w.u8(0);
            w.u16(g.0);
        }
        Item::Industry(g) => {
            w.u8(1);
            w.u32(g.0);
        }
    }
}

fn get_item(r: &mut Reader<'_>) -> codec::Result<Item> {
    Ok(match r.u8()? {
        0 => Item::World(GoodId(r.u16()?)),
        1 => Item::Industry(Good(r.u32()?)),
        t => return bad(format!("item tag {t}")),
    })
}

fn put_items(w: &mut Writer, m: &BTreeMap<Item, i64>) {
    w.var_u64(m.len() as u64);
    for (k, v) in m {
        put_item(w, *k);
        w.var_i64(*v);
    }
}

fn get_items(r: &mut Reader<'_>) -> codec::Result<BTreeMap<Item, i64>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(4)? {
        let k = get_item(r)?;
        m.insert(k, r.var_i64()?);
    }
    Ok(m)
}

fn put_items128(w: &mut Writer, m: &BTreeMap<Item, i128>) {
    w.var_u64(m.len() as u64);
    for (k, v) in m {
        put_item(w, *k);
        w.i128(*v);
    }
}

fn get_items128(r: &mut Reader<'_>) -> codec::Result<BTreeMap<Item, i128>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(19)? {
        let k = get_item(r)?;
        m.insert(k, r.i128()?);
    }
    Ok(m)
}

fn put_target(w: &mut Writer, t: &Target) {
    match *t {
        Target::Road {
            edge,
            x,
            z,
            frac_milli,
        } => {
            w.u8(0);
            w.var_u64(u64::from(edge));
            w.var_i64(i64::from(x));
            w.var_i64(i64::from(z));
            w.var_u64(u64::from(frac_milli));
        }
        Target::Village { node } => {
            w.u8(1);
            w.var_u64(u64::from(node));
        }
        Target::Home {
            player,
            frame,
            x,
            z,
        } => {
            w.u8(2);
            w.u64(player.to_bits());
            w.u16(frame);
            w.var_i64(i64::from(x));
            w.var_i64(i64::from(z));
        }
        Target::Herd { herd, frame, x, z } => {
            w.u8(3);
            w.u64(herd);
            w.u16(frame);
            w.var_i64(i64::from(x));
            w.var_i64(i64::from(z));
        }
    }
}

fn get_target(r: &mut Reader<'_>) -> codec::Result<Target> {
    Ok(match r.u8()? {
        0 => Target::Road {
            edge: u32_of(r.var_u64()?)?,
            x: i32_of(r.var_i64()?)?,
            z: i32_of(r.var_i64()?)?,
            frac_milli: u32_of(r.var_u64()?)?,
        },
        1 => Target::Village {
            node: u32_of(r.var_u64()?)?,
        },
        2 => Target::Home {
            player: h(r)?,
            frame: r.u16()?,
            x: i32_of(r.var_i64()?)?,
            z: i32_of(r.var_i64()?)?,
        },
        3 => Target::Herd {
            herd: r.u64()?,
            frame: r.u16()?,
            x: i32_of(r.var_i64()?)?,
            z: i32_of(r.var_i64()?)?,
        },
        t => return bad(format!("target tag {t}")),
    })
}

fn put_purpose(w: &mut Writer, p: &Purpose) {
    match p {
        Purpose::Hunt(t) => {
            w.u8(0);
            put_target(w, t);
        }
        Purpose::Home => w.u8(1),
        Purpose::Claim(l) => {
            w.u8(2);
            w.var_u64(u64::from(*l));
        }
    }
}

fn get_purpose(r: &mut Reader<'_>) -> codec::Result<Purpose> {
    Ok(match r.u8()? {
        0 => Purpose::Hunt(get_target(r)?),
        1 => Purpose::Home,
        2 => Purpose::Claim(u32_of(r.var_u64()?)?),
        t => return bad(format!("purpose tag {t}")),
    })
}

fn put_state(w: &mut Writer, s: &State) {
    match s {
        State::Roosting => w.u8(0),
        State::Flying(p) => {
            w.u8(1);
            put_purpose(w, p);
        }
        State::Hunting { target, until } => {
            w.u8(2);
            put_target(w, target);
            w.u64(until.0);
        }
        State::Embodied {
            epoch,
            since,
            depart_at,
            purpose,
            harass,
        } => {
            w.u8(3);
            w.u32(*epoch);
            w.u64(since.0);
            w.u64(depart_at.0);
            put_purpose(w, purpose);
            w.bool(*harass);
        }
        State::Leaving { epoch, purpose } => {
            w.u8(4);
            w.u32(*epoch);
            put_purpose(w, purpose);
        }
    }
}

fn get_state(r: &mut Reader<'_>) -> codec::Result<State> {
    Ok(match r.u8()? {
        0 => State::Roosting,
        1 => State::Flying(get_purpose(r)?),
        2 => State::Hunting {
            target: get_target(r)?,
            until: Tick(r.u64()?),
        },
        3 => State::Embodied {
            epoch: r.u32()?,
            since: Tick(r.u64()?),
            depart_at: Tick(r.u64()?),
            purpose: get_purpose(r)?,
            harass: r.bool()?,
        },
        4 => State::Leaving {
            epoch: r.u32()?,
            purpose: get_purpose(r)?,
        },
        t => return bad(format!("dragon state {t}")),
    })
}

fn put_pos_y(w: &mut Writer, p: &Option<(Pos, i32)>) {
    match p {
        Some((pos, y)) => {
            w.u8(1);
            pos.encode(w);
            w.var_i64(i64::from(*y));
        }
        None => w.u8(0),
    }
}

fn get_pos_y(r: &mut Reader<'_>) -> codec::Result<Option<(Pos, i32)>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some((Pos::decode(r)?, i32_of(r.var_i64()?)?)),
        t => return bad(format!("option tag {t}")),
    })
}

fn put_dragon(w: &mut Writer, x: &Dragon) {
    w.u64(x.actor.to_bits());
    w.u64(x.seed);
    w.bytes(x.name.as_bytes());
    w.u8(x.species.index() as u8);
    put_gender(w, x.gender);
    w.u8(x.variant);
    w.var_u64(x.age_base);
    w.u64(x.age_at.0);
    w.u64(x.dies.0);
    w.var_u64(x.lair.map_or(0, |l| u64::from(l) + 1));
    w.var_i64(i64::from(x.fullness));
    w.u64(x.fed_at.0);
    w.f32(x.health);
    w.u64(x.health_at.0);
    put_state(w, &x.state);
    match &x.leg {
        Some(l) => {
            w.u8(1);
            l.path.encode(w);
            w.var_i64(i64::from(l.y));
            l.stamp.encode(w);
        }
        None => w.u8(0),
    }
    w.u32(x.plan);
    w.u32(x.epoch);
    opt_h(w, x.mate);
    opt_h(w, x.parent);
    w.var_u64(x.children.len() as u64);
    x.children.iter().for_each(|c| w.u64(c.to_bits()));
    w.var_u64(x.victims.len() as u64);
    x.victims.iter().for_each(|v| w.u64(v.to_bits()));
    w.u8(x.fruitless);
    w.u8(x.bout);
    put_opt_tick(w, x.last_grudge);
    put_items(w, &x.carried);
    put_pos_y(w, &x.rest);
}

fn get_dragon(r: &mut Reader<'_>) -> codec::Result<Dragon> {
    Ok(Dragon {
        actor: h(r)?,
        seed: r.u64()?,
        name: String::from_utf8(r.bytes()?.to_vec())
            .map_err(|e| CodecError::Invalid(format!("a dragon's name: {e}")))?,
        species: species(r)?,
        gender: gender(r)?,
        variant: r.u8()?,
        age_base: r.var_u64()?,
        age_at: Tick(r.u64()?),
        dies: Tick(r.u64()?),
        lair: r.var_u64()?.checked_sub(1).map(u32_of).transpose()?,
        fullness: i32_of(r.var_i64()?)?,
        fed_at: Tick(r.u64()?),
        health: r.f32()?,
        health_at: Tick(r.u64()?),
        state: get_state(r)?,
        leg: match r.u8()? {
            0 => None,
            1 => Some(FlightLeg {
                path: Path::decode(r)?,
                y: i32_of(r.var_i64()?)?,
                stamp: Stamp::decode(r)?,
            }),
            t => return bad(format!("leg tag {t}")),
        },
        plan: r.u32()?,
        epoch: r.u32()?,
        mate: get_opt_h(r)?,
        parent: get_opt_h(r)?,
        children: {
            let n = r.len_prefix(8)?;
            (0..n).map(|_| h(r)).collect::<codec::Result<_>>()?
        },
        victims: {
            let n = r.len_prefix(8)?;
            (0..n).map(|_| h(r)).collect::<codec::Result<_>>()?
        },
        fruitless: r.u8()?,
        bout: r.u8()?,
        last_grudge: get_opt_tick(r)?,
        carried: get_items(r)?,
        rest: get_pos_y(r)?,
    })
}

fn put_lair(w: &mut Writer, l: &Lair) {
    w.u64(l.key);
    w.bytes(l.name.as_bytes());
    w.u8(l.species.index() as u8);
    w.u8(match l.kind {
        LairKind::Roost => 0,
        LairKind::Cave => 1,
    });
    l.pos.encode(w);
    w.var_i64(i64::from(l.y));
    w.u64(l.actor.to_bits());
    w.var_u64(l.occupants.len() as u64);
    l.occupants.iter().for_each(|d| w.u64(d.to_bits()));
    put_items(w, &l.hoard);
    put_opt_tick(w, l.vacant_since);
    w.u32(l.plan);
    put_items(w, &l.unplaced);
    w.var_i64(l.placed);
    w.var_u64(l.stacks);
}

fn get_lair(r: &mut Reader<'_>) -> codec::Result<Lair> {
    Ok(Lair {
        key: r.u64()?,
        name: String::from_utf8(r.bytes()?.to_vec())
            .map_err(|e| CodecError::Invalid(format!("a lair's name: {e}")))?,
        species: species(r)?,
        kind: match r.u8()? {
            0 => LairKind::Roost,
            1 => LairKind::Cave,
            t => return bad(format!("lair kind {t}")),
        },
        pos: Pos::decode(r)?,
        y: i32_of(r.var_i64()?)?,
        actor: h(r)?,
        occupants: {
            let n = r.len_prefix(8)?;
            (0..n).map(|_| h(r)).collect::<codec::Result<_>>()?
        },
        hoard: get_items(r)?,
        vacant_since: get_opt_tick(r)?,
        plan: r.u32()?,
        unplaced: get_items(r)?,
        placed: r.var_i64()?,
        stacks: r.var_u64()?,
    })
}

/// Heap bytes a `BTreeMap` of `len` entries of `entry` bytes owns: leaf nodes of eleven, at
/// least one when not empty (an estimate for the page budget).
fn btree_bytes(len: usize, entry: usize) -> usize {
    len.div_ceil(11) * (11 * entry + 16)
}

impl Encode for Dragon {
    fn encode(&self, w: &mut Writer) {
        put_dragon(w, self);
    }
}

impl Decode for Dragon {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        get_dragon(r)
    }
}

impl Row for Dragon {
    fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.children.capacity() * std::mem::size_of::<Handle<Dragon>>()
            + self.victims.capacity() * std::mem::size_of::<rtsim_knowledge::ActorId>()
            + btree_bytes(self.carried.len(), std::mem::size_of::<(Item, i64)>())
    }
}

impl Encode for Lair {
    fn encode(&self, w: &mut Writer) {
        put_lair(w, self);
    }
}

impl Decode for Lair {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        get_lair(r)
    }
}

impl Row for Lair {
    fn heap_bytes(&self) -> usize {
        self.name.capacity()
            + self.occupants.capacity() * std::mem::size_of::<Handle<Dragon>>()
            + btree_bytes(self.hoard.len(), std::mem::size_of::<(Item, i64)>())
            + btree_bytes(self.unplaced.len(), std::mem::size_of::<(Item, i64)>())
    }
}

fn put_ev(w: &mut Writer, e: &Ev) {
    match *e {
        Ev::Hungry { d, plan } => {
            w.u8(0);
            w.u64(d.to_bits());
            w.u32(plan);
        }
        Ev::LegEnd { d, plan } => {
            w.u8(1);
            w.u64(d.to_bits());
            w.u32(plan);
        }
        Ev::HuntEnd { d, plan } => {
            w.u8(2);
            w.u64(d.to_bits());
            w.u32(plan);
        }
        Ev::Cross {
            d,
            plan,
            edge,
            frac_milli,
            from,
            to,
        } => {
            w.u8(3);
            w.u64(d.to_bits());
            w.u32(plan);
            w.u32(edge);
            w.u32(frac_milli);
            w.u64(from.0);
            w.u64(to.0);
        }
        Ev::Seen { d, player, meeting } => {
            w.u8(4);
            w.u64(d.to_bits());
            w.u64(player.to_bits());
            w.u64(meeting.at.0);
            meeting.a.encode(w);
            meeting.b.encode(w);
        }
        Ev::SeenAtLair {
            d,
            player,
            interest,
            lair,
        } => {
            w.u8(5);
            w.u64(d.to_bits());
            w.u64(player.to_bits());
            interest.encode(w);
            w.u32(lair);
        }
        Ev::TrainScan { d, plan } => {
            w.u8(6);
            w.u64(d.to_bits());
            w.u32(plan);
        }
        Ev::TrainRaid { d, plan, train } => {
            w.u8(7);
            w.u64(d.to_bits());
            w.u32(plan);
            w.u64(train.0);
        }
        Ev::Depart { d, plan, reason } => {
            w.u8(8);
            w.u64(d.to_bits());
            w.u32(plan);
            w.u32(reason);
        }
        Ev::Breed { d } => {
            w.u8(9);
            w.u64(d.to_bits());
        }
        Ev::Disperse { d } => {
            w.u8(10);
            w.u64(d.to_bits());
        }
        Ev::Respawn { lair, plan } => {
            w.u8(11);
            w.u32(lair);
            w.u32(plan);
        }
        Ev::Death { d } => {
            w.u8(12);
            w.u64(d.to_bits());
        }
        Ev::Tribute { node, stamp } => {
            w.u8(13);
            w.var_u64(u64::from(node));
            w.u32(stamp);
        }
    }
}

fn get_ev(r: &mut Reader<'_>) -> codec::Result<Ev> {
    Ok(match r.u8()? {
        0 => Ev::Hungry {
            d: h(r)?,
            plan: r.u32()?,
        },
        1 => Ev::LegEnd {
            d: h(r)?,
            plan: r.u32()?,
        },
        2 => Ev::HuntEnd {
            d: h(r)?,
            plan: r.u32()?,
        },
        3 => Ev::Cross {
            d: h(r)?,
            plan: r.u32()?,
            edge: r.u32()?,
            frac_milli: r.u32()?,
            from: Tick(r.u64()?),
            to: Tick(r.u64()?),
        },
        4 => Ev::Seen {
            d: h(r)?,
            player: h(r)?,
            meeting: Meeting {
                at: Tick(r.u64()?),
                a: Stamp::decode(r)?,
                b: Stamp::decode(r)?,
            },
        },
        5 => Ev::SeenAtLair {
            d: h(r)?,
            player: h(r)?,
            interest: Stamp::decode(r)?,
            lair: r.u32()?,
        },
        6 => Ev::TrainScan {
            d: h(r)?,
            plan: r.u32()?,
        },
        7 => Ev::TrainRaid {
            d: h(r)?,
            plan: r.u32()?,
            train: TrainId(r.u64()?),
        },
        8 => Ev::Depart {
            d: h(r)?,
            plan: r.u32()?,
            reason: r.u32()?,
        },
        9 => Ev::Breed { d: h(r)? },
        10 => Ev::Disperse { d: h(r)? },
        11 => Ev::Respawn {
            lair: r.u32()?,
            plan: r.u32()?,
        },
        12 => Ev::Death { d: h(r)? },
        13 => Ev::Tribute {
            node: u32_of(r.var_u64()?)?,
            stamp: r.u32()?,
        },
        t => return bad(format!("dragon event {t}")),
    })
}

fn put_homes(w: &mut Writer, x: &Homes) {
    w.var_u64(x.half_life);
    w.var_u64(x.cap as u64);
    w.var_u64(x.max_gap);
    w.var_u64(x.players.len() as u64);
    for (a, p) in &x.players {
        w.u64(a.to_bits());
        match p.last {
            Some((pos, t)) => {
                w.u8(1);
                pos.encode(w);
                w.u64(t.0);
            }
            None => w.u8(0),
        }
        w.var_u64(p.regions.len() as u64);
        for e in &p.regions {
            w.u16(e.frame);
            w.var_i64(e.rx);
            w.var_i64(e.rz);
            w.f64(e.v);
            w.f64(e.sx);
            w.f64(e.sz);
            w.u64(e.at.0);
        }
    }
}

fn get_homes(r: &mut Reader<'_>) -> codec::Result<Homes> {
    let half_life = r.var_u64()?;
    let cap = (r.var_u64()? as usize).max(1);
    let max_gap = r.var_u64()?;
    let mut players = BTreeMap::new();
    for _ in 0..r.len_prefix(10)? {
        let a = h(r)?;
        let last = match r.u8()? {
            0 => None,
            1 => Some((Pos::decode(r)?, Tick(r.u64()?))),
            t => return bad(format!("option tag {t}")),
        };
        let n = r.len_prefix(36)?;
        let mut regions = Vec::with_capacity(n);
        for _ in 0..n {
            regions.push(Entry {
                frame: r.u16()?,
                rx: r.var_i64()?,
                rz: r.var_i64()?,
                v: r.f64()?,
                sx: r.f64()?,
                sz: r.f64()?,
                at: Tick(r.u64()?),
            });
        }
        players.insert(a, PlayerHome { last, regions });
    }
    Ok(Homes {
        players,
        half_life,
        cap,
        max_gap,
    })
}

type Cell = (u16, i64, i64);
type Cells<T> = PagedMap<Cell, Vec<T>>;

fn put_cell(w: &mut Writer, &(f, x, z): &Cell) {
    w.u16(f);
    w.var_i64(x);
    w.var_i64(z);
}

fn get_cell(r: &mut Reader<'_>) -> codec::Result<Cell> {
    Ok((r.u16()?, r.var_i64()?, r.var_i64()?))
}

fn put_ids(w: &mut Writer, v: &[u32]) {
    w.var_u64(v.len() as u64);
    v.iter().for_each(|i| w.var_u64(u64::from(*i)));
}

fn get_ids(r: &mut Reader<'_>) -> codec::Result<Vec<u32>> {
    let n = r.len_prefix(1)?;
    (0..n)
        .map(|_| r.var_u64().and_then(u32_of))
        .collect::<codec::Result<Vec<_>>>()
}

fn put_cells(w: &mut Writer, m: &Cells<u32>) {
    w.var_u64(m.len() as u64);
    for (c, v) in m {
        put_cell(w, c);
        put_ids(w, v);
    }
}

fn get_cells(r: &mut Reader<'_>) -> codec::Result<Cells<u32>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(5)? {
        let cell = get_cell(r)?;
        m.insert(cell, get_ids(r)?);
    }
    Ok(m.into())
}

const STATS: usize = 31;

fn stats_array(s: &DragonStats) -> [u64; STATS] {
    [
        s.events,
        s.stale,
        s.legs,
        s.decisions,
        s.hunts,
        s.fruitless,
        s.crossings,
        s.caravans_attacked,
        s.village_raids,
        s.district_raids,
        s.train_raids,
        s.seen,
        s.seen_stale,
        s.materialized,
        s.departures,
        s.bored,
        s.hurt,
        s.deaths_violent,
        s.deaths_age,
        s.births,
        s.respawns,
        s.claims,
        s.migrations,
        s.adopted,
        s.discarded,
        s.hoards_robbed,
        s.herd_hunts,
        s.seen_across_stop,
        s.tributes,
        s.tribute_lapses,
        s.tribute_stacks,
    ]
}

fn stats_of(v: [u64; STATS]) -> DragonStats {
    DragonStats {
        events: v[0],
        stale: v[1],
        legs: v[2],
        decisions: v[3],
        hunts: v[4],
        fruitless: v[5],
        crossings: v[6],
        caravans_attacked: v[7],
        village_raids: v[8],
        district_raids: v[9],
        train_raids: v[10],
        seen: v[11],
        seen_stale: v[12],
        materialized: v[13],
        departures: v[14],
        bored: v[15],
        hurt: v[16],
        deaths_violent: v[17],
        deaths_age: v[18],
        births: v[19],
        respawns: v[20],
        claims: v[21],
        migrations: v[22],
        adopted: v[23],
        discarded: v[24],
        hoards_robbed: v[25],
        herd_hunts: v[26],
        seen_across_stop: v[27],
        tributes: v[28],
        tribute_lapses: v[29],
        tribute_stacks: v[30],
    }
}

/// What [`Dragons::save_paged`] put in a batch: hand it back with [`Dragons::saved`] once the
/// commit is durable, or [`Dragons::save_failed`].
#[derive(Debug, Default)]
#[must_use = "row pages stay pinned until the tickets come back through saved or save_failed"]
pub struct DragonsTickets {
    wheel: WheelTicket,
    by_key: MapTicket,
    /// Lairs by cell, the prey index's villages and its roads.
    cells: [MapTicket; 3],
    dragons: SaveTicket,
    lairs: SaveTicket,
}

impl DragonsTickets {
    /// Row and index pages the save wrote (or deleted, for the rows).
    pub fn pages(&self) -> usize {
        self.dragons.pages()
            + self.lairs.pages()
            + self.wheel.pages()
            + self.by_key.pages()
            + self.cells.iter().map(MapTicket::pages).sum::<usize>()
    }
}

/// Every page table the dragons own when they page: a meta key in none of them.
const TABLES: [u16; 7] = [
    DRAGON_TABLE,
    Lairs::TABLE,
    WHEEL_TABLE,
    LAIR_KEY_TABLE,
    LAIR_CELL_TABLE,
    VILLAGE_CELL_TABLE,
    ROAD_CELL_TABLE,
];

/// The row tables' paging counters, for the bench.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DragonsPaging {
    pub dragons: RowStats,
    pub lairs: RowStats,
}

impl DragonsPaging {
    pub fn loads(&self) -> u64 {
        self.dragons.loads + self.lairs.loads
    }

    pub fn evictions(&self) -> u64 {
        self.dragons.evictions + self.lairs.evictions
    }

    pub fn over_budget(&self) -> u64 {
        self.dragons.over_budget + self.lairs.over_budget
    }
}

/// The budget's share for the dragons' rows; the lairs get the rest. PLACEHOLDER three quarters:
/// a dragon row is about twice a lair's, and most events change a dragon but only read its lair.
fn split_budget(budget: usize) -> (usize, usize) {
    let lairs = budget / 4;
    (budget - lairs, lairs)
}

fn format_err(e: CodecError) -> PageError {
    PageError::Format(format!("the dragons' meta page: {e}"))
}

impl Dragons {
    /// Everything resident (module docs): the meta of both save forms.
    fn put_meta(&self, w: &mut Writer) {
        w.u64(self.seed);
        w.u64(self.now.0);
        w.u64(self.next_seed);
        put_homes(w, &self.homes);
        let l = &self.ledger;
        for m in [
            &l.taken, &l.eaten, &l.burnt, &l.hoarded, &l.robbed, &l.lost, &l.tribute,
        ] {
            put_items128(w, m);
        }
        stats_array(&self.stats).iter().for_each(|v| w.var_u64(*v));
        w.bool(self.log_enabled);
        for season in 0..4 {
            for k in 0..PreyKind::COUNT {
                w.var_u64(self.prey_by_season[season][k]);
                w.var_i64(self.food_by_season[season][k]);
            }
        }
        let mut cmds = Vec::new();
        let dict = SpeciesDict::default();
        for c in &self.outbox {
            boundary::encode_cmd(0, c, &dict, &mut cmds);
        }
        w.var_u64(self.outbox.len() as u64);
        w.bytes(&cmds);
        w.var_u64(self.players.len() as u64);
        for (a, p) in &self.players {
            w.u64(a.to_bits());
            w.u64(p.uuid.0);
            w.u64(p.uuid.1);
            match p.interest {
                Some(s) => {
                    w.u8(1);
                    s.encode(w);
                }
                None => w.u8(0),
            }
        }
        w.var_u64(self.index.nodes as u64);
        w.var_u64(self.index.edges as u64);
        w.var_u64(self.by_entity.len() as u64);
        for ((most, least), d) in &self.by_entity {
            w.u64(*most);
            w.u64(*least);
            w.u64(d.to_bits());
        }
        w.var_u64(self.adopted.len() as u64);
        for ((most, least), d) in &self.adopted {
            w.u64(*most);
            w.u64(*least);
            w.u64(d.to_bits());
        }
        w.bool(self.dying_by_body);
        w.var_u64(self.items.len() as u64);
        for (id, item) in &self.items {
            w.u32(*id);
            put_item(w, *item);
        }
        w.var_u64(self.herd_bodies.len() as u64);
        for (id, g) in &self.herd_bodies {
            w.u32(*id);
            w.u16(g.0);
        }
        w.var_u64(self.embodied.len() as u64);
        self.embodied.iter().for_each(|d| w.u64(d.to_bits()));
        w.var_u64(self.vacant.len() as u64);
        self.vacant.iter().for_each(|l| w.var_u64(u64::from(*l)));
        w.u32(self.tribute_stamp);
        w.var_u64(self.tribute.len() as u64);
        for (node, row) in &self.tribute {
            w.var_u64(u64::from(*node));
            w.u32(row.lair);
            w.u32(row.stamp);
            put_opt_tick(w, row.paid_at);
        }
        w.var_u64(self.placing.len() as u64);
        for (op, p) in &self.placing {
            w.u64(*op);
            w.u32(p.lair);
            put_item(w, p.item);
            w.var_i64(p.count);
            w.u32(p.chest.dimension);
            p.chest.pos.iter().for_each(|v| w.var_i64(i64::from(*v)));
        }
    }

    /// An engine from what [`Dragons::put_meta`] wrote, with empty row tables.
    fn get_meta(tun: DragonTunables, r: &mut Reader<'_>) -> codec::Result<Dragons> {
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let next_seed = r.u64()?;
        let mut d = Dragons::new(seed, tun, now);
        d.next_seed = next_seed;
        d.homes = get_homes(r)?;
        d.ledger = PreyLedger {
            taken: get_items128(r)?,
            eaten: get_items128(r)?,
            burnt: get_items128(r)?,
            hoarded: get_items128(r)?,
            robbed: get_items128(r)?,
            lost: get_items128(r)?,
            tribute: get_items128(r)?,
        };
        let mut s = [0u64; STATS];
        for v in &mut s {
            *v = r.var_u64()?;
        }
        d.stats = stats_of(s);
        d.log_enabled = r.bool()?;
        for season in 0..4 {
            for k in 0..PreyKind::COUNT {
                d.prey_by_season[season][k] = r.var_u64()?;
                d.food_by_season[season][k] = r.var_i64()?;
            }
        }
        let n = r.var_u64()?;
        let mut cmds = r.bytes()?;
        let dict = SpeciesDict::default();
        for _ in 0..n {
            let (size, _, c) = boundary::decode_cmd(cmds, &dict)
                .map_err(|e| CodecError::Invalid(format!("a queued dragon command: {e:?}")))?;
            d.outbox.push(c);
            cmds = &cmds[size..];
        }
        if !cmds.is_empty() {
            return bad("bytes after the queued dragon commands".into());
        }
        for _ in 0..r.len_prefix(25)? {
            let a = h(r)?;
            let uuid = (r.u64()?, r.u64()?);
            let interest = match r.u8()? {
                0 => None,
                1 => Some(Stamp::decode(r)?),
                t => return bad(format!("option tag {t}")),
            };
            d.players.insert(a, PlayerView { uuid, interest });
        }
        d.index = PreyIndex {
            villages: PagedMap::new(),
            roads: PagedMap::new(),
            nodes: r.var_u64()? as usize,
            edges: r.var_u64()? as usize,
        };
        for _ in 0..r.len_prefix(24)? {
            let uuid = (r.u64()?, r.u64()?);
            d.by_entity.insert(uuid, h(r)?);
        }
        for _ in 0..r.len_prefix(24)? {
            let uuid = (r.u64()?, r.u64()?);
            d.adopted.insert(uuid, h(r)?);
        }
        d.dying_by_body = r.bool()?;
        for _ in 0..r.len_prefix(6)? {
            let id = r.u32()?;
            d.items.insert(id, get_item(r)?);
        }
        for _ in 0..r.len_prefix(6)? {
            let id = r.u32()?;
            d.herd_bodies.insert(id, GoodId(r.u16()?));
        }
        for _ in 0..r.len_prefix(8)? {
            d.embodied.insert(h(r)?);
        }
        for _ in 0..r.len_prefix(1)? {
            d.vacant.insert(u32_of(r.var_u64()?)?);
        }
        d.tribute_stamp = r.u32()?;
        for _ in 0..r.len_prefix(10)? {
            let node = u32_of(r.var_u64()?)?;
            let row = TributeRow {
                lair: r.u32()?,
                stamp: r.u32()?,
                paid_at: get_opt_tick(r)?,
            };
            d.tribute.insert(node, row);
        }
        for _ in 0..r.len_prefix(20)? {
            let op = r.u64()?;
            let lair = r.u32()?;
            let item = get_item(r)?;
            let count = r.var_i64()?;
            let dimension = r.u32()?;
            let mut pos = [0; 3];
            for v in &mut pos {
                *v = i32_of(r.var_i64()?)?;
            }
            d.placing.insert(
                op,
                Placing {
                    lair,
                    item,
                    count,
                    chest: Chest { dimension, pos },
                },
            );
        }
        Ok(d)
    }

    /// Every dragon, lair, event and table (see the module docs for what is left out), as one
    /// blob: the meta, then the dragons and the lairs in key order. The commands not taken yet
    /// are written too. A whole-table pass: in a paged engine it loads every row page.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(1024 + self.dragons.len() * 256);
        w.u32(MAGIC);
        self.put_meta(&mut w);
        w.var_u64(self.lair_by_key.len() as u64);
        for (k, l) in &self.lair_by_key {
            w.u64(*k);
            w.u32(*l);
        }
        put_cells(&mut w, &self.lair_cells);
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        put_cells(&mut w, &self.index.villages);
        put_cells(&mut w, &self.index.roads);
        self.dragons.encode_all(&mut w);
        self.lairs.rows().encode_all(&mut w);
        w.into_bytes()
    }

    /// Reads what [`Dragons::save`] wrote, with the tunables the engine runs with: a resident
    /// engine (no page store).
    pub fn load(tun: DragonTunables, bytes: &[u8]) -> codec::Result<Dragons> {
        let r = &mut Reader::new(bytes);
        if r.u32()? != MAGIC {
            return bad("not a dragons page".into());
        }
        let mut d = Dragons::get_meta(tun, r)?;
        for _ in 0..r.len_prefix(12)? {
            let k = r.u64()?;
            d.lair_by_key.insert(k, r.u32()?);
        }
        d.lair_cells = get_cells(r)?;
        d.wheel = PagedWheel::decode_with(r, get_ev)?;
        d.index.villages = get_cells(r)?;
        d.index.roads = get_cells(r)?;
        d.dragons = PagedArena::decode_all(DRAGON_TABLE, DRAGONS_PER_PAGE, r)?;
        let lairs = PagedRows::decode_all(Lairs::TABLE, Lairs::PER_PAGE, r)?;
        // Keys in strictly rising order, the last `len - 1`: exactly `0..len`.
        if !lairs.is_empty() && lairs.get(lairs.len() - 1).is_none() {
            return bad("the lairs' keys are not 0..len".into());
        }
        d.lairs = Lairs::from_rows(lairs);
        if !r.is_empty() {
            return bad(format!(
                "{} trailing bytes in a dragons page",
                r.remaining()
            ));
        }
        d.check_resident()?;
        d.check_rows()?;
        Ok(d)
    }

    // ---- paged (A16) ---------------------------------------------------------------------------

    /// Starts paging the row tables to `source` (the page store the caller commits saves
    /// through) under `budget` bytes of resident row pages, split between the dragons and the
    /// lairs. The rows stay resident until a save writes them ([`Dragons::save_paged`]).
    pub fn attach_store(&mut self, source: Arc<dyn PageSource>, budget: usize) {
        let (bd, bl) = split_budget(budget);
        self.dragons.rows_mut().attach(source.clone(), bd);
        self.lairs.rows_mut().attach(source, bl);
    }

    /// Whether the row tables page to a store.
    pub fn is_paged(&self) -> bool {
        self.dragons.rows().is_paged()
    }

    /// A paged save into `batch`: the meta page under `meta_key` (everything resident, and each
    /// row table's row count and next index) and every row page changed since the last save.
    /// The pages stay pinned until the tickets come back. `meta_key` must not be in a row table.
    pub fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> DragonsTickets {
        assert!(
            !TABLES.contains(&meta_key.table()),
            "the dragons' meta page {meta_key:?} inside a row or index table"
        );
        let mut w = Writer::with_capacity(4096);
        w.u32(META_MAGIC);
        self.put_meta(&mut w);
        let (len, next) = self.dragons.meta();
        w.var_u64(len);
        w.u32(next);
        w.var_u64(self.lairs.rows().len());
        let wheel = self
            .wheel
            .save_paged(batch, WHEEL_TABLE, &mut w, |e, w| put_ev(w, e));
        let by_key = self.lair_by_key.save_paged(
            batch,
            LAIR_KEY_TABLE,
            &mut w,
            |k, w| w.u64(*k),
            |l, w| w.u32(*l),
        );
        let cells = [
            (&mut self.lair_cells, LAIR_CELL_TABLE),
            (&mut self.index.villages, VILLAGE_CELL_TABLE),
            (&mut self.index.roads, ROAD_CELL_TABLE),
        ]
        .map(|(m, table)| {
            m.save_paged(
                batch,
                table,
                &mut w,
                |c, w| put_cell(w, c),
                |v, w| put_ids(w, v),
            )
        });
        batch.put(meta_key, w.as_bytes());
        DragonsTickets {
            wheel,
            by_key,
            cells,
            dragons: self.dragons.rows_mut().save_dirty(batch),
            lairs: self.lairs.rows_mut().save_dirty(batch),
        }
    }

    /// The commit holding [`Dragons::save_paged`]'s batch is durable.
    pub fn saved(&mut self, t: DragonsTickets) {
        self.wheel.saved(t.wheel);
        self.lair_by_key.saved(t.by_key);
        let [a, b, c] = t.cells;
        self.lair_cells.saved(a);
        self.index.villages.saved(b);
        self.index.roads.saved(c);
        self.dragons.rows_mut().saved(t.dragons);
        self.lairs.rows_mut().saved(t.lairs);
    }

    /// The commit failed: the row and index pages it held are dirty again (the rest of the meta
    /// is rewritten whole by every save, so it needs nothing).
    pub fn save_failed(&mut self, t: DragonsTickets) {
        self.wheel.save_failed(t.wheel);
        self.lair_by_key.save_failed(t.by_key);
        let [a, b, c] = t.cells;
        self.lair_cells.save_failed(a);
        self.index.villages.save_failed(b);
        self.index.roads.save_failed(c);
        self.dragons.rows_mut().save_failed(t.dragons);
        self.lairs.rows_mut().save_failed(t.lairs);
    }

    /// An engine saved by [`Dragons::save_paged`]: reads the meta page and opens both row tables
    /// with nothing resident, paging under `budget` as [`Dragons::attach_store`].
    pub fn open_paged(
        source: Arc<dyn PageSource>,
        budget: usize,
        meta_key: PageKey,
        tun: DragonTunables,
    ) -> Result<Dragons, PageError> {
        let raw = source
            .read(meta_key)?
            .ok_or_else(|| PageError::Format(format!("no dragons meta page at {meta_key:?}")))?;
        let r = &mut Reader::new(&raw);
        if r.u32().map_err(format_err)? != META_MAGIC {
            return Err(PageError::Format("not a dragons meta page".into()));
        }
        let mut d = Dragons::get_meta(tun, r).map_err(format_err)?;
        let len = r.var_u64().map_err(format_err)?;
        let next = r.u32().map_err(format_err)?;
        let lairs = r.var_u64().map_err(format_err)?;
        let src = &*source;
        d.wheel = PagedWheel::open_paged(r, WHEEL_TABLE, src, get_ev).map_err(format_err)?;
        d.lair_by_key = PagedMap::open_paged(r, LAIR_KEY_TABLE, src, |r| r.u64(), |r| r.u32())
            .map_err(format_err)?;
        d.lair_cells =
            PagedMap::open_paged(r, LAIR_CELL_TABLE, src, get_cell, get_ids).map_err(format_err)?;
        d.index.villages = PagedMap::open_paged(r, VILLAGE_CELL_TABLE, src, get_cell, get_ids)
            .map_err(format_err)?;
        d.index.roads =
            PagedMap::open_paged(r, ROAD_CELL_TABLE, src, get_cell, get_ids).map_err(format_err)?;
        if !r.is_empty() {
            return Err(PageError::Format(format!(
                "{} trailing bytes in the dragons' meta page",
                r.remaining()
            )));
        }
        if len > u64::from(next) || lairs > u64::from(u32::MAX) {
            return Err(PageError::Format(format!(
                "the dragons' meta page: {len} rows under next index {next}, {lairs} lairs"
            )));
        }
        let (bd, bl) = split_budget(budget);
        d.dragons = PagedArena::open(
            DRAGON_TABLE,
            DRAGONS_PER_PAGE,
            source.clone(),
            bd,
            (len, next),
        );
        d.lairs = Lairs::from_rows(PagedRows::open(
            Lairs::TABLE,
            Lairs::PER_PAGE,
            source,
            bl,
            lairs,
        ));
        d.check_resident().map_err(format_err)?;
        Ok(d)
    }

    /// Evicts row pages whose state is on disk until each table fits its share of the budget,
    /// and measures the rows changed since the last call. A paged host calls it at its step
    /// boundaries (rtsim-jni once a step, the bench every second of sim time): `run_until` does
    /// not, so a host stepping event by event pays the check (a pass over the resident pages
    /// when over budget) once a step, not once an event. Returns the pages evicted.
    pub fn enforce_budget(&mut self) -> usize {
        self.dragons.rows_mut().enforce_budget() + self.lairs.rows_mut().enforce_budget()
    }

    /// Resident row page bytes, both tables: what the budget bounds.
    pub fn resident_row_bytes(&self) -> usize {
        self.dragons.rows().resident_bytes() + self.lairs.rows().resident_bytes()
    }

    /// The row tables' budget (both shares), `usize::MAX` while resident.
    pub fn row_budget(&self) -> usize {
        self.dragons
            .rows()
            .budget()
            .saturating_add(self.lairs.rows().budget())
    }

    /// Whether the rows are over their budget with nothing left to evict (asked right after
    /// [`Dragons::enforce_budget`]): every resident page beyond the budget changed since the last
    /// save. The host should save; an extra save is a pending generation like any other (P29),
    /// as herds' `needs_save` (PM7). Over both tables, so either may use the other's unused share
    /// until the save. Never while resident.
    pub fn needs_save(&self) -> bool {
        self.is_paged() && self.resident_row_bytes() > self.row_budget()
    }

    pub fn paging_stats(&self) -> DragonsPaging {
        DragonsPaging {
            dragons: self.dragons.rows().stats(),
            lairs: self.lairs.rows().stats(),
        }
    }

    pub fn reset_paging_stats(&mut self) {
        self.dragons.rows_mut().reset_stats();
        self.lairs.rows_mut().reset_stats();
    }

    // ---- checks on load --------------------------------------------------------------------------

    /// The resident state a corrupt page could name wrongly: lairs past the table, embodied
    /// dragons past the arena. Checked on every load, paged or not; no row is read.
    fn check_resident(&self) -> codec::Result<()> {
        let nl = self.lairs.len() as u32;
        let lairs_ok = self.lair_by_key.values().all(|&l| l < nl)
            && self.lair_cells.values().flatten().all(|&l| l < nl)
            && self.vacant.iter().all(|&l| l < nl)
            && self.tribute.values().all(|t| t.lair < nl)
            && self.placing.values().all(|p| p.lair < nl);
        if !lairs_ok {
            return bad("a lair index names a lair past the table".into());
        }
        for (_, e) in self.wheel.iter() {
            let ok = match *e {
                Ev::SeenAtLair { lair, .. } | Ev::Respawn { lair, .. } => lair < nl,
                _ => true,
            };
            if !ok {
                return bad("a dragon event names a lair past the table".into());
            }
        }
        let (_, next) = self.dragons.meta();
        if let Some(d) = self
            .embodied
            .iter()
            .find(|d| d.generation() != PagedArena::<Dragon>::GENERATION || d.index() >= next)
        {
            return bad(format!("the embodied index names {d:?}, past the dragons"));
        }
        Ok(())
    }

    /// Lairs the rows name, and the embodied and vacant indexes against the rows: a whole-table pass,
    /// for [`Dragons::load`] (which has read every row anyway).
    fn check_rows(&self) -> codec::Result<()> {
        let nl = self.lairs.len() as u32;
        let mut embodied = BTreeSet::new();
        for (id, x) in self.dragons.iter() {
            let ok = x.lair.is_none_or(|l| l < nl)
                && match x.state {
                    State::Flying(Purpose::Claim(l))
                    | State::Embodied {
                        purpose: Purpose::Claim(l),
                        ..
                    }
                    | State::Leaving {
                        purpose: Purpose::Claim(l),
                        ..
                    } => l < nl,
                    _ => true,
                };
            if !ok {
                return bad(format!("dragon {id:?} names a lair past {nl}"));
            }
            if x.is_embodied() {
                embodied.insert(id);
            }
        }
        if embodied != self.embodied {
            return bad("the embodied index does not match the dragons".into());
        }
        let vacant: BTreeSet<u32> = self
            .lairs
            .iter()
            .enumerate()
            .filter(|(_, l)| l.occupants.is_empty() && l.vacant_since.is_some())
            .map(|(i, _)| i as u32)
            .collect();
        if vacant != self.vacant {
            return bad("the vacant index does not match the lairs".into());
        }
        Ok(())
    }
}
