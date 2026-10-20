// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world's snapshot (docs/overnight-choices-persist.md, S1 to S4): every table, wheel
//! and index, so a world read back with [`World::load`] continues exactly as the one saved
//! (the same `state_hash` after the same inputs, bit for bit) and costs no replay.
//!
//! **Not written:** the configuration, the goods registry and the recipe book (the caller builds
//! the world from them and passes the same ones to [`World::load`]; their sizes are checked), and
//! the market trace (diagnostics, off in play).

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Decode, Encode, Reader, Writer};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::lazy::LazyTable;
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::board::{BoardCursor, Packet};
use rtsim_knowledge::{GossipType, Knowledge, ReportKind, VanillaGossip};
use rtsim_settle::{NodeKind, Pos, Settlements};
use rtsim_trade::labour::{Costs, Lint, Local, RecipeBook};
use rtsim_trade::persist::{get_opt_good, load_market, put_opt_good, save_market};
use rtsim_trade::{GoodId, Goods};
use rtsim_travel::caravan::{Body, Caravan, CaravanKey};
use rtsim_travel::edge::{DirRef, Edge};
use rtsim_travel::persist::{SnapDec, SnapEnc};
use rtsim_travel::{LegIndex, NewsRing, RoadIndex};

use crate::bases::BaseState;
use crate::config::WorldConfig;
use crate::interest::PlayerState;
use crate::ribbits::{Band, Growth, PlaceHouse, SentimentSnapshot, SpawnResident};
use crate::world::{Command, Ev, Node, Outside, PacketData, Stats, Via, World};

const MAGIC: u32 = u32::from_le_bytes(*b"RTW1");

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

fn handle<T>(r: &mut Reader<'_>) -> codec::Result<Handle<T>> {
    Ok(Handle::from_bits(r.u64()?))
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

fn put_opt_pending(w: &mut Writer, p: Option<(u64, u32)>) {
    match p {
        Some((a, b)) => {
            w.u8(1);
            w.var_u64(a);
            w.u32(b);
        }
        None => w.u8(0),
    }
}

fn get_opt_pending(r: &mut Reader<'_>) -> codec::Result<Option<(u64, u32)>> {
    Ok(match r.u8()? {
        0 => None,
        1 => Some((r.var_u64()?, r.u32()?)),
        t => return bad(format!("option tag {t}")),
    })
}

fn put_dirs(w: &mut Writer, v: &[DirRef]) {
    put_usize(w, v.len());
    v.iter().for_each(|d| d.encode(w));
}

fn get_dirs(r: &mut Reader<'_>) -> codec::Result<Vec<DirRef>> {
    let n = r.len_prefix(2)?;
    (0..n).map(|_| DirRef::decode(r)).collect()
}

fn put_usizes(w: &mut Writer, v: &[usize]) {
    put_usize(w, v.len());
    v.iter().for_each(|x| put_usize(w, *x));
}

fn get_usizes(r: &mut Reader<'_>) -> codec::Result<Vec<usize>> {
    let n = r.len_prefix(1)?;
    (0..n).map(|_| get_usize(r)).collect()
}

fn put_node(w: &mut Writer, n: &Node, snaps: &mut SnapEnc) {
    w.u64(n.settlement.to_bits());
    w.u64(n.key);
    n.kind.encode(w);
    put_opt_good(w, n.currency);
    n.pos.encode(w);
    w.u64(n.actor.to_bits());
    w.u64(n.region.to_bits());
    n.news.save(w, snaps);
    put_dirs(w, &n.out);
    put_dirs(w, &n.raids_in);
    put_usizes(w, &n.neighbours);
    put_usize(w, n.bands.len());
    for (b, s) in &n.bands {
        w.var_u64(u64::from(*b));
        w.var_u64(u64::from(*s));
    }
    snaps.put_opt(w, &n.last_sent);
    n.costs.encode(w);
    n.local.encode(w);
}

fn get_node(r: &mut Reader<'_>, snaps: &mut SnapDec) -> codec::Result<Node> {
    let settlement = handle(r)?;
    let key = r.u64()?;
    let kind = NodeKind::decode(r)?;
    let currency = get_opt_good(r)?;
    let pos = Pos::decode(r)?;
    let actor = handle(r)?;
    let region = handle(r)?;
    let news = NewsRing::load(r, snaps)?;
    let out = get_dirs(r)?;
    let raids_in = get_dirs(r)?;
    let neighbours = get_usizes(r)?;
    let n = r.len_prefix(2)?;
    let mut bands = Vec::with_capacity(n);
    for _ in 0..n {
        bands.push((get_u32(r)?, get_u32(r)?));
    }
    let last_sent = snaps.get_opt(r)?;
    Ok(Node {
        settlement,
        key,
        kind,
        currency,
        pos,
        actor,
        region,
        news,
        out,
        raids_in,
        neighbours,
        bands,
        last_sent,
        costs: Costs::decode(r)?,
        local: Local::decode(r)?,
    })
}

fn put_ev(w: &mut Writer, e: &Ev) {
    match *e {
        Ev::Wake(n) => {
            w.u8(0);
            put_usize(w, n);
        }
        Ev::Deliver(p) => {
            w.u8(1);
            w.var_u64(u64::from(p));
        }
        Ev::Arrive {
            caravan,
            generation,
        } => {
            w.u8(2);
            w.u64(caravan.to_bits());
            w.u32(generation);
        }
        Ev::Grow { node, stamp } => {
            w.u8(3);
            put_usize(w, node);
            w.u32(stamp);
        }
    }
}

fn get_ev(r: &mut Reader<'_>) -> codec::Result<Ev> {
    Ok(match r.u8()? {
        0 => Ev::Wake(get_usize(r)?),
        1 => Ev::Deliver(get_u32(r)?),
        2 => Ev::Arrive {
            caravan: handle(r)?,
            generation: r.u32()?,
        },
        3 => Ev::Grow {
            node: get_usize(r)?,
            stamp: r.u32()?,
        },
        t => return bad(format!("world event {t}")),
    })
}

fn put_packet(w: &mut Writer, p: &PacketData, snaps: &mut SnapEnc) {
    match p.via {
        Via::Road(d) => {
            w.u8(0);
            d.encode(w);
        }
        Via::Band { band, stop } => {
            w.u8(1);
            w.var_u64(u64::from(band));
            w.var_u64(u64::from(stop));
        }
    }
    put_usize(w, p.from);
    put_usize(w, p.to);
    w.var_u64(p.departs);
    snaps.put_opt(w, &p.own);
    put_usize(w, p.relays.len());
    for s in &p.relays {
        snaps.put(w, s);
    }
    put_usize(w, p.reports.reports.len());
    p.reports.reports.iter().for_each(|r| w.u64(r.to_bits()));
}

fn get_packet(r: &mut Reader<'_>, snaps: &mut SnapDec) -> codec::Result<PacketData> {
    let via = match r.u8()? {
        0 => Via::Road(DirRef::decode(r)?),
        1 => Via::Band {
            band: get_u32(r)?,
            stop: get_u32(r)?,
        },
        t => return bad(format!("via {t}")),
    };
    let from = get_usize(r)?;
    let to = get_usize(r)?;
    let departs = r.var_u64()?;
    let own = snaps.get_opt(r)?;
    let n = r.len_prefix(2)?;
    let relays = (0..n).map(|_| snaps.get(r)).collect::<codec::Result<_>>()?;
    let n = r.len_prefix(8)?;
    let reports = (0..n).map(|_| handle(r)).collect::<codec::Result<_>>()?;
    Ok(PacketData {
        via,
        from,
        to,
        departs,
        own,
        relays,
        reports: Packet { reports },
    })
}

fn put_body(w: &mut Writer, b: Body) {
    b.encode(w);
}

fn put_command(w: &mut Writer, c: &Command) {
    match c {
        Command::VanillaGossip(g) => {
            w.u8(0);
            w.u64(g.village.to_bits());
            w.u64(g.player.to_bits());
            w.u8(g.gossip as u8);
            w.var_i64(i64::from(g.amount));
            w.u64(g.report.to_bits());
            w.u16(g.kind as u16);
            w.u64(g.at.0);
            w.bool(g.first_hand);
        }
        Command::Sentiment(s) => {
            w.u8(1);
            w.u64(s.village);
            w.u8(s.dislike_at_or_below as u8);
            w.u8(s.friend_at_or_above as u8);
            w.u64(s.at.0);
            put_usize(w, s.entries.len());
            for (a, b, v) in &s.entries {
                w.u64(*a);
                w.u64(*b);
                w.u8(*v as u8);
            }
        }
        Command::PlaceHouse(h) => {
            w.u8(2);
            w.u64(h.op);
            w.u64(h.village);
            w.u16(h.frame);
            w.u32(h.template);
            w.var_i64(i64::from(h.x));
            w.var_i64(i64::from(h.z));
            w.u32(h.radius);
            w.u8(h.rotation);
            w.u64(h.seed);
        }
        Command::SpawnResident(s) => {
            w.u8(3);
            w.u64(s.op);
            w.u64(s.village);
            w.u64(s.house);
            put_body(w, s.body);
        }
    }
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

fn get_command(r: &mut Reader<'_>) -> codec::Result<Command> {
    Ok(match r.u8()? {
        0 => {
            let village = handle(r)?;
            let player = handle(r)?;
            let gossip = match r.u8()? {
                1 => GossipType::MajorNegative,
                2 => GossipType::MinorNegative,
                3 => GossipType::MinorPositive,
                4 => GossipType::MajorPositive,
                5 => GossipType::Trading,
                t => return bad(format!("gossip type {t}")),
            };
            let amount = i32_of(r.var_i64()?)?;
            let report = handle(r)?;
            let k = r.u16()?;
            let kind =
                ReportKind::from_u16(k).map_or_else(|| bad(format!("report kind {k}")), Ok)?;
            Command::VanillaGossip(VanillaGossip {
                village,
                player,
                gossip,
                amount,
                report,
                kind,
                at: Tick(r.u64()?),
                first_hand: r.bool()?,
            })
        }
        1 => {
            let village = r.u64()?;
            let dislike_at_or_below = r.u8()? as i8;
            let friend_at_or_above = r.u8()? as i8;
            let at = Tick(r.u64()?);
            let n = r.len_prefix(17)?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((r.u64()?, r.u64()?, r.u8()? as i8));
            }
            Command::Sentiment(SentimentSnapshot {
                village,
                dislike_at_or_below,
                friend_at_or_above,
                at,
                entries,
            })
        }
        2 => Command::PlaceHouse(PlaceHouse {
            op: r.u64()?,
            village: r.u64()?,
            frame: r.u16()?,
            template: r.u32()?,
            x: i32_of(r.var_i64()?)?,
            z: i32_of(r.var_i64()?)?,
            radius: r.u32()?,
            rotation: r.u8()?,
            seed: r.u64()?,
        }),
        3 => Command::SpawnResident(SpawnResident {
            op: r.u64()?,
            village: r.u64()?,
            house: r.u64()?,
            body: Body::decode(r)?,
        }),
        t => return bad(format!("command {t}")),
    })
}

fn put_band(w: &mut Writer, b: &Band) {
    put_usize(w, b.home);
    w.u64(b.seed);
    put_usizes(w, &b.route);
    put_usize(w, b.walks.len());
    b.walks.iter().for_each(|x| w.var_u64(*x));
    w.var_u64(b.stay);
    w.var_u64(b.start);
    w.u8(b.members);
    put_usize(w, b.cursors.len());
    b.cursors.iter().for_each(|c| w.var_u64(c.0));
    put_usize(w, b.pending.len());
    b.pending.iter().for_each(|p| put_opt_pending(w, *p));
    put_usize(w, b.relayed.len());
    b.relayed.iter().for_each(|t| w.u64(t.0));
}

fn get_band(r: &mut Reader<'_>) -> codec::Result<Band> {
    let home = get_usize(r)?;
    let seed = r.u64()?;
    let route = get_usizes(r)?;
    let n = r.len_prefix(1)?;
    let walks = (0..n).map(|_| r.var_u64()).collect::<codec::Result<_>>()?;
    let stay = r.var_u64()?;
    let start = r.var_u64()?;
    let members = r.u8()?;
    let n = r.len_prefix(1)?;
    let cursors = (0..n)
        .map(|_| r.var_u64().map(BoardCursor))
        .collect::<codec::Result<_>>()?;
    let n = r.len_prefix(1)?;
    let pending = (0..n)
        .map(|_| get_opt_pending(r))
        .collect::<codec::Result<_>>()?;
    let n = r.len_prefix(8)?;
    let relayed = (0..n)
        .map(|_| r.u64().map(Tick))
        .collect::<codec::Result<_>>()?;
    Ok(Band {
        home,
        seed,
        route,
        walks,
        stay,
        start,
        members,
        cursors,
        pending,
        relayed,
    })
}

fn put_i128_map(w: &mut Writer, m: &BTreeMap<GoodId, i128>) {
    put_usize(w, m.len());
    for (g, v) in m {
        w.u16(g.0);
        w.i128(*v);
    }
}

fn get_i128_map(r: &mut Reader<'_>) -> codec::Result<BTreeMap<GoodId, i128>> {
    let mut m = BTreeMap::new();
    for _ in 0..r.len_prefix(18)? {
        m.insert(GoodId(r.u16()?), r.i128()?);
    }
    Ok(m)
}

impl World {
    /// The whole world as bytes (see the module docs for what is left out).
    pub fn save(&self) -> Vec<u8> {
        let mut w = Writer::with_capacity(4096 + self.nodes.len() * 8192);
        w.u32(MAGIC);
        put_usize(&mut w, self.goods.len());
        put_usize(&mut w, self.book.recipes.len());
        w.u64(self.now.0);
        self.settlements.encode(&mut w);
        let mut snaps = SnapEnc::new();
        put_usize(&mut w, self.nodes.len());
        for n in &self.nodes {
            put_node(&mut w, n, &mut snaps);
        }
        self.markets.encode_with(&mut w, |m, w| save_market(w, m));
        put_usize(&mut w, self.edges.len());
        self.edges.iter().for_each(|e| e.encode(&mut w));
        self.knowledge.save(&mut w);
        self.wheel.encode_with(&mut w, |e, w| put_ev(w, e));
        put_usize(&mut w, self.packets.len());
        for (k, p) in &self.packets {
            w.var_u64(u64::from(*k));
            put_packet(&mut w, p, &mut snaps);
        }
        put_usize(&mut w, self.wakes.len());
        for (t, n) in &self.wakes {
            w.u64(t.0);
            put_usize(&mut w, *n);
        }
        w.u32(self.next_packet);
        self.caravans.encode_with(&mut w, |c, w| c.encode(w));
        self.legs.encode(&mut w);
        put_usize(&mut w, self.leg_ids.len());
        for (c, l) in &self.leg_ids {
            w.u64(c.to_bits());
            w.u64(l.to_bits());
        }
        put_i128_map(&mut w, &self.outside.player_in);
        put_i128_map(&mut w, &self.outside.player_out);
        put_i128_map(&mut w, &self.outside.destroyed);
        put_i128_map(&mut w, &self.outside.held);
        let s = &self.stats;
        for v in [
            s.market_events,
            s.breakpoints,
            s.wakes,
            s.deliveries,
            s.packets_sent,
            s.segments,
            s.caravan_arrivals,
            s.promotions,
            s.player_trades,
            s.houses,
            s.band_packets,
        ] {
            w.var_u64(v);
        }
        put_usize(&mut w, self.outbox.len());
        self.outbox.iter().for_each(|c| put_command(&mut w, c));
        put_usize(&mut w, self.lints.len());
        self.lints.iter().for_each(|l| l.encode(&mut w));
        w.f64(self.mint_cost[0]);
        w.f64(self.mint_cost[1]);
        put_usize(&mut w, self.players.len());
        for (a, p) in &self.players {
            w.u64(a.to_bits());
            w.u64(p.uuid.0);
            w.u64(p.uuid.1);
            match p.pos {
                Some((pos, t)) => {
                    w.u8(1);
                    pos.encode(&mut w);
                    w.u64(t.0);
                }
                None => w.u8(0),
            }
            w.u64(p.interest.map_or(0, |l| l.to_bits()));
        }
        self.roads.encode(&mut w);
        put_usize(&mut w, self.bands.len());
        self.bands.iter().for_each(|b| put_band(&mut w, b));
        put_usize(&mut w, self.growth.len());
        for (n, g) in &self.growth {
            put_usize(&mut w, *n);
            put_opt_tick(&mut w, g.since);
            w.u32(g.stamp);
            w.u32(g.houses);
        }
        put_usize(&mut w, self.houses_pending.len());
        for (op, n) in &self.houses_pending {
            w.u64(*op);
            put_usize(&mut w, *n);
        }
        put_usize(&mut w, self.sentiment_dirty.len());
        self.sentiment_dirty
            .iter()
            .for_each(|n| put_usize(&mut w, *n));
        w.u64(self.op_counter);
        put_usize(&mut w, self.bases.len());
        for (n, b) in &self.bases {
            put_usize(&mut w, *n);
            put_usize(&mut w, b.players.len());
            b.players.iter().for_each(|p| w.u64(p.to_bits()));
        }
        put_usize(&mut w, self.promoted.len());
        for (k, c) in &self.promoted {
            k.encode(&mut w);
            w.u64(c.to_bits());
        }
        w.into_bytes()
    }

    /// Reads a world [`World::save`] wrote, with the configuration, goods and recipe book it was
    /// built from.
    pub fn load(
        cfg: WorldConfig,
        goods: Goods,
        book: RecipeBook,
        bytes: &[u8],
    ) -> codec::Result<World> {
        let r = &mut Reader::new(bytes);
        if r.u32()? != MAGIC {
            return bad("not a phase 1 world snapshot".into());
        }
        let (ng, nr) = (get_usize(r)?, get_usize(r)?);
        if ng != goods.len() || nr != book.recipes.len() {
            return bad(format!(
                "the snapshot has {ng} goods and {nr} recipes, this catalogue {} and {}",
                goods.len(),
                book.recipes.len()
            ));
        }
        let now = Tick(r.u64()?);
        let settlements = Settlements::decode(r)?;
        let mut snaps = SnapDec::new();
        let n = r.len_prefix(40)?;
        let mut nodes = Vec::with_capacity(n);
        for _ in 0..n {
            nodes.push(get_node(r, &mut snaps)?);
        }
        let markets = LazyTable::decode_with(r, load_market)?;
        let n = r.len_prefix(40)?;
        let edges = (0..n)
            .map(|_| Edge::decode(r))
            .collect::<codec::Result<_>>()?;
        let knowledge = Knowledge::load(r, cfg.knowledge.clone())?;
        let wheel = TimingWheel::decode_with(r, get_ev)?;
        let mut packets = BTreeMap::new();
        for _ in 0..r.len_prefix(8)? {
            let k = get_u32(r)?;
            packets.insert(k, get_packet(r, &mut snaps)?);
        }
        let mut wakes = BTreeSet::new();
        for _ in 0..r.len_prefix(9)? {
            wakes.insert((Tick(r.u64()?), get_usize(r)?));
        }
        let next_packet = r.u32()?;
        let caravans: Arena<Caravan> = Arena::decode_with(r, Caravan::decode)?;
        let legs = LegIndex::decode(r)?;
        let mut leg_ids = BTreeMap::new();
        for _ in 0..r.len_prefix(16)? {
            leg_ids.insert(handle(r)?, handle(r)?);
        }
        let outside = Outside {
            player_in: get_i128_map(r)?,
            player_out: get_i128_map(r)?,
            destroyed: get_i128_map(r)?,
            held: get_i128_map(r)?,
        };
        let stats = Stats {
            market_events: r.var_u64()?,
            breakpoints: r.var_u64()?,
            wakes: r.var_u64()?,
            deliveries: r.var_u64()?,
            packets_sent: r.var_u64()?,
            segments: r.var_u64()?,
            caravan_arrivals: r.var_u64()?,
            promotions: r.var_u64()?,
            player_trades: r.var_u64()?,
            houses: r.var_u64()?,
            band_packets: r.var_u64()?,
        };
        let n = r.len_prefix(2)?;
        let outbox = (0..n)
            .map(|_| get_command(r))
            .collect::<codec::Result<_>>()?;
        let n = r.len_prefix(2)?;
        let lints = (0..n)
            .map(|_| Lint::decode(r))
            .collect::<codec::Result<_>>()?;
        let mint_cost = [r.f64()?, r.f64()?];
        let mut players = BTreeMap::new();
        for _ in 0..r.len_prefix(33)? {
            let a = handle(r)?;
            let uuid = (r.u64()?, r.u64()?);
            let pos = match r.u8()? {
                0 => None,
                1 => Some((Pos::decode(r)?, Tick(r.u64()?))),
                t => return bad(format!("option tag {t}")),
            };
            let interest = handle(r)?;
            let interest = (!interest.is_null()).then_some(interest);
            players.insert(
                a,
                PlayerState {
                    uuid,
                    pos,
                    interest,
                },
            );
        }
        let roads = RoadIndex::decode(r)?;
        let n = r.len_prefix(16)?;
        let bands = (0..n).map(|_| get_band(r)).collect::<codec::Result<_>>()?;
        let mut growth = BTreeMap::new();
        for _ in 0..r.len_prefix(10)? {
            let n = get_usize(r)?;
            growth.insert(
                n,
                Growth {
                    since: get_opt_tick(r)?,
                    stamp: r.u32()?,
                    houses: r.u32()?,
                },
            );
        }
        let mut houses_pending = BTreeMap::new();
        for _ in 0..r.len_prefix(9)? {
            houses_pending.insert(r.u64()?, get_usize(r)?);
        }
        let mut sentiment_dirty = BTreeSet::new();
        for _ in 0..r.len_prefix(1)? {
            sentiment_dirty.insert(get_usize(r)?);
        }
        let op_counter = r.u64()?;
        let mut bases = BTreeMap::new();
        for _ in 0..r.len_prefix(2)? {
            let n = get_usize(r)?;
            let k = r.len_prefix(8)?;
            let players = (0..k).map(|_| handle(r)).collect::<codec::Result<_>>()?;
            bases.insert(n, BaseState { players });
        }
        let mut promoted = BTreeMap::new();
        for _ in 0..r.len_prefix(11)? {
            promoted.insert(CaravanKey::decode(r)?, handle(r)?);
        }
        if !r.is_empty() {
            return bad(format!(
                "{} trailing bytes in a world snapshot",
                r.remaining()
            ));
        }
        let world = World {
            cfg,
            goods,
            book,
            settlements,
            nodes,
            markets,
            edges,
            knowledge,
            wheel,
            packets,
            wakes,
            next_packet,
            caravans,
            legs,
            leg_ids,
            outside,
            stats,
            outbox,
            lints,
            mint_cost,
            now,
            trace: None,
            players,
            roads,
            bands,
            growth,
            houses_pending,
            sentiment_dirty,
            op_counter,
            bases,
            promoted,
        };
        world.check_indices()?;
        Ok(world)
    }

    /// Indices into the world's own tables that a corrupt snapshot could get wrong, checked once
    /// on load so a bad page fails there rather than panicking in a later step.
    fn check_indices(&self) -> codec::Result<()> {
        let nn = self.nodes.len();
        let ne = self.edges.len();
        let node_ok = |i: usize| i < nn;
        let dir_ok = |d: &DirRef| (d.edge as usize) < ne;
        for (i, n) in self.nodes.iter().enumerate() {
            let ok = self.settlements.get(n.settlement).is_some()
                && self.markets.get(n.region).is_some()
                && n.out.iter().all(dir_ok)
                && n.raids_in.iter().all(dir_ok)
                && n.neighbours.iter().all(|&x| node_ok(x))
                && n.bands
                    .iter()
                    .all(|&(b, _)| (b as usize) < self.bands.len());
            if !ok {
                return bad(format!("node {i} refers past the world's tables"));
            }
        }
        for (i, e) in self.edges.iter().enumerate() {
            let ok = node_ok(e.a)
                && node_ok(e.b)
                && e.dirs.iter().all(|d| node_ok(d.from) && node_ok(d.to))
                && e.stops.iter().all(|&s| node_ok(s));
            if !ok {
                return bad(format!("edge {i} refers past the world's nodes"));
            }
        }
        for (h, c) in self.caravans.iter() {
            if !node_ok(c.home) || !node_ok(c.leg.from) || !node_ok(c.leg.to) {
                return bad(format!("caravan {h:?} refers past the world's nodes"));
            }
        }
        for b in &self.bands {
            if !node_ok(b.home) || !b.route.iter().all(|&x| node_ok(x)) {
                return bad("a band refers past the world's nodes".into());
            }
        }
        let keys_ok = self.growth.keys().all(|&n| node_ok(n))
            && self.houses_pending.values().all(|&n| node_ok(n))
            && self.sentiment_dirty.iter().all(|&n| node_ok(n))
            && self.bases.keys().all(|&n| node_ok(n))
            && self.wakes.iter().all(|&(_, n)| node_ok(n));
        if !keys_ok {
            return bad("a growth, house, base or wake entry refers past the world's nodes".into());
        }
        for p in self.packets.values() {
            let via_ok = match p.via {
                Via::Road(d) => dir_ok(&d),
                Via::Band { band, .. } => (band as usize) < self.bands.len(),
            };
            if !via_ok || !node_ok(p.from) || !node_ok(p.to) {
                return bad("a news packet refers past the world's tables".into());
            }
        }
        for (_, e) in self.wheel.iter() {
            let ok = match *e {
                Ev::Wake(n) | Ev::Grow { node: n, .. } => node_ok(n),
                Ev::Deliver(_) | Ev::Arrive { .. } => true,
            };
            if !ok {
                return bad("a world event refers past the world's nodes".into());
            }
        }
        Ok(())
    }
}
