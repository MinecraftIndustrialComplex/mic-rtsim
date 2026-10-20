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

use std::collections::BTreeMap;

use rtsim_industry::ids::{Good, TrainId};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_settle::Pos;
use rtsim_trade::GoodId;
use rtsim_travel::legs::{Meeting, Path, Stamp};

use crate::boundary::{self, SpeciesDict};
use crate::dragon::{Dragon, FlightLeg, Lair, LairKind, Purpose, State, Target};
use crate::engine::{DragonStats, Dragons, Ev, PlayerView};
use crate::homes::{Entry, Homes, PlayerHome};
use crate::index::PreyIndex;
use crate::prey::{Item, PreyLedger};
use crate::species::{Gender, Species};
use crate::tunables::{DragonTunables, PreyKind};

const MAGIC: u32 = u32::from_le_bytes(*b"RTDG");

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
    })
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

type Cells<T> = BTreeMap<(u16, i64, i64), Vec<T>>;

fn put_cells(w: &mut Writer, m: &Cells<u32>) {
    w.var_u64(m.len() as u64);
    for (&(f, x, z), v) in m {
        w.u16(f);
        w.var_i64(x);
        w.var_i64(z);
        w.var_u64(v.len() as u64);
        v.iter().for_each(|i| w.var_u64(u64::from(*i)));
    }
}

fn get_cells(r: &mut Reader<'_>) -> codec::Result<Cells<u32>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(5)? {
        let cell = (r.u16()?, r.var_i64()?, r.var_i64()?);
        let n = r.len_prefix(1)?;
        let v = (0..n)
            .map(|_| r.var_u64().and_then(u32_of))
            .collect::<codec::Result<Vec<_>>>()?;
        m.insert(cell, v);
    }
    Ok(m)
}

const STATS: usize = 26;

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
    }
}

impl Dragons {
    /// Every dragon, lair, event and table (see the module docs for what is left out). The
    /// commands not taken yet are written too.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(1024 + self.dragons.len() * 256);
        w.u32(MAGIC);
        w.u64(self.seed);
        w.u64(self.now.0);
        w.u64(self.next_seed);
        self.dragons.encode_with(&mut w, |x, w| put_dragon(w, x));
        w.var_u64(self.lairs.len() as u64);
        self.lairs.iter().for_each(|l| put_lair(&mut w, l));
        w.var_u64(self.lair_by_key.len() as u64);
        for (k, l) in &self.lair_by_key {
            w.u64(*k);
            w.u32(*l);
        }
        put_cells(&mut w, &self.lair_cells);
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        put_homes(&mut w, &self.homes);
        let l = &self.ledger;
        for m in [&l.taken, &l.eaten, &l.burnt, &l.hoarded, &l.robbed, &l.lost] {
            put_items128(&mut w, m);
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
                    s.encode(&mut w);
                }
                None => w.u8(0),
            }
        }
        put_cells(&mut w, &self.index.villages);
        put_cells(&mut w, &self.index.roads);
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
            put_item(&mut w, *item);
        }
        w.var_u64(self.herd_bodies.len() as u64);
        for (id, g) in &self.herd_bodies {
            w.u32(*id);
            w.u16(g.0);
        }
        w.into_bytes()
    }

    /// Reads what [`Dragons::save`] wrote, with the tunables the engine runs with.
    pub fn load(tun: DragonTunables, bytes: &[u8]) -> codec::Result<Dragons> {
        let r = &mut Reader::new(bytes);
        if r.u32()? != MAGIC {
            return bad("not a dragons page".into());
        }
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let next_seed = r.u64()?;
        let mut d = Dragons::new(seed, tun, now);
        d.next_seed = next_seed;
        d.dragons = Arena::decode_with(r, get_dragon)?;
        let n = r.len_prefix(30)?;
        d.lairs = (0..n).map(|_| get_lair(r)).collect::<codec::Result<_>>()?;
        for _ in 0..r.len_prefix(12)? {
            let k = r.u64()?;
            d.lair_by_key.insert(k, r.u32()?);
        }
        d.lair_cells = get_cells(r)?;
        d.wheel = TimingWheel::decode_with(r, get_ev)?;
        d.homes = get_homes(r)?;
        d.ledger = PreyLedger {
            taken: get_items128(r)?,
            eaten: get_items128(r)?,
            burnt: get_items128(r)?,
            hoarded: get_items128(r)?,
            robbed: get_items128(r)?,
            lost: get_items128(r)?,
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
            villages: get_cells(r)?,
            roads: get_cells(r)?,
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
        if !r.is_empty() {
            return bad(format!(
                "{} trailing bytes in a dragons page",
                r.remaining()
            ));
        }
        d.check_refs()?;
        Ok(d)
    }

    /// Lairs and dragons a corrupt page could name wrongly, checked once on load.
    fn check_refs(&self) -> codec::Result<()> {
        let nl = self.lairs.len() as u32;
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
        }
        let lairs_ok = self.lair_by_key.values().all(|&l| l < nl)
            && self.lair_cells.values().flatten().all(|&l| l < nl);
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
        Ok(())
    }
}
