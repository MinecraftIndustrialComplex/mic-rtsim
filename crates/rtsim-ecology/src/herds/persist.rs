// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving the herds (the dragons' pattern, docs/overnight-choices-persist.md S8): [`Herds::save`]
//! writes every herd (classes, condition, route, seen and dead members, its embodied group,
//! followers, ledger, booked legs), the grazed forage rows, the obstacle costs, the pending
//! events in pop order, the players, the sub-season's claims, the commands not taken and the op
//! ids applied, so [`Herds::load`] gives back an engine that continues exactly as the saved one
//! (the same `state_hash`, the same events in the same order).
//!
//! **Not saved:** the tunables and the climate provider (the caller's), the region index (derived
//! from each herd's own list, rebuilt on load), and the CLI's diagnostics (`log`, `trace`). The
//! leg index the herds book into is the caller's (the phase 1 world's), saved beside them.
//!
//! # Paged (docs/choices-paging-modules.md)
//!
//! The same state in two parts: the **meta** (everything but the rows: the wheel, players,
//! claims, obstacles, ops, the active set, the predator map and each row table's count) and the
//! **rows** (herds, forage cells, and the region index). [`Herds::save`] writes the meta then
//! every row in key order: one page, as before. [`Herds::save_paged`] writes the meta as one page
//! under the caller's key and only the row pages changed since the last save;
//! [`Herds::open_paged`] reads the meta and leaves every row page where it is. Both give the same
//! `state_hash`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::meta_pages::{MapTicket, PagedMap, PagedWheel, WheelTicket};
use rtsim_kernel::page::{PageKey, SaveBatch};
use rtsim_kernel::paging::SaveTicket;
use rtsim_kernel::rows::{PageSource, PagedArena, PagedRows, RowStats};
use rtsim_settle::Pos;
use rtsim_travel::legs::{Meeting, Stamp};

use super::boundary::{self, HerdDict};
use super::climate::Climate;
use super::cohort::Class;
use super::engine::{
    Ev, HERDS_PER_PAGE, HerdStats, Herds, PlayerView, REGIONS_PER_PAGE, TABLE_CLAIMS, TABLE_HERDS,
    TABLE_REGIONS, TABLE_WHEEL, player_key,
};
use super::forage::{CellKey, FORAGE_PER_PAGE, TABLE_FORAGE};
use super::herd::{Embodied, Herd, HerdId, HerdLedger, Route, Seen};
use super::route::Obstacle;
use super::species::HerdSpecies;
use super::tunables::HerdTunables;

const MAGIC: u32 = u32::from_le_bytes(*b"RTHD");
/// 2: the rows moved to row tables (key order, the herd's regions in its row), the active set and
/// the predator map are saved. 3: the wheel and the claims follow the rest of the meta, as blobs
/// or as page headers (docs/choices-paging-followups.md PF8). 4: a herd row is a range (its K,
/// range, home and vacancy, the ledger's dispersal), the region index holds each range's herd,
/// and the stats count renewals (EQUILIBRIUM.md S2; no migration before release).
const VERSION: u8 = 4;

/// The herds' page tables: the caller's meta key must be in none of them.
pub const TABLES: [u16; 5] = [
    TABLE_HERDS,
    TABLE_FORAGE,
    TABLE_REGIONS,
    TABLE_WHEEL,
    TABLE_CLAIMS,
];

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

fn put_claim(w: &mut Writer, k: &CellKey) {
    w.u16(k.0);
    w.var_i64(i64::from(k.1));
    w.var_i64(i64::from(k.2));
}

fn get_claim(r: &mut Reader<'_>) -> codec::Result<CellKey> {
    Ok((r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?))
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
        l.founded, l.born, l.died, l.starved, l.killed, l.eaten, l.taken, l.lost, l.arrived, l.left,
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
        arrived: r.var_u64()?,
        left: r.var_u64()?,
    })
}

fn put_herd(w: &mut Writer, x: &Herd) {
    w.bytes(x.name.as_bytes());
    w.u8(x.species.index() as u8);
    w.u64(x.seed);
    w.u16(x.frame);
    w.var_u64(x.k);
    w.u16(x.range.0);
    w.var_i64(i64::from(x.range.1));
    w.var_i64(i64::from(x.range.2));
    put_xz(w, x.home);
    put_opt_tick(w, x.vacant);
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
            w.u8(e.chart.k);
            w.i64(e.chart.tx);
            w.i64(e.chart.tz);
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
    w.var_u64(x.regions.len() as u64);
    for r in &x.regions {
        w.u16(r.0);
        w.var_i64(r.1);
        w.var_i64(r.2);
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
    let k = r.var_u64()?;
    let range = (r.u16()?, i32_of(r.var_i64()?)?, i32_of(r.var_i64()?)?);
    let home = get_xz(r)?;
    let vacant = get_opt_tick(r)?;
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
                chart: {
                    let k = r.u8()?;
                    let (tx, tz) = (r.i64()?, r.i64()?);
                    rtsim_space::topology::Iso::new(i64::from(k), tx, tz)
                },
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
    let mut regions = Vec::new();
    for _ in 0..r.len_prefix(4)? {
        regions.push((r.u16()?, r.var_i64()?, r.var_i64()?));
    }
    Ok(Herd {
        name,
        species,
        seed,
        frame,
        k,
        range,
        home,
        vacant,
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
        regions,
    })
}

impl Encode for Herd {
    fn encode(&self, w: &mut Writer) {
        put_herd(w, self);
    }
}

impl Decode for Herd {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        get_herd(r)
    }
}

impl rtsim_kernel::rows::Row for Herd {
    fn heap_bytes(&self) -> usize {
        Herd::heap_bytes(self)
    }
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
fn stats_array(s: &HerdStats) -> [u64; STATS] {
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
        s.vacated,
        s.frontier_foundings,
        s.frontier_postponed,
        s.dispersals,
        s.dispersed,
        s.recolonised,
    ]
}

/// Stats counters saved.
const STATS: usize = 29;

fn stats_of(v: [u64; STATS]) -> HerdStats {
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
        vacated: v[23],
        frontier_foundings: v[24],
        frontier_postponed: v[25],
        dispersals: v[26],
        dispersed: v[27],
        recolonised: v[28],
    }
}

/// Row counts the meta records, so a paged engine opens its tables without reading them.
struct Counts {
    herds: (u64, u32),
    forage: u64,
    regions: u64,
}

/// The tickets of one paged save: the wheel, the claims and one per row table.
#[derive(Debug)]
pub struct HerdsTicket {
    wheel: WheelTicket,
    claims: MapTicket,
    herds: SaveTicket,
    forage: SaveTicket,
    regions: SaveTicket,
}

/// How a budget of row bytes splits over the herds' tables: herd rows, forage cells, the region
/// index. PLACEHOLDER shares (docs/choices-paging-modules.md).
fn split(budget: usize) -> (usize, usize, usize) {
    let herds = budget / 2;
    let forage = budget / 100 * 35;
    (herds, forage, budget.saturating_sub(herds + forage))
}

fn page_err(e: rtsim_kernel::page::PageError) -> CodecError {
    CodecError::Invalid(format!("herds page: {e}"))
}

impl Herds {
    /// Everything but the rows.
    fn put_meta(&self, w: &mut Writer) {
        w.u32(MAGIC);
        w.u8(VERSION);
        w.u64(self.seed);
        w.u64(self.now.0);
        w.u64(self.next_seed);
        let (len, next) = self.herds.meta();
        w.var_u64(len);
        w.u32(next);
        w.var_u64(self.forage.rows.len());
        w.var_u64(self.regions.len());
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
                    s.encode(w);
                }
                None => w.u8(0),
            }
            w.var_u64(u64::from(p.puppets));
        }
        w.var_i64(self.claims_sub);
        let dict = HerdDict::default();
        let mut cmds = Vec::new();
        for c in self.outbox.iter() {
            boundary::encode_cmd(0, c, &dict, &mut cmds);
        }
        w.var_u64(self.outbox.len() as u64);
        w.bytes(&cmds);
        w.var_u64(self.ops.len() as u64);
        for (op, t) in &self.ops {
            w.u64(*op);
            w.u64(t.0);
        }
        w.var_u64(self.active.len() as u64);
        for h in &self.active {
            w.u32(h.index());
        }
        w.var_u64(self.follows.len() as u64);
        for (p, hs) in &self.follows {
            w.u64(*p);
            w.var_u64(hs.len() as u64);
            for h in hs {
                w.u32(h.index());
            }
        }
        stats_array(&self.stats).iter().for_each(|v| w.var_u64(*v));
    }

    /// An engine with the meta's state and empty row tables, and the tables' counts.
    fn get_meta(
        tun: HerdTunables,
        climate: Box<dyn Climate>,
        r: &mut Reader<'_>,
    ) -> codec::Result<(Herds, Counts)> {
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
        let counts = Counts {
            herds: (r.var_u64()?, r.u32()?),
            forage: r.var_u64()?,
            regions: r.var_u64()?,
        };
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
        h.claims_sub = r.var_i64()?;
        let n = r.len_prefix(0)?;
        let cmds = r.bytes()?;
        let dict = HerdDict::default();
        let mut b = cmds;
        for _ in 0..n {
            let (size, _, c) = boundary::decode_cmd(b, &dict)
                .map_err(|e| CodecError::Invalid(format!("outbox: {e:?}")))?;
            h.outbox.restore(c);
            b = &b[size..];
        }
        for _ in 0..r.len_prefix(16)? {
            let op = r.u64()?;
            h.ops.insert(op, Tick(r.u64()?));
        }
        for _ in 0..r.len_prefix(4)? {
            h.active.insert(PagedArena::<Herd>::handle(r.u32()?));
        }
        for _ in 0..r.len_prefix(9)? {
            let p = r.u64()?;
            let mut hs = BTreeSet::new();
            for _ in 0..r.len_prefix(4)? {
                hs.insert(PagedArena::<Herd>::handle(r.u32()?));
            }
            h.follows.insert(p, hs);
        }
        let mut s = [0u64; STATS];
        for v in &mut s {
            *v = r.var_u64()?;
        }
        h.stats = stats_of(s);
        Ok((h, counts))
    }

    /// The whole state as one page: the meta, then every row in key order.
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(1024 + self.herds.len() * 512);
        self.put_meta(&mut w);
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        w.var_u64(self.claims.len() as u64);
        for (k, v) in self.claims.iter() {
            put_claim(&mut w, k);
            w.u64(*v);
        }
        self.herds.encode_all(&mut w);
        self.forage.rows.encode_all(&mut w);
        w.into_bytes()
    }

    /// Reads [`Herds::save`]'s page with the caller's tunables and climate. The region index is
    /// rebuilt from the herds' own lists.
    pub fn load(
        tun: HerdTunables,
        climate: Box<dyn Climate>,
        bytes: &[u8],
    ) -> codec::Result<Herds> {
        let mut rd = Reader::new(bytes);
        let r = &mut rd;
        let (mut h, counts) = Self::get_meta(tun, climate, r)?;
        h.wheel = PagedWheel::decode_with(r, get_ev)?;
        for _ in 0..r.len_prefix(12)? {
            let k = get_claim(r)?;
            h.claims.insert(k, r.u64()?);
        }
        h.herds = PagedArena::decode_all(TABLE_HERDS, HERDS_PER_PAGE, r)?;
        h.forage.rows = PagedRows::decode_all(TABLE_FORAGE, FORAGE_PER_PAGE, r)?;
        rd.finish()?;
        if h.herds.meta() != counts.herds || h.forage.rows.len() != counts.forage {
            return bad("row counts differ from the meta's".into());
        }
        h.index_regions();
        if h.regions.len() != counts.regions {
            return bad("region index differs from the meta's count".into());
        }
        Ok(h)
    }

    // ---- paged ----------------------------------------------------------------------------------

    /// Starts paging the row tables through `source`, keeping about `budget` bytes of rows
    /// resident (split over the tables). Every row stays until a save writes it.
    pub fn attach_store(&mut self, source: Arc<dyn PageSource>, budget: usize) {
        let (a, b, c) = split(budget);
        self.herds.rows_mut().attach(source.clone(), a);
        self.forage.rows.attach(source.clone(), b);
        self.regions.attach(source, c);
    }

    /// Opens an engine [`Herds::save_paged`] saved under `meta_key`: the meta is read, no row is.
    pub fn open_paged(
        source: Arc<dyn PageSource>,
        budget: usize,
        meta_key: PageKey,
        tun: HerdTunables,
        climate: Box<dyn Climate>,
    ) -> codec::Result<Herds> {
        let raw = source
            .read(meta_key)
            .map_err(page_err)?
            .ok_or_else(|| CodecError::Invalid(format!("no herds meta page at {meta_key:?}")))?;
        let mut rd = Reader::new(&raw);
        let (mut h, counts) = Self::get_meta(tun, climate, &mut rd)?;
        h.wheel = PagedWheel::open_paged(&mut rd, TABLE_WHEEL, &*source, get_ev)?;
        h.claims = PagedMap::open_paged(&mut rd, TABLE_CLAIMS, &*source, get_claim, |r| r.u64())?;
        rd.finish()?;
        let (a, b, c) = split(budget);
        h.herds = PagedArena::open(TABLE_HERDS, HERDS_PER_PAGE, source.clone(), a, counts.herds);
        h.forage.rows = PagedRows::open(
            TABLE_FORAGE,
            FORAGE_PER_PAGE,
            source.clone(),
            b,
            counts.forage,
        );
        h.regions = PagedRows::open(TABLE_REGIONS, REGIONS_PER_PAGE, source, c, counts.regions);
        Ok(h)
    }

    /// The meta page (under `meta_key`, the caller's: in none of [`TABLES`]) and every row page
    /// changed since the last save, into `batch`. The pages stay pinned until the ticket comes
    /// back ([`Herds::saved`] once the commit is durable, [`Herds::save_failed`] if it failed).
    pub fn save_paged(&mut self, batch: &mut SaveBatch, meta_key: PageKey) -> HerdsTicket {
        assert!(
            !TABLES.contains(&meta_key.table()),
            "the herds' meta page {meta_key:?} inside one of their tables"
        );
        let mut w = Writer::with_capacity(1024);
        self.put_meta(&mut w);
        let wheel = self
            .wheel
            .save_paged(batch, TABLE_WHEEL, &mut w, |e, w| put_ev(w, e));
        let claims = self.claims.save_paged(
            batch,
            TABLE_CLAIMS,
            &mut w,
            |k, w| put_claim(w, k),
            |v, w| w.u64(*v),
        );
        batch.put(meta_key, w.as_bytes());
        HerdsTicket {
            wheel,
            claims,
            herds: self.herds.rows_mut().save_dirty(batch),
            forage: self.forage.rows.save_dirty(batch),
            regions: self.regions.save_dirty(batch),
        }
    }

    pub fn saved(&mut self, t: HerdsTicket) {
        self.wheel.saved(t.wheel);
        self.claims.saved(t.claims);
        self.herds.rows_mut().saved(t.herds);
        self.forage.rows.saved(t.forage);
        self.regions.saved(t.regions);
    }

    pub fn save_failed(&mut self, t: HerdsTicket) {
        self.wheel.save_failed(t.wheel);
        self.claims.save_failed(t.claims);
        self.herds.rows_mut().save_failed(t.herds);
        self.forage.rows.save_failed(t.forage);
        self.regions.save_failed(t.regions);
    }

    /// Evicts saved row pages down to the budget (a step boundary). Nothing in a resident engine.
    pub fn enforce_budget(&mut self) -> usize {
        self.herds.rows_mut().enforce_budget()
            + self.forage.rows.enforce_budget()
            + self.regions.enforce_budget()
    }

    /// Bytes of row pages resident now, over every row table: what the budget bounds.
    pub fn rows_resident_bytes(&self) -> usize {
        self.herds.rows().resident_bytes()
            + self.forage.rows.resident_bytes()
            + self.regions.resident_bytes()
    }

    /// The row budget over every table (`usize::MAX` resident).
    pub fn rows_budget(&self) -> usize {
        self.herds
            .rows()
            .budget()
            .saturating_add(self.forage.rows.budget())
            .saturating_add(self.regions.budget())
    }

    /// Whether the rows are over their budget with nothing left to evict: every resident page is
    /// changed since the last save. The host should save (an extra, pending save is a generation
    /// like any other, P29): the pages it writes can go.
    pub fn needs_save(&self) -> bool {
        self.herds.rows().is_paged() && self.rows_resident_bytes() > self.rows_budget()
    }

    pub fn reset_row_stats(&mut self) {
        self.herds.rows_mut().reset_stats();
        self.forage.rows.reset_stats();
        self.regions.reset_stats();
    }

    /// The row tables' counters, summed.
    pub fn row_stats(&self) -> RowStats {
        let mut out = RowStats::default();
        for s in [
            self.herds.rows().stats(),
            self.forage.rows.stats(),
            self.regions.stats(),
        ] {
            out.loads += s.loads;
            out.load_bytes += s.load_bytes;
            out.evictions += s.evictions;
            out.over_budget += s.over_budget;
            out.peak_resident_bytes += s.peak_resident_bytes;
        }
        out
    }
}
