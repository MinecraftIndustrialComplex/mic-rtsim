// SPDX-License-Identifier: AGPL-3.0-or-later
//! The phase 1 world's inputs, as a log (choice V6).
//!
//! The phase 1 world (`rtsim_world::World`) is deterministic: given the same inputs at the same
//! world ticks it reaches the same state, bit for bit (`state_hash`). Everything that changes it
//! from outside (a village joining, a trade, a robbery, a caravan the engine materialized) is an
//! [`Input`], applied through [`apply`] at a world tick and appended to the log with that tick.
//! The pages keep the log, and `start` rebuilds the world by replaying it. A full snapshot codec of
//! the world (markets, pipelines, knowledge, wheels) is the follow-up; the log is small, and a
//! quiet world replays fast (measured in `tests/protocol.rs`).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{self, CodecError, Reader, Writer};
use rtsim_kernel::handle::Handle;
use rtsim_knowledge::{ActorId, ReportKind};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::labour::ResourceId;
use rtsim_trade::{Amount, GoodId};
use rtsim_travel::caravan::CaravanKey;
use rtsim_world::{CaravanId, NodeData, Side, World};

/// A player by UUID halves.
pub type Uuid = (u64, u64);

/// One thing that happened to the phase 1 world from outside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// A village adopted (C22) or nothing: `culture` 0 human, 1 ribbit.
    Join {
        key: u64,
        culture: u8,
        frame: u16,
        x: i32,
        z: i32,
        pop: u32,
        resources: Vec<(u16, Amount)>,
    },
    /// A census changed a village's population.
    Population { key: u64, pop: u32 },
    /// A resident's trade, as the bridge reported it.
    Trade {
        key: u64,
        player: Uuid,
        good: u16,
        items: i64,
        coins: i64,
        buy: bool,
    },
    /// A sampled traveller made an individual row (materialized near a player).
    Promote { edge: u32, dir: u8, departure: u64 },
    /// A puppet caravan's trade.
    CaravanTrade {
        caravan: u64,
        at_node: u64,
        player: Uuid,
        good: u16,
        currency: u16,
        buy: bool,
        items: i64,
        coins: i64,
    },
    /// Goods taken out of a caravan; `player` (0, 0) is nobody known.
    Rob {
        caravan: u64,
        player: Uuid,
        goods: Vec<(u16, Amount)>,
    },
    /// Goods put into a caravan.
    CargoIn {
        caravan: u64,
        goods: Vec<(u16, Amount)>,
    },
    /// A member taken (`killed` false) or killed; `player` (0, 0) is nobody known. Its planned
    /// cargo leaves with it (the sim's own books, before Review P3).
    Member {
        caravan: u64,
        member: u32,
        player: Uuid,
        killed: bool,
    },
    /// The same, booking exactly what the bridge saw it hold (its MEMBER_STATE's items the sim
    /// trades; Review P3, Ben: "Book what was really there").
    MemberHeld {
        caravan: u64,
        member: u32,
        player: Uuid,
        killed: bool,
        held: Vec<(u16, Amount)>,
    },
    /// A player hurt a caravan member, or killed a mob attacking one.
    CaravanReport {
        caravan: u64,
        player: Uuid,
        rescue: bool,
    },
    /// A report at a village about one of its residents (`kind`: rtsim-knowledge `ReportKind`).
    VillageReport { key: u64, player: Uuid, kind: u8 },
    /// BASE MARKED.
    MarkBase {
        key: u64,
        frame: u16,
        x: i32,
        z: i32,
        player: Uuid,
    },
    /// BASE UNMARKED.
    UnmarkBase { key: u64, player: Uuid },
    /// HOUSE_PLACED for a house the world booked.
    HousePlaced { op: u64 },
}

/// An input and the world tick it was applied at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logged {
    pub at: u64,
    pub input: Input,
}

/// What applying an input did, for the caller's bookkeeping (never for the world's: replay does
/// not see it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// The node a join or base made, or the trade happened at.
    pub node: Option<usize>,
    /// The row a promotion made.
    pub caravan: Option<CaravanId>,
    /// The input changed nothing (a refused trade, a caravan already gone).
    pub refused: bool,
    /// The bridge moved more than the world held; the rest was not booked.
    pub clamped: bool,
}

fn player(w: &mut World, p: Uuid) -> Option<ActorId> {
    (p != (0, 0)).then(|| w.player_uuid(p.0, p.1))
}

fn goods_map(list: &[(u16, Amount)]) -> BTreeMap<GoodId, Amount> {
    let mut m = BTreeMap::new();
    for &(g, a) in list {
        *m.entry(GoodId(g)).or_default() += a;
    }
    m
}

/// Applies one input at world tick `at` (the world is run there first). Deterministic: the same
/// world and input give the same world, which is what replay rests on.
pub fn apply(w: &mut World, nodes: &mut BTreeMap<u64, usize>, input: &Input, at: u64) -> Applied {
    let at = Tick(at.max(w.now().0));
    w.run_until(at);
    let mut out = Applied::default();
    let car = |bits: u64| -> CaravanId { Handle::from_bits(bits) };
    match input {
        Input::Join {
            key,
            culture,
            frame,
            x,
            z,
            pop,
            resources,
        } => {
            if nodes.contains_key(key) {
                out.refused = true;
                return out;
            }
            let c = if *culture == 1 {
                Culture::Ribbit
            } else {
                Culture::Human
            };
            let i = w.join(
                NodeData {
                    key: *key,
                    kind: NodeKind::Village(c),
                    pos: Pos::new(*frame, *x, *z),
                    pop: *pop,
                    resources: resources.iter().map(|&(r, a)| (ResourceId(r), a)).collect(),
                },
                at,
            );
            nodes.insert(*key, i);
            out.node = Some(i);
        }
        Input::Population { key, pop } => match nodes.get(key) {
            Some(&i) => w.set_population(i, (*pop).max(1), at),
            None => out.refused = true,
        },
        Input::Trade {
            key,
            player: p,
            good,
            items,
            coins,
            buy,
        } => {
            let (Some(&i), Some(a)) = (nodes.get(key), player(w, *p)) else {
                out.refused = true;
                return out;
            };
            out.node = Some(i);
            let side = if *buy { Side::Buy } else { Side::Sell };
            match w.trade_exact(i, a, GoodId(*good), *items, *coins, side, at) {
                Ok(b) => out.clamped = b.clamped,
                Err(_) => out.refused = true,
            }
        }
        Input::Promote {
            edge,
            dir,
            departure,
        } => {
            out.caravan = w.promote(CaravanKey {
                edge: *edge,
                dir: *dir,
                departure: *departure,
            });
            out.refused = out.caravan.is_none();
        }
        Input::CaravanTrade {
            caravan,
            at_node,
            player: p,
            good,
            currency,
            buy,
            items,
            coins,
        } => {
            let Some(a) = player(w, *p) else {
                out.refused = true;
                return out;
            };
            let at_node = nodes.get(at_node).copied();
            let side = if *buy { Side::Buy } else { Side::Sell };
            match w.caravan_trade_exact(
                car(*caravan),
                at_node,
                a,
                GoodId(*good),
                GoodId(*currency),
                side,
                *items,
                *coins,
            ) {
                Ok(b) => out.clamped = b.clamped,
                Err(_) => out.refused = true,
            }
        }
        Input::Rob {
            caravan,
            player: p,
            goods,
        } => {
            let a = player(w, *p);
            out.refused = w.rob_goods(car(*caravan), a, &goods_map(goods)).is_none();
        }
        Input::CargoIn { caravan, goods } => {
            out.refused = !w.cargo_in(car(*caravan), &goods_map(goods));
        }
        Input::Member {
            caravan,
            member,
            player: p,
            killed,
        } => {
            let c = car(*caravan);
            let m = *member as usize;
            out.refused = match (player(w, *p), killed) {
                (Some(a), true) => w.kill_member(c, m, a).is_none(),
                (Some(a), false) => w.take_member(c, m, a).is_none(),
                (None, k) => w.lose_member(c, m, !k).is_none(),
            };
        }
        Input::MemberHeld {
            caravan,
            member,
            player: p,
            killed,
            held,
        } => {
            let c = car(*caravan);
            let m = *member as usize;
            let held = goods_map(held);
            let held = Some(&held);
            out.refused = match (player(w, *p), killed) {
                (Some(a), true) => w.kill_member_holding(c, m, a, held).is_none(),
                (Some(a), false) => w.take_member_holding(c, m, a, held).is_none(),
                (None, k) => w.lose_member_holding(c, m, !k, held).is_none(),
            };
        }
        Input::CaravanReport {
            caravan,
            player: p,
            rescue,
        } => {
            let Some(a) = player(w, *p) else {
                out.refused = true;
                return out;
            };
            let c = car(*caravan);
            out.refused = if *rescue {
                w.rescue(c, a).is_none()
            } else {
                w.assault_caravan(c, a).is_none()
            };
        }
        Input::VillageReport {
            key,
            player: p,
            kind,
        } => {
            let (Some(&i), Some(a), Some(k)) = (nodes.get(key), player(w, *p), report_kind(*kind))
            else {
                out.refused = true;
                return out;
            };
            w.village_report(i, a, k, at);
            out.node = Some(i);
        }
        Input::MarkBase {
            key,
            frame,
            x,
            z,
            player: p,
        } => {
            let Some(a) = player(w, *p) else {
                out.refused = true;
                return out;
            };
            let i = w.mark_base(*key, Pos::new(*frame, *x, *z), a, at);
            nodes.insert(*key, i);
            out.node = Some(i);
        }
        Input::UnmarkBase { key, player: p } => {
            let Some(a) = player(w, *p) else {
                out.refused = true;
                return out;
            };
            w.unmark_base(*key, a);
        }
        Input::HousePlaced { op } => {
            out.refused = w.house_placed(*op) == 0;
        }
    }
    out
}

/// rtsim-knowledge's report kind from its number (1 assault ... 8 resident killing, 9 encroachment).
pub fn report_kind(k: u8) -> Option<ReportKind> {
    Some(match k {
        1 => ReportKind::Assault,
        2 => ReportKind::Killing,
        3 => ReportKind::Robbery,
        4 => ReportKind::Theft,
        5 => ReportKind::Trade,
        6 => ReportKind::Rescue,
        7 => ReportKind::ResidentAssault,
        8 => ReportKind::ResidentKilling,
        9 => ReportKind::Encroachment,
        _ => return None,
    })
}

// ---- codec ---------------------------------------------------------------------------------------

fn put_uuid(w: &mut Writer, p: &Uuid) {
    w.u64(p.0);
    w.u64(p.1);
}

fn get_uuid(r: &mut Reader<'_>) -> codec::Result<Uuid> {
    Ok((r.u64()?, r.u64()?))
}

fn put_goods(w: &mut Writer, g: &[(u16, Amount)]) {
    w.var_u64(g.len() as u64);
    for &(id, a) in g {
        w.u16(id);
        w.var_i64(a);
    }
}

fn get_goods(r: &mut Reader<'_>) -> codec::Result<Vec<(u16, Amount)>> {
    let n = r.var_u64()?;
    if n > 1 << 16 {
        return Err(CodecError::Invalid(format!("{n} goods in one input")));
    }
    (0..n).map(|_| Ok((r.u16()?, r.var_i64()?))).collect()
}

fn i32_of(v: i64) -> codec::Result<i32> {
    i32::try_from(v).map_err(|_| CodecError::Invalid(format!("{v} is not an i32")))
}

impl Logged {
    pub fn encode(&self, w: &mut Writer) {
        w.var_u64(self.at);
        match &self.input {
            Input::Join {
                key,
                culture,
                frame,
                x,
                z,
                pop,
                resources,
            } => {
                w.u8(1);
                w.u64(*key);
                w.u8(*culture);
                w.u16(*frame);
                w.var_i64(i64::from(*x));
                w.var_i64(i64::from(*z));
                w.u32(*pop);
                put_goods(w, resources);
            }
            Input::Population { key, pop } => {
                w.u8(2);
                w.u64(*key);
                w.u32(*pop);
            }
            Input::Trade {
                key,
                player,
                good,
                items,
                coins,
                buy,
            } => {
                w.u8(3);
                w.u64(*key);
                put_uuid(w, player);
                w.u16(*good);
                w.var_i64(*items);
                w.var_i64(*coins);
                w.u8(u8::from(*buy));
            }
            Input::Promote {
                edge,
                dir,
                departure,
            } => {
                w.u8(4);
                w.u32(*edge);
                w.u8(*dir);
                w.var_u64(*departure);
            }
            Input::CaravanTrade {
                caravan,
                at_node,
                player,
                good,
                currency,
                buy,
                items,
                coins,
            } => {
                w.u8(5);
                w.u64(*caravan);
                w.u64(*at_node);
                put_uuid(w, player);
                w.u16(*good);
                w.u16(*currency);
                w.u8(u8::from(*buy));
                w.var_i64(*items);
                w.var_i64(*coins);
            }
            Input::Rob {
                caravan,
                player,
                goods,
            } => {
                w.u8(6);
                w.u64(*caravan);
                put_uuid(w, player);
                put_goods(w, goods);
            }
            Input::CargoIn { caravan, goods } => {
                w.u8(7);
                w.u64(*caravan);
                put_goods(w, goods);
            }
            Input::Member {
                caravan,
                member,
                player,
                killed,
            } => {
                w.u8(8);
                w.u64(*caravan);
                w.u32(*member);
                put_uuid(w, player);
                w.u8(u8::from(*killed));
            }
            Input::CaravanReport {
                caravan,
                player,
                rescue,
            } => {
                w.u8(9);
                w.u64(*caravan);
                put_uuid(w, player);
                w.u8(u8::from(*rescue));
            }
            Input::VillageReport { key, player, kind } => {
                w.u8(10);
                w.u64(*key);
                put_uuid(w, player);
                w.u8(*kind);
            }
            Input::MarkBase {
                key,
                frame,
                x,
                z,
                player,
            } => {
                w.u8(11);
                w.u64(*key);
                w.u16(*frame);
                w.var_i64(i64::from(*x));
                w.var_i64(i64::from(*z));
                put_uuid(w, player);
            }
            Input::UnmarkBase { key, player } => {
                w.u8(12);
                w.u64(*key);
                put_uuid(w, player);
            }
            Input::HousePlaced { op } => {
                w.u8(13);
                w.u64(*op);
            }
            Input::MemberHeld {
                caravan,
                member,
                player,
                killed,
                held,
            } => {
                w.u8(14);
                w.u64(*caravan);
                w.u32(*member);
                put_uuid(w, player);
                w.u8(u8::from(*killed));
                put_goods(w, held);
            }
        }
    }

    pub fn decode(r: &mut Reader<'_>) -> codec::Result<Logged> {
        let at = r.var_u64()?;
        let tag = r.u8()?;
        let input = match tag {
            1 => Input::Join {
                key: r.u64()?,
                culture: r.u8()?,
                frame: r.u16()?,
                x: i32_of(r.var_i64()?)?,
                z: i32_of(r.var_i64()?)?,
                pop: r.u32()?,
                resources: get_goods(r)?,
            },
            2 => Input::Population {
                key: r.u64()?,
                pop: r.u32()?,
            },
            3 => Input::Trade {
                key: r.u64()?,
                player: get_uuid(r)?,
                good: r.u16()?,
                items: r.var_i64()?,
                coins: r.var_i64()?,
                buy: r.u8()? != 0,
            },
            4 => Input::Promote {
                edge: r.u32()?,
                dir: r.u8()?,
                departure: r.var_u64()?,
            },
            5 => Input::CaravanTrade {
                caravan: r.u64()?,
                at_node: r.u64()?,
                player: get_uuid(r)?,
                good: r.u16()?,
                currency: r.u16()?,
                buy: r.u8()? != 0,
                items: r.var_i64()?,
                coins: r.var_i64()?,
            },
            6 => Input::Rob {
                caravan: r.u64()?,
                player: get_uuid(r)?,
                goods: get_goods(r)?,
            },
            7 => Input::CargoIn {
                caravan: r.u64()?,
                goods: get_goods(r)?,
            },
            8 => Input::Member {
                caravan: r.u64()?,
                member: r.u32()?,
                player: get_uuid(r)?,
                killed: r.u8()? != 0,
            },
            9 => Input::CaravanReport {
                caravan: r.u64()?,
                player: get_uuid(r)?,
                rescue: r.u8()? != 0,
            },
            10 => Input::VillageReport {
                key: r.u64()?,
                player: get_uuid(r)?,
                kind: r.u8()?,
            },
            11 => Input::MarkBase {
                key: r.u64()?,
                frame: r.u16()?,
                x: i32_of(r.var_i64()?)?,
                z: i32_of(r.var_i64()?)?,
                player: get_uuid(r)?,
            },
            12 => Input::UnmarkBase {
                key: r.u64()?,
                player: get_uuid(r)?,
            },
            13 => Input::HousePlaced { op: r.u64()? },
            14 => Input::MemberHeld {
                caravan: r.u64()?,
                member: r.u32()?,
                player: get_uuid(r)?,
                killed: r.u8()? != 0,
                held: get_goods(r)?,
            },
            t => return Err(CodecError::Invalid(format!("phase 1 input tag {t}"))),
        };
        Ok(Logged { at, input })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_round_trips() {
        let p = (0x1234, 0x5678);
        let inputs = vec![
            Input::Join {
                key: 7,
                culture: 1,
                frame: 2,
                x: -300,
                z: 4000,
                pop: 12,
                resources: vec![(0, 100_000), (5, 4_000)],
            },
            Input::Population { key: 7, pop: 13 },
            Input::Trade {
                key: 7,
                player: p,
                good: 1,
                items: 6,
                coins: 1,
                buy: true,
            },
            Input::Promote {
                edge: 3,
                dir: 1,
                departure: 99,
            },
            Input::CaravanTrade {
                caravan: 1 << 33 | 4,
                at_node: 0,
                player: p,
                good: 0,
                currency: 7,
                buy: false,
                items: 10,
                coins: 1,
            },
            Input::Rob {
                caravan: 5,
                player: (0, 0),
                goods: vec![(0, -5_000)],
            },
            Input::CargoIn {
                caravan: 5,
                goods: vec![(2, 1_000)],
            },
            Input::Member {
                caravan: 5,
                member: 2,
                player: p,
                killed: true,
            },
            Input::CaravanReport {
                caravan: 5,
                player: p,
                rescue: true,
            },
            Input::VillageReport {
                key: 7,
                player: p,
                kind: 7,
            },
            Input::MarkBase {
                key: 1 << 62 | 9,
                frame: 1,
                x: 10,
                z: -10,
                player: p,
            },
            Input::UnmarkBase {
                key: 1 << 62 | 9,
                player: p,
            },
            Input::HousePlaced { op: 42 },
            Input::MemberHeld {
                caravan: 5,
                member: 1,
                player: p,
                killed: false,
                held: vec![(7, 3_000), (0, 1_000)],
            },
        ];
        let mut w = Writer::new();
        for (i, input) in inputs.iter().enumerate() {
            Logged {
                at: 1000 + i as u64,
                input: input.clone(),
            }
            .encode(&mut w);
        }
        let bytes = w.into_bytes();
        let mut r = Reader::new(&bytes);
        for (i, input) in inputs.iter().enumerate() {
            let l = Logged::decode(&mut r).unwrap();
            assert_eq!(l.at, 1000 + i as u64);
            assert_eq!(&l.input, input);
        }
        r.finish().unwrap();
    }
}
