// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the herds (the dragons' pattern, docs/overnight-choices-persist.md S8): [`Herds::save`]
//! writes every herd (classes, condition, route, seen and dead members, its embodied group,
//! followers, ledger, booked legs), the grazed forage rows, the obstacle costs, the pending
//! events in pop order, the players, the sub-season's claims, the commands not taken and the op
//! ids applied, so [`Herds::load`] gives back an engine that continues exactly as the saved one
//! (the same `state_hash`, the same events in the same order).
//!
//! **Not saved:** the tunables and the climate provider (the caller's), the region index and the
//! predator → herd map (derived from the routes and followers, rebuilt on load), and the CLI's
//! diagnostics (`log`, `trace`). The leg index the herds book into is the caller's (the phase 1
//! world's), saved beside them.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::Arena;
use rtsim_kernel::wheel::TimingWheel;
use rtsim_settle::Pos;
use rtsim_travel::legs::{Meeting, Stamp};

use super::boundary::{self, HerdDict};
use super::climate::Climate;
use super::cohort::Class;
use super::engine::{Ev, HerdStats, Herds, PlayerView, player_key};
use super::forage::Row;
use super::herd::{Embodied, Herd, HerdId, HerdLedger, Route, Seen};
use super::route::Obstacle;
use super::species::HerdSpecies;
use super::tunables::HerdTunables;

const MAGIC: u32 = u32::from_le_bytes(*b"RTHD");
const VERSION: u8 = 1;

fn bad<T>(what: String) -> codec::Result<T> {
    Err(CodecError::Invalid(what))
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

fn u32_of(v: u64) -> codec::Result<u32> {
    u32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} over u32")))
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

fn put_xz(w: &mut Writer, p: (i32, i32)) {
    w.var_i64(i64::from(p.0));
    w.var_i64(i64::from(p.1));
}

fn get_xz(r: &mut Reader<'_>) -> codec::Result<(i32, i32)> {
    Ok((i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?))
}

fn put_class(w: &mut Writer, c: &Class) {
    w.var_u64(c.first);
    w.var_u64(u64::from(c.born));
    w.var_i64(c.birth_day);
    w.var_i64(c.anchor_day);
    w.var_u64(u64::from(c.alive));
    w.var_u64(u64::from(c.generation));
}

fn get_class(r: &mut Reader<'_>) -> codec::Result<Class> {
    let c = Class {
        first: r.var_u64()?,
        born: u32_of(r.var_u64()?)?,
        birth_day: r.var_i64()?,
        anchor_day: r.var_i64()?,
        alive: u32_of(r.var_u64()?)?,
        generation: u32_of(r.var_u64()?)?,
    };
    if c.alive > c.born || c.anchor_day < c.birth_day {
        return bad(format!("class {c:?}"));
    }
    Ok(c)
}

fn put_route(w: &mut Writer, x: &Route) {
    w.var_u64(x.points.len() as u64);
    for (p, t) in x.points.iter().zip(&x.times) {
        put_xz(w, *p);
        w.u64(t.0);
    }
    w.u64(x.until.0);
}

fn get_route(r: &mut Reader<'_>) -> codec::Result<Route> {
    let n = r.len_prefix(10)?;
    let mut points = Vec::with_capacity(n);
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        points.push(get_xz(r)?);
        times.push(Tick(r.u64()?));
    }
    if times.windows(2).any(|w| w[1] < w[0]) {
        return bad("route times go back".into());
    }
    Ok(Route {
        points,
        times,
        until: Tick(r.u64()?),
    })
}

fn put_ledger(w: &mut Writer, l: &HerdLedger) {
    for v in [
        l.founded, l.born, l.died, l.starved, l.killed, l.eaten, l.taken, l.lost,
    ] {
        w.var_u64(v);
    }
}

fn get_ledger(r: &mut Reader<'_>) -> codec::Result<HerdLedger> {
    Ok(HerdLedger {
        founded: r.var_u64()?,
        born: r.var_u64()?,
        died: r.var_u64()?,
        starved: r.var_u64()?,
        killed: r.var_u64()?,
        eaten: r.var_u64()?,
        taken: r.var_u64()?,
        lost: r.var_u64()?,
    })
}

fn put_herd(w: &mut Writer, x: &Herd) {
    w.bytes(x.name.as_bytes());
    w.u8(x.species.index() as u8);
    w.u64(x.seed);
    w.u16(x.frame);
    w.var_u64(x.classes.len() as u64);
    x.classes.iter().for_each(|c| put_class(w, c));
    w.var_u64(x.next_index);
    x.condition.iter().for_each(|c| w.u16(*c));
    w.u16(x.satisfaction);
    put_route(w, &x.route);
    w.var_u64(u64::from(x.reach));
    w.var_u64(u64::from(x.plan));
    w.var_i64(x.bred_year);
    w.u64(x.decided.0);
    w.u64(x.next_decide.0);
    w.var_u64(x.seen.len() as u64);
    for (k, s) in &x.seen {
        w.var_u64(*k);
        w.u64(s.last.0);
        put_opt_tick(w, s.sheared_at);
    }
    w.var_u64(x.dead.len() as u64);
    x.dead.iter().for_each(|k| w.var_u64(*k));
    match &x.embodied {
        Some(e) => {
            w.u8(1);
            w.var_u64(u64::from(e.epoch));
            w.var_u64(u64::from(e.player));
            w.var_u64(e.members.len() as u64);
            e.members.iter().for_each(|k| w.var_u64(*k));
            w.u64(e.since.0);
            put_xz(w, e.steer);
            w.bool(e.leaving);
        }
        None => w.u8(0),
    }
    w.var_u64(u64::from(x.epoch));
    w.var_u64(x.followers.len() as u64);
    for (p, t) in &x.followers {
        w.u64(*p);
        w.u64(t.0);
    }
    put_ledger(w, &x.ledger);
    w.var_u64(x.clips.len() as u64);
    for (slot, s) in &x.clips {
        w.var_u64(u64::from(*slot));
        s.encode(w);
    }
}

fn get_herd(r: &mut Reader<'_>) -> codec::Result<Herd> {
    let name = String::from_utf8(r.bytes()?.to_vec())
        .map_err(|_| CodecError::Invalid("herd name".into()))?;
    let s = r.u8()?;
    let species =
        HerdSpecies::from_index(u32::from(s)).map_or_else(|| bad(format!("species {s}")), Ok)?;
    let seed = r.u64()?;
    let frame = r.u16()?;
    let classes = (0..r.len_prefix(6)?)
        .map(|_| get_class(r))
        .collect::<codec::Result<Vec<_>>>()?;
    let next_index = r.var_u64()?;
    let condition = [r.u16()?, r.u16()?, r.u16()?];
    let satisfaction = r.u16()?;
    let route = get_route(r)?;
    let reach = u32_of(r.var_u64()?)?;
    let plan = u32_of(r.var_u64()?)?;
    let bred_year = r.var_i64()?;
    let decided = Tick(r.u64()?);
    let next_decide = Tick(r.u64()?);
    let mut seen = BTreeMap::new();
    for _ in 0..r.len_prefix(10)? {
        let k = r.var_u64()?;
        let last = Tick(r.u64()?);
        seen.insert(
            k,
            Seen {
                last,
                sheared_at: get_opt_tick(r)?,
            },
        );
    }
    let mut dead = BTreeSet::new();
    for _ in 0..r.len_prefix(1)? {
        dead.insert(r.var_u64()?);
    }
    let embodied = match r.u8()? {
        0 => None,
        1 => {
            let epoch = u32_of(r.var_u64()?)?;
            let player = u32_of(r.var_u64()?)?;
            let members = (0..r.len_prefix(1)?)
                .map(|_| r.var_u64())
                .collect::<codec::Result<Vec<_>>>()?;
            Some(Embodied {
                epoch,
                player,
                members,
                since: Tick(r.u64()?),
                steer: get_xz(r)?,
                leaving: r.bool()?,
            })
        }
        t => return bad(format!("embodied tag {t}")),
    };
    let epoch = u32_of(r.var_u64()?)?;
    let mut followers = Vec::new();
    for _ in 0..r.len_prefix(16)? {
        let p = r.u64()?;
        followers.push((p, Tick(r.u64()?)));
    }
    let ledger = get_ledger(r)?;
    let mut clips = BTreeMap::new();
    for _ in 0..r.len_prefix(2)? {
        let slot = u32_of(r.var_u64()?)?;
        clips.insert(slot, Stamp::decode(r)?);
    }
    Ok(Herd {
        name,
        species,
        seed,
        frame,
        classes,
        next_index,
        condition,
        satisfaction,
        route,
        reach,
        plan,
        bred_year,
        decided,
        next_decide,
        seen,
        dead,
        embodied,
        epoch,
        followers,
        ledger,
        clips,
    })
}

fn put_ev(w: &mut Writer, e: &Ev) {
    match e {
        Ev::Decide { h } => {
            w.u8(0);
            w.u64(h.to_bits());
        }
        Ev::Seen { h, player, meeting } => {
            w.u8(1);
            w.u64(h.to_bits());
            w.var_u64(u64::from(*player));
            w.u64(meeting.at.0);
            meeting.a.encode(w);
            meeting.b.encode(w);
        }
        Ev::Check { h, epoch } => {
            w.u8(2);
            w.u64(h.to_bits());
            w.var_u64(u64::from(*epoch));
        }
        Ev::Year => w.u8(3),
    }
}

fn get_ev(r: &mut Reader<'_>) -> codec::Result<Ev> {
    Ok(match r.u8()? {
        0 => Ev::Decide {
            h: HerdId::from_bits(r.u64()?),
        },
        1 => Ev::Seen {
            h: HerdId::from_bits(r.u64()?),
            player: u32_of(r.var_u64()?)?,
            meeting: Meeting {
                at: Tick(r.u64()?),
                a: Stamp::decode(r)?,
                b: Stamp::decode(r)?,
            },
        },
        2 => Ev::Check {
            h: HerdId::from_bits(r.u64()?),
            epoch: u32_of(r.var_u64()?)?,
        },
        3 => Ev::Year,
        t => return bad(format!("event tag {t}")),
    })
}

/// Every stats counter in field order.
fn stats_array(s: &HerdStats) -> [u64; 23] {
    [
        s.events,
        s.stale,
        s.decisions,
        s.cells_reached,
        s.breedings,
        s.starvations,
        s.clips,
        s.seen,
        s.seen_stale,
        s.seen_empty,
        s.materialized,
        s.despawns,
        s.captures,
        s.rejected,
        s.steers,
        s.killed,
        s.sheared,
        s.taken,
        s.obstacles,
        s.replans,
        s.predations,
        s.duplicate_ops,
        s.late_records,
    ]
}

fn stats_of(v: [u64; 23]) -> HerdStats {
    HerdStats {
        events: v[0],
        stale: v[1],
        decisions: v[2],
        cells_reached: v[3],
        breedings: v[4],
        starvations: v[5],
        clips: v[6],
        seen: v[7],
        seen_stale: v[8],
        seen_empty: v[9],
        materialized: v[10],
        despawns: v[11],
        captures: v[12],
        rejected: v[13],
        steers: v[14],
        killed: v[15],
        sheared: v[16],
        taken: v[17],
        obstacles: v[18],
        replans: v[19],
        predations: v[20],
        duplicate_ops: v[21],
        late_records: v[22],
    }
}

impl Herds {
    /// The whole state as one page.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(1024 + self.herds.len() * 512);
        w.u32(MAGIC);
        w.u8(VERSION);
        w.u64(self.seed);
        w.u64(self.now.0);
        w.u64(self.next_seed);
        self.herds.encode_with(&mut w, |x, w| put_herd(w, x));
        w.var_u64(self.forage.rows.len() as u64);
        for (k, row) in &self.forage.rows {
            w.u16(k.0);
            w.var_i64(i64::from(k.1));
            w.var_i64(i64::from(k.2));
            w.u64(row.anchor.0);
            w.f64(row.forage);
        }
        let fs = &self.forage.stats;
        for v in [fs.reads, fs.grazed, fs.folded] {
            w.var_u64(v);
        }
        w.var_u64(self.obstacles.cells.len() as u64);
        for (k, o) in &self.obstacles.cells {
            w.u16(k.0);
            w.var_i64(i64::from(k.1));
            w.var_i64(i64::from(k.2));
            w.f64(o.extra);
            w.u64(o.at.0);
            w.var_u64(u64::from(o.reports));
        }
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        w.var_u64(self.players.len() as u64);
        for p in &self.players {
            w.u64(p.uuid.0);
            w.u64(p.uuid.1);
            w.u64(p.actor);
            match p.pos {
                Some((pos, t)) => {
                    w.u8(1);
                    w.u16(pos.frame);
                    w.var_i64(i64::from(pos.x));
                    w.var_i64(i64::from(pos.z));
                    w.u64(t.0);
                }
                None => w.u8(0),
            }
            match p.interest {
                Some(s) => {
                    w.u8(1);
                    s.encode(&mut w);
                }
                None => w.u8(0),
            }
            w.var_u64(u64::from(p.puppets));
        }
        w.var_u64(self.claims.len() as u64);
        for (k, v) in &self.claims {
            w.u16(k.0);
            w.var_i64(i64::from(k.1));
            w.var_i64(i64::from(k.2));
            w.u64(*v);
        }
        w.var_i64(self.claims_sub);
        let dict = HerdDict::default();
        let mut cmds = Vec::new();
        for c in &self.outbox {
            boundary::encode_cmd(0, c, &dict, &mut cmds);
        }
        w.var_u64(self.outbox.len() as u64);
        w.bytes(&cmds);
        w.var_u64(self.ops.len() as u64);
        for (op, t) in &self.ops {
            w.u64(*op);
            w.u64(t.0);
        }
        stats_array(&self.stats).iter().for_each(|v| w.var_u64(*v));
        w.into_bytes()
    }

    /// Reads [`Herds::save`]'s page with the caller's tunables and climate.
    pub fn load(
        tun: HerdTunables,
        climate: Box<dyn Climate>,
        bytes: &[u8],
    ) -> codec::Result<Herds> {
        let mut rd = Reader::new(bytes);
        let r = &mut rd;
        if r.u32()? != MAGIC {
            return bad("not a herds page".into());
        }
        let v = r.u8()?;
        if v != VERSION {
            return bad(format!("herds page version {v}"));
        }
        let seed = r.u64()?;
        let now = Tick(r.u64()?);
        let next_seed = r.u64()?;
        let mut h = Herds::new(seed, tun, climate, now);
        h.next_seed = next_seed;
        h.herds = Arena::decode_with(r, get_herd)?;
        for _ in 0..r.len_prefix(14)? {
            let k = (r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?);
            let anchor = Tick(r.u64()?);
            h.forage.rows.insert(
                k,
                Row {
                    anchor,
                    forage: r.f64()?,
                },
            );
        }
        h.forage.stats.reads = r.var_u64()?;
        h.forage.stats.grazed = r.var_u64()?;
        h.forage.stats.folded = r.var_u64()?;
        for _ in 0..r.len_prefix(20)? {
            let k = (r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?);
            let extra = r.f64()?;
            let at = Tick(r.u64()?);
            h.obstacles.cells.insert(
                k,
                Obstacle {
                    extra,
                    at,
                    reports: u32_of(r.var_u64()?)?,
                },
            );
        }
        h.wheel = TimingWheel::decode_with(r, get_ev)?;
        for _ in 0..r.len_prefix(27)? {
            let uuid = (r.u64()?, r.u64()?);
            let actor = r.u64()?;
            let pos = match r.u8()? {
                0 => None,
                1 => {
                    let frame = r.u16()?;
                    let x = i32_of(r.var_i64()?)?;
                    let z = i32_of(r.var_i64()?)?;
                    Some((Pos::new(frame, x, z), Tick(r.u64()?)))
                }
                t => return bad(format!("pos tag {t}")),
            };
            let interest = match r.u8()? {
                0 => None,
                1 => Some(Stamp::decode(r)?),
                t => return bad(format!("interest tag {t}")),
            };
            let puppets = u32_of(r.var_u64()?)?;
            let slot = h.players.len() as u32;
            h.player_by_key.insert(player_key(uuid), slot);
            h.players.push(PlayerView {
                uuid,
                actor,
                pos,
                interest,
                puppets,
            });
        }
        for _ in 0..r.len_prefix(12)? {
            let k = (r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?);
            h.claims.insert(k, r.u64()?);
        }
        h.claims_sub = r.var_i64()?;
        let n = r.len_prefix(0)?;
        let cmds = r.bytes()?;
        let dict = HerdDict::default();
        let mut b = cmds;
        for _ in 0..n {
            let (size, _, c) = boundary::decode_cmd(b, &dict)
                .map_err(|e| CodecError::Invalid(format!("outbox: {e:?}")))?;
            h.outbox.push(c);
            b = &b[size..];
        }
        for _ in 0..r.len_prefix(16)? {
            let op = r.u64()?;
            h.ops.insert(op, Tick(r.u64()?));
        }
        let mut s = [0u64; 23];
        for v in &mut s {
            *v = r.var_u64()?;
        }
        h.stats = stats_of(s);
        rd.finish()?;
        // Derived indexes.
        h.rebuild_regions();
        let pairs: Vec<(u64, HerdId)> = h
            .herds
            .iter()
            .flat_map(|(id, x)| x.followers.iter().map(move |(p, _)| (*p, id)))
            .collect();
        for (p, id) in pairs {
            h.follows.entry(p).or_default().insert(id);
        }
        Ok(h)
    }
}
