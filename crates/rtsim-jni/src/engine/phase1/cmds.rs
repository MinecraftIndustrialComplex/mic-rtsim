// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase 1's commands as records (docs/boundary.md §4.2), and the puppet commands phase 1 decides
//! (SPAWN_GROUP with its SURFACE flag, DESPAWN_GROUP, STEER_GROUP, §2.4).
//!
//! The same writer serves the sim's own decisions and a test's scripted orders. WRITE_GOSSIP,
//! PLACE_HOUSE and SPAWN_RESIDENT carry op ids the bridge answers with COMMAND_RESULT: those are
//! kept (saved with the pages, resent after a restart under their sequence) until the answer
//! comes, as the core's transfers are (J11).

use rtsim_kernel::codec::{self, CodecError, Reader, Writer};

use crate::orders::{
    GossipCmd, MarketCmd, MemberOrder, OffersCmd, PlaceHouseCmd, SentimentCmd, SpawnResidentCmd,
};
use crate::wire::{self, RecordWriter, cmd, obs};

/// One phase 1 command, decided by the sim or ordered by a test.
#[derive(Clone, Debug, PartialEq)]
pub enum P1Cmd {
    Market(MarketCmd),
    Offers(OffersCmd),
    Gossip(GossipCmd),
    Sentiment(SentimentCmd),
    PlaceHouse(PlaceHouseCmd),
    SpawnResident(SpawnResidentCmd),
    Spawn {
        agent: u64,
        epoch: u32,
        group_kind: String,
        dimension: String,
        pos: [f64; 3],
        flags: u32,
        members: Vec<MemberOrder>,
    },
    Despawn {
        agent: u64,
        epoch: u32,
        reason: u32,
    },
    Steer {
        agent: u64,
        epoch: u32,
        speed: f32,
        pos: [f64; 3],
    },
}

impl P1Cmd {
    /// The op id the bridge answers with COMMAND_RESULT, for the commands that are kept.
    pub fn kept_op(&self) -> Option<u64> {
        match self {
            P1Cmd::Gossip(g) => Some(g.op),
            P1Cmd::PlaceHouse(h) => Some(h.op),
            P1Cmd::SpawnResident(r) => Some(r.op),
            _ => None,
        }
    }

    /// Latest wins: a newer one replaces an older one still queued for the same target.
    pub fn supersedes(&self, older: &P1Cmd) -> bool {
        match (self, older) {
            (P1Cmd::Market(a), P1Cmd::Market(b)) => a.village == b.village,
            (P1Cmd::Sentiment(a), P1Cmd::Sentiment(b)) => a.holder == b.holder,
            (P1Cmd::Offers(a), P1Cmd::Offers(b)) => {
                (a.agent, a.epoch, a.member) == (b.agent, b.epoch, b.member)
            }
            _ => false,
        }
    }

    /// The dictionary names this command needs: (kind, name).
    pub fn names(&self, out: &mut Vec<(u32, String)>) {
        let item = |out: &mut Vec<(u32, String)>, n: &str| {
            if !n.is_empty() {
                out.push((wire::DICT_ITEM, n.to_string()));
            }
        };
        match self {
            P1Cmd::Market(m) => {
                item(out, &m.currency);
                m.goods.iter().for_each(|g| item(out, &g.item));
            }
            P1Cmd::Offers(o) => {
                for f in &o.offers {
                    item(out, &f.cost_a);
                    item(out, &f.cost_b);
                    item(out, &f.result);
                }
            }
            P1Cmd::Gossip(_)
            | P1Cmd::Sentiment(_)
            | P1Cmd::Despawn { .. }
            | P1Cmd::Steer { .. } => {}
            P1Cmd::PlaceHouse(h) => {
                out.push((wire::DICT_DIMENSION, h.dimension.clone()));
                out.push((wire::DICT_TEMPLATE, h.template.clone()));
                if !h.processors.is_empty() {
                    out.push((wire::DICT_PROCESSORS, h.processors.clone()));
                }
            }
            P1Cmd::SpawnResident(r) => {
                out.push((wire::DICT_DIMENSION, r.dimension.clone()));
                out.push((wire::DICT_BODY, r.body.clone()));
                if !r.profession.is_empty() {
                    out.push((wire::DICT_PROFESSION, r.profession.clone()));
                }
            }
            P1Cmd::Spawn {
                group_kind,
                dimension,
                members,
                ..
            } => {
                out.push((wire::DICT_BODY, group_kind.clone()));
                out.push((wire::DICT_DIMENSION, dimension.clone()));
                for m in members {
                    m.items.iter().for_each(|it| item(out, &it.item));
                }
            }
        }
    }

    /// Bytes of the command record (DICT records not included).
    pub fn bytes(&self) -> usize {
        wire::align8(match self {
            P1Cmd::Market(m) => cmd::market::size(
                m.goods.len(),
                super::PRICE_LEVELS.len() + 1,
                super::LADDER_DEPTH,
            ),
            P1Cmd::Offers(o) => cmd::merchant_offers::size(o.offers.len()),
            P1Cmd::Gossip(_) => cmd::write_gossip::SIZE,
            P1Cmd::Sentiment(s) => cmd::sentiment::size(s.entries.len()),
            P1Cmd::PlaceHouse(_) => cmd::place_house::SIZE,
            P1Cmd::SpawnResident(_) => cmd::spawn_resident::SIZE,
            P1Cmd::Spawn { members, .. } => {
                cmd::spawn_group::MEMBERS
                    + members
                        .iter()
                        .map(|m| cmd::spawn_group::member::size(m.items.len()))
                        .sum::<usize>()
            }
            P1Cmd::Despawn { .. } => cmd::despawn_group::SIZE,
            P1Cmd::Steer { .. } => cmd::steer_group::SIZE,
        })
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            P1Cmd::Market(_) => "MARKET",
            P1Cmd::Offers(_) => "MERCHANT_OFFERS",
            P1Cmd::Gossip(_) => "WRITE_GOSSIP",
            P1Cmd::Sentiment(_) => "SENTIMENT",
            P1Cmd::PlaceHouse(_) => "PLACE_HOUSE",
            P1Cmd::SpawnResident(_) => "SPAWN_RESIDENT",
            P1Cmd::Spawn { .. } => "SPAWN_GROUP",
            P1Cmd::Despawn { .. } => "DESPAWN_GROUP",
            P1Cmd::Steer { .. } => "STEER_GROUP",
        }
    }
}

/// Writes `c` as its record. `dict(kind, name)` gives a name's dictionary id, writing its DICT
/// record first when the session has not sent it (the caller checked everything fits). The
/// record's sequence is `seq`.
pub fn write(
    w: &mut RecordWriter<'_>,
    c: &P1Cmd,
    seq: u64,
    dict: &mut dyn FnMut(&mut RecordWriter<'_>, u32, &str) -> u32,
) {
    // DICT records first, so every id a command uses is defined before it.
    let mut ids = Vec::new();
    let mut names = Vec::new();
    c.names(&mut names);
    for (k, n) in &names {
        ids.push(dict(w, *k, n));
    }
    let id_of = |kind: u32, name: &str| -> u32 {
        if name.is_empty() {
            return 0;
        }
        names
            .iter()
            .position(|(k, n)| *k == kind && n == name)
            .map_or(0, |i| ids[i])
    };
    let size = c.bytes();
    match c {
        P1Cmd::Market(m) => {
            let r = w
                .reserve(cmd::MARKET, size, m.goods.len() as u32)
                .expect("checked to fit");
            use cmd::market as k;
            let (levels, depth) = (super::PRICE_LEVELS.len() + 1, super::LADDER_DEPTH);
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::VILLAGE, m.village);
            wire::put_u64(r, k::AS_OF_OBS_SEQ, m.as_of);
            wire::put_u32(r, k::CURRENCY, id_of(wire::DICT_ITEM, &m.currency));
            wire::put_i64(r, k::TREASURY, m.treasury);
            wire::put_i64(r, k::TREASURY_TARGET, m.treasury_target);
            r[k::LEVELS] = levels as u8;
            r[k::DEPTH] = depth as u8;
            wire::put_i32(r, k::THRESHOLDS, i32::from(i8::MAX));
            for (i, t) in super::PRICE_LEVELS.iter().enumerate() {
                wire::put_i32(r, k::THRESHOLDS + 4 * (i + 1), *t);
            }
            for (i, g) in m.goods.iter().enumerate() {
                let o = k::GOODS + i * k::good(levels, depth);
                wire::put_u32(r, o + k::G_ITEM, id_of(wire::DICT_ITEM, &g.item));
                wire::put_i64(r, o + k::G_STOCK, g.stock);
                for (side, steps) in market_ladders(m, g).into_iter().enumerate() {
                    for (lv, ladder) in steps.into_iter().enumerate() {
                        for (j, (items, coins)) in ladder.into_iter().enumerate() {
                            let at = o + k::step(levels, depth, side, lv, j);
                            wire::put_i32(r, at, items);
                            wire::put_i32(r, at + 4, coins);
                        }
                    }
                }
            }
        }
        P1Cmd::Offers(o) => {
            let r = w
                .reserve(cmd::MERCHANT_OFFERS, size, o.offers.len() as u32)
                .expect("checked to fit");
            use cmd::merchant_offers as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::AGENT, o.agent);
            wire::put_u32(r, k::EPOCH, o.epoch);
            wire::put_u32(r, k::MEMBER, o.member);
            for (i, f) in o.offers.iter().enumerate() {
                let p = k::OFFERS + i * k::OFFER;
                wire::put_u32(r, p + k::O_COST_A_ITEM, id_of(wire::DICT_ITEM, &f.cost_a));
                wire::put_i32(r, p + k::O_COST_A_COUNT, f.cost_a_count);
                wire::put_u32(r, p + k::O_COST_B_ITEM, id_of(wire::DICT_ITEM, &f.cost_b));
                wire::put_i32(r, p + k::O_COST_B_COUNT, f.cost_b_count);
                wire::put_u32(r, p + k::O_RESULT_ITEM, id_of(wire::DICT_ITEM, &f.result));
                wire::put_i32(r, p + k::O_RESULT_COUNT, f.result_count);
                wire::put_i32(r, p + k::O_MAX_USES, f.max_uses);
                wire::put_i32(r, p + k::O_XP, 0);
                // Vanilla's wandering trader trades use 0.05 (the stub sends the same).
                wire::put_f32(r, p + k::O_PRICE_MULT, 0.05);
                wire::put_u32(r, p + k::O_FLAGS, f.flags);
            }
        }
        P1Cmd::Gossip(g) => {
            let r = w
                .reserve(cmd::WRITE_GOSSIP, size, 0)
                .expect("checked to fit");
            use cmd::write_gossip as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::OP_ID, g.op);
            wire::put_u64(r, k::VILLAGE, g.village);
            wire::put_u64(r, k::PLAYER_MOST, g.player.0);
            wire::put_u64(r, k::PLAYER_LEAST, g.player.1);
            wire::put_u32(r, k::GOSSIP, g.gossip);
            wire::put_i32(r, k::AMOUNT, g.amount);
            wire::put_u32(r, k::REPORT, g.report);
            wire::put_u32(r, k::FLAGS, g.flags);
        }
        P1Cmd::Sentiment(s) => {
            let r = w
                .reserve(cmd::SENTIMENT, size, s.entries.len() as u32)
                .expect("checked to fit");
            use cmd::sentiment as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::HOLDER, s.holder);
            wire::put_i32(r, k::DISLIKE, s.dislike);
            wire::put_i32(r, k::FRIEND, s.friend);
            wire::put_u32(r, k::FLAGS, s.flags);
            for (i, &(most, least, v)) in s.entries.iter().enumerate() {
                let e = k::ENTRIES + i * k::ENTRY;
                wire::put_u64(r, e, most);
                wire::put_u64(r, e + 8, least);
                wire::put_i32(r, e + 16, v);
            }
        }
        P1Cmd::PlaceHouse(h) => {
            let r = w
                .reserve(cmd::PLACE_HOUSE, size, 0)
                .expect("checked to fit");
            use cmd::place_house as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::OP_ID, h.op);
            wire::put_u64(r, k::VILLAGE, h.village);
            wire::put_u32(r, k::DIM, id_of(wire::DICT_DIMENSION, &h.dimension));
            wire::put_u32(r, k::TEMPLATE, id_of(wire::DICT_TEMPLATE, &h.template));
            wire::put_u32(
                r,
                k::PROCESSORS,
                id_of(wire::DICT_PROCESSORS, &h.processors),
            );
            wire::put_i32(r, k::X, h.pos[0]);
            wire::put_i32(r, k::Y, h.pos[1]);
            wire::put_i32(r, k::Z, h.pos[2]);
            r[k::ROTATION] = h.rotation as u8;
            r[k::MIRROR] = h.mirror as u8;
            wire::put_u16(r, k::FLAGS, h.flags as u16);
            wire::put_u32(r, k::RADIUS, h.radius);
            wire::put_u64(r, k::SEED, h.seed);
            wire::put_i32(r, k::Y_OFFSET, h.y_offset);
        }
        P1Cmd::SpawnResident(s) => {
            let r = w
                .reserve(cmd::SPAWN_RESIDENT, size, 0)
                .expect("checked to fit");
            use cmd::spawn_resident as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::OP_ID, s.op);
            wire::put_u64(r, k::VILLAGE, s.village);
            wire::put_u32(r, k::DIM, id_of(wire::DICT_DIMENSION, &s.dimension));
            wire::put_u32(r, k::BODY, id_of(wire::DICT_BODY, &s.body));
            wire::put_u32(
                r,
                k::PROFESSION,
                id_of(wire::DICT_PROFESSION, &s.profession),
            );
            wire::put_u32(r, k::FLAGS, s.flags);
            wire::put_f64(r, k::X, s.pos[0]);
            wire::put_f64(r, k::Y, s.pos[1]);
            wire::put_f64(r, k::Z, s.pos[2]);
            wire::put_u64(r, k::HOUSE, s.house);
        }
        P1Cmd::Spawn {
            agent,
            epoch,
            group_kind,
            dimension,
            pos,
            flags,
            members,
        } => {
            let r = w
                .reserve(cmd::SPAWN_GROUP, size, members.len() as u32)
                .expect("checked to fit");
            use cmd::spawn_group as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::AGENT, *agent);
            wire::put_u32(r, k::EPOCH, *epoch);
            wire::put_u32(r, k::GROUP_KIND, id_of(wire::DICT_BODY, group_kind));
            wire::put_u32(r, k::DIM, id_of(wire::DICT_DIMENSION, dimension));
            wire::put_u32(r, k::FLAGS, *flags);
            wire::put_f64(r, k::X, pos[0]);
            wire::put_f64(r, k::Y, pos[1]);
            wire::put_f64(r, k::Z, pos[2]);
            let mut at = k::MEMBERS;
            for m in members {
                use k::member as mb;
                wire::put_u32(r, at + mb::INDEX, m.index);
                wire::put_u32(r, at + mb::ITEM_COUNT, m.items.len() as u32);
                wire::put_f32(r, at + mb::HEALTH, m.health);
                wire::put_i32(r, at + mb::BODY_A, m.body_a);
                wire::put_i32(r, at + mb::BODY_B, m.body_b);
                for (i, it) in m.items.iter().enumerate() {
                    let ia = at + mb::ITEMS + i * obs::item::SIZE;
                    wire::put_u32(r, ia + obs::item::SLOT, it.slot);
                    wire::put_u32(r, ia + obs::item::ITEM, id_of(wire::DICT_ITEM, &it.item));
                    wire::put_i32(r, ia + obs::item::COUNT, it.count);
                }
                at += mb::size(m.items.len());
            }
        }
        P1Cmd::Despawn {
            agent,
            epoch,
            reason,
        } => {
            let r = w
                .reserve(cmd::DESPAWN_GROUP, size, 0)
                .expect("checked to fit");
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, cmd::despawn_group::AGENT, *agent);
            wire::put_u32(r, cmd::despawn_group::EPOCH, *epoch);
            wire::put_u32(r, cmd::despawn_group::REASON, *reason);
        }
        P1Cmd::Steer {
            agent,
            epoch,
            speed,
            pos,
        } => {
            let r = w
                .reserve(cmd::STEER_GROUP, size, 0)
                .expect("checked to fit");
            use cmd::steer_group as k;
            wire::put_u64(r, cmd::CMD_SEQ, seq);
            wire::put_u64(r, k::AGENT, *agent);
            wire::put_u32(r, k::EPOCH, *epoch);
            wire::put_f32(r, k::SPEED, *speed);
            wire::put_f64(r, k::X, pos[0]);
            wire::put_f64(r, k::Y, pos[1]);
            wire::put_f64(r, k::Z, pos[2]);
        }
    }
}

// ---- the kept commands in the pages ---------------------------------------------------------------

fn put_str(w: &mut Writer, s: &str) {
    w.bytes(s.as_bytes());
}

fn get_str(r: &mut Reader<'_>) -> codec::Result<String> {
    String::from_utf8(r.bytes()?.to_vec()).map_err(|e| CodecError::Invalid(format!("utf-8: {e}")))
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

/// Encodes a kept command (WRITE_GOSSIP, PLACE_HOUSE, SPAWN_RESIDENT). Others are never kept.
pub fn encode_kept(w: &mut Writer, c: &P1Cmd) {
    match c {
        P1Cmd::Gossip(g) => {
            w.u8(1);
            w.u64(g.op);
            w.u64(g.village);
            w.u64(g.player.0);
            w.u64(g.player.1);
            w.u32(g.gossip);
            w.var_i64(i64::from(g.amount));
            w.u32(g.report);
            w.u32(g.flags);
        }
        P1Cmd::PlaceHouse(h) => {
            w.u8(2);
            w.u64(h.op);
            w.u64(h.village);
            put_str(w, &h.dimension);
            put_str(w, &h.template);
            put_str(w, &h.processors);
            h.pos.iter().for_each(|v| w.var_i64(i64::from(*v)));
            w.u32(h.rotation);
            w.u32(h.mirror);
            w.u32(h.flags);
            w.u32(h.radius);
            w.u64(h.seed);
            w.var_i64(i64::from(h.y_offset));
        }
        P1Cmd::SpawnResident(s) => {
            w.u8(3);
            w.u64(s.op);
            w.u64(s.village);
            put_str(w, &s.dimension);
            put_str(w, &s.body);
            put_str(w, &s.profession);
            w.u32(s.flags);
            s.pos.iter().for_each(|v| w.f64(*v));
            w.u64(s.house);
        }
        other => unreachable!("{} is never kept", other.kind_name()),
    }
}

pub fn decode_kept(r: &mut Reader<'_>) -> codec::Result<P1Cmd> {
    Ok(match r.u8()? {
        1 => P1Cmd::Gossip(GossipCmd {
            op: r.u64()?,
            village: r.u64()?,
            player: (r.u64()?, r.u64()?),
            gossip: r.u32()?,
            amount: i32_of(r.var_i64()?)?,
            report: r.u32()?,
            flags: r.u32()?,
        }),
        2 => P1Cmd::PlaceHouse(PlaceHouseCmd {
            op: r.u64()?,
            village: r.u64()?,
            dimension: get_str(r)?,
            template: get_str(r)?,
            processors: get_str(r)?,
            pos: [
                i32_of(r.var_i64()?)?,
                i32_of(r.var_i64()?)?,
                i32_of(r.var_i64()?)?,
            ],
            rotation: r.u32()?,
            mirror: r.u32()?,
            flags: r.u32()?,
            radius: r.u32()?,
            seed: r.u64()?,
            y_offset: i32_of(r.var_i64()?)?,
        }),
        3 => P1Cmd::SpawnResident(SpawnResidentCmd {
            op: r.u64()?,
            village: r.u64()?,
            dimension: get_str(r)?,
            body: get_str(r)?,
            profession: get_str(r)?,
            flags: r.u32()?,
            pos: [r.f64()?, r.f64()?, r.f64()?],
            house: r.u64()?,
        }),
        t => return Err(CodecError::Invalid(format!("kept phase 1 command tag {t}"))),
    })
}

/// A MARKET good's two ladders (the market's sales, then its purchases), level by level: what a
/// run of trades meets, bundle by bundle, at each price level's markup (rtsim-trade's
/// [`rtsim_trade::market::ladder`], the one source of the price math; Ben's morning review,
/// "Prices" and G31). Level 0 is fair; level k is `markup_for` at its threshold. A curve the
/// engine cannot price (a scripted order out of range) gives empty ladders: the offer shows
/// crossed out.
pub fn market_ladders(
    m: &crate::orders::MarketCmd,
    g: &crate::orders::MarketGood,
) -> [Vec<Vec<(i32, i32)>>; 2] {
    use rtsim_trade::market::{FAIR, ladder, markup_for};
    use rtsim_trade::tunables::Ratio;
    use rtsim_trade::{OfferSide, PriceCurve};
    let levels = super::PRICE_LEVELS.len() + 1;
    let empty = || vec![Vec::new(); levels];
    let r_ok = g.r_num >= 0
        && g.r_den > 0
        && g.r_num < g.r_den
        && i64::from(g.r_den) <= rtsim_trade::price::MAX_R_TERM;
    if !r_ok || !(0..=rtsim_trade::price::MAX_ANCHOR).contains(&g.anchor) {
        return [empty(), empty()];
    }
    let curve = PriceCurve::new(
        g.anchor,
        Ratio::new(i64::from(g.r_num), i64::from(g.r_den)),
        g.target,
    );
    let worst = Ratio::new(i64::from(m.worst_num.max(0)), i64::from(m.worst_den.max(1)));
    let markups: Vec<(Ratio, Ratio)> = std::iter::once((FAIR, FAIR))
        .chain(
            super::PRICE_LEVELS
                .iter()
                .map(|&t| markup_for(t.clamp(-128, 0) as i8, worst)),
        )
        .collect();
    let fit = |v: Vec<(i64, i64)>| -> Vec<(i32, i32)> {
        v.into_iter()
            .map_while(|(a, b)| Some((i32::try_from(a).ok()?, i32::try_from(b).ok()?)))
            .collect()
    };
    let side = |s: OfferSide| -> Vec<Vec<(i32, i32)>> {
        markups
            .iter()
            .map(|&(up, down)| {
                let mk = if s == OfferSide::MarketSells {
                    up
                } else {
                    down
                };
                fit(ladder(
                    &curve,
                    g.stock,
                    g.target,
                    m.treasury,
                    m.treasury_target,
                    s,
                    mk,
                    super::LADDER_DEPTH,
                ))
            })
            .collect()
    };
    [side(OfferSide::MarketSells), side(OfferSide::MarketBuys)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_commands_round_trip() {
        let cmds = vec![
            P1Cmd::Gossip(GossipCmd {
                op: 9,
                village: 8,
                player: (7, 6),
                gossip: 1,
                amount: 12,
                report: 3,
                flags: 0,
            }),
            P1Cmd::PlaceHouse(PlaceHouseCmd {
                op: 10,
                village: 8,
                dimension: "minecraft:overworld".into(),
                template: "ribbits:houses/small_house_red_1".into(),
                processors: "ribbits:main".into(),
                pos: [1, -2, 3],
                rotation: 2,
                mirror: 0,
                flags: 3,
                radius: 48,
                seed: 77,
                y_offset: 0,
            }),
            P1Cmd::SpawnResident(SpawnResidentCmd {
                op: 11,
                village: 8,
                dimension: "minecraft:overworld".into(),
                body: "ribbits:ribbit".into(),
                profession: String::new(),
                flags: 1,
                pos: [0.0; 3],
                house: 10,
            }),
        ];
        let mut w = Writer::new();
        cmds.iter().for_each(|c| encode_kept(&mut w, c));
        let b = w.into_bytes();
        let mut r = Reader::new(&b);
        for c in &cmds {
            assert_eq!(&decode_kept(&mut r).unwrap(), c);
        }
        r.finish().unwrap();
    }

    /// Prices (PR2): MARKET's ladders are rtsim-trade's, level by level, and a curve the engine
    /// cannot price gives empty ladders instead of a panic.
    #[test]
    fn market_ladders_are_rtsim_trades() {
        use crate::orders::{MarketCmd, MarketGood};
        let g = MarketGood {
            item: "minecraft:wheat".into(),
            stock: 400_000,
            target: 400_000,
            anchor: 50_000,
            r_num: 3,
            r_den: 4,
        };
        let m = MarketCmd {
            village: 1,
            as_of: 0,
            currency: "minecraft:emerald".into(),
            treasury: 3_000,
            treasury_target: 20_000,
            worst_num: 1,
            worst_den: 2,
            goods: vec![g.clone()],
        };
        let [sells, buys] = market_ladders(&m, &g);
        assert_eq!(sells.len(), super::super::PRICE_LEVELS.len() + 1);
        // Three emeralds in the treasury: three purchases at the fair level, one emerald each.
        assert_eq!(buys[0].len(), 3);
        assert!(buys[0].iter().all(|&(_, c)| c == 1));
        let curve =
            rtsim_trade::PriceCurve::new(50_000, rtsim_trade::tunables::Ratio::new(3, 4), 400_000);
        let want = rtsim_trade::market::ladder(
            &curve,
            400_000,
            400_000,
            3_000,
            20_000,
            rtsim_trade::OfferSide::MarketBuys,
            rtsim_trade::market::FAIR,
            super::super::LADDER_DEPTH,
        );
        let want: Vec<(i32, i32)> = want
            .into_iter()
            .map(|(a, b)| (a as i32, b as i32))
            .collect();
        assert_eq!(buys[0], want);
        // A disliked player brings more wheat for the same emerald.
        let last = buys.last().unwrap();
        assert!(last[0].0 > buys[0][0].0, "{:?} {:?}", buys[0], last);
        let bad = MarketGood {
            r_num: 5,
            r_den: 4,
            ..g
        };
        let [s, b] = market_ladders(&m, &bad);
        assert!(s.iter().chain(b.iter()).all(|l| l.is_empty()));
    }
}
