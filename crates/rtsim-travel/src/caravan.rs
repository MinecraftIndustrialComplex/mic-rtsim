// SPDX-License-Identifier: AGPL-3.0-or-later
//! Travellers: sampled from flows near a player, and individual rows once something happens to
//! them (A13, SCALE.md §3, PLAN.md §8.6).
//!
//! - A [`Traveller`] is derived, never stored: identity from (edge seed, direction, departure
//!   boundary), cargo from the consignments its window still has on the road, position from the
//!   departure time and the travel time. Walk away and back and the same caravan comes back.
//! - A [`Caravan`] is an individual row: made when something must persist (robbed, rescued, a
//!   llama taken, a trade with consequence). Its consignments are cut out of the flow, so totals
//!   stay exact, and it walks its own legs from then on.
//! - A caravan is a group (C25): a wandering trader and two trader llamas, or a ribbit merchant.
//!   They materialize, are captured and despawn as one unit; the cargo rides in the llamas'
//!   chests (C21), or with the ribbit merchant itself (P31's recommendation, overnight choice E12).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_knowledge::ActorId;
use rtsim_settle::Culture;
use rtsim_trade::{Amount, GoodId, Goods, MILLI};

/// The caravans' RNG system: identity and looks of sampled travellers.
pub const SYS_TRAVELLER: SystemId = SystemId::named("travel.traveller");

/// What a group member is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    /// A wandering trader (C11).
    Trader,
    /// A trader llama with a chest (C21).
    Llama,
    /// A ribbit merchant (`RibbitData` profession merchant).
    RibbitMerchant,
}

/// Which group shape a caravan has (C25; PLAN.md §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// One wandering trader and two trader llamas, as vanilla spawns them.
    HumanCaravan,
    /// Ribbit merchants; how many is P18 (one, PLACEHOLDER).
    RibbitCaravan,
}

impl GroupKind {
    pub fn of(culture: Option<Culture>) -> GroupKind {
        match culture {
            Some(Culture::Ribbit) => GroupKind::RibbitCaravan,
            _ => GroupKind::HumanCaravan,
        }
    }

    pub fn bodies(self) -> &'static [Body] {
        match self {
            GroupKind::HumanCaravan => &[Body::Trader, Body::Llama, Body::Llama],
            GroupKind::RibbitCaravan => &[Body::RibbitMerchant],
        }
    }

    /// Which members carry cargo.
    pub fn carries(body: Body) -> bool {
        matches!(body, Body::Llama | Body::RibbitMerchant)
    }
}

/// A caravan's identity: the direction it travels and the boundary it left at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaravanKey {
    pub edge: u32,
    pub dir: u8,
    pub departure: u64,
}

impl CaravanKey {
    /// The seed every derived trait of this caravan comes from (looks, temperament, names).
    pub fn seed(self, world_seed: u64, edge_seed: u64) -> u64 {
        let key = StreamKey::new(world_seed, SYS_TRAVELLER, edge_seed);
        key.block(self.departure, u64::from(self.dir))[0]
    }
}

/// A traveller sampled from a flow: derived, never stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traveller {
    pub key: CaravanKey,
    pub seed: u64,
    pub from: usize,
    pub to: usize,
    pub group: GroupKind,
    /// Goods and money it carries now, thousandths.
    pub cargo: BTreeMap<GoodId, Amount>,
    /// Where it is along the edge, thousandths of the way (0 at departure, 1,000 on arrival).
    pub progress_milli: u32,
    pub departed: Tick,
    pub arrives: Tick,
}

/// One member of an individual caravan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub body: Body,
    pub alive: bool,
    /// Still part of the group (not taken by a player, A10).
    pub present: bool,
    /// What this member carries (a llama's chest, a ribbit merchant's stock).
    pub cargo: BTreeMap<GoodId, Amount>,
}

/// Where an individual caravan is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    /// To the market it set out for.
    Outbound,
    /// Home, with what it was paid.
    Returning,
}

/// A caravan leg: a straight walk between two markets, so position is a formula (PLAN.md §4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Leg {
    pub from: usize,
    pub to: usize,
    pub start: Tick,
    pub end: Tick,
    /// Bumped on every replan: bookings and meetings stamped with an older value are stale.
    pub generation: u32,
}

/// An individual caravan row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caravan {
    pub key: CaravanKey,
    pub seed: u64,
    /// Its owner: the market it belongs to (PLAN.md §8.7: a caravan's owner is its home market).
    pub home: usize,
    pub group: GroupKind,
    pub members: Vec<Member>,
    pub stage: Stage,
    pub leg: Leg,
    /// The caravan as a knowledge holder: it carries what happened to it.
    pub holder: ActorId,
    /// Its trader as a notable, once something made it one.
    pub notable: Option<ActorId>,
    /// The price each good was quoted at (the destination's, in its currency), for the sale.
    pub quotes: BTreeMap<GoodId, i64>,
}

impl Caravan {
    /// Everything it carries, summed over members.
    pub fn cargo(&self) -> BTreeMap<GoodId, Amount> {
        let mut out: BTreeMap<GoodId, Amount> = BTreeMap::new();
        for m in self.members.iter().filter(|m| m.present) {
            for (&g, &a) in &m.cargo {
                if a != 0 {
                    *out.entry(g).or_default() += a;
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.cargo().values().all(|&a| a == 0)
    }

    /// Whether anyone is left to carry on (a trader or a merchant alive and present).
    pub fn has_leader(&self) -> bool {
        self.members.iter().any(|m| {
            m.alive && m.present && matches!(m.body, Body::Trader | Body::RibbitMerchant)
        })
    }

    /// Takes up to `amount` of `g` from the members, first carrier first; returns what was taken.
    pub fn take(&mut self, g: GoodId, amount: Amount) -> Amount {
        let mut left = amount;
        for m in self.members.iter_mut().filter(|m| m.present) {
            if left == 0 {
                break;
            }
            if let Some(have) = m.cargo.get_mut(&g) {
                let t = (*have).min(left);
                *have -= t;
                left -= t;
            }
        }
        amount - left
    }

    /// Adds `amount` of `g`, to the first carrier.
    pub fn give(&mut self, g: GoodId, amount: Amount) {
        if amount == 0 {
            return;
        }
        let i = self
            .members
            .iter()
            .position(|m| m.present && m.alive && GroupKind::carries(m.body))
            .or_else(|| self.members.iter().position(|m| m.present))
            .expect("a caravan with nobody to carry its cargo");
        *self.members[i].cargo.entry(g).or_default() += amount;
    }

    /// Where it is, thousandths of the way along its leg.
    pub fn progress_milli(&self, at: Tick) -> u32 {
        let (s, e) = (self.leg.start.0, self.leg.end.0);
        if at.0 <= s || e <= s {
            return 0;
        }
        (((at.0 - s).min(e - s) as u128 * 1000) / u128::from(e - s)) as u32
    }
}

/// Spreads cargo over a group's carriers: llamas fill chest by chest (C21: 3 rows × strength
/// slots each, PLAN.md §8.4); a ribbit merchant carries it all. Whatever does not fit the chests
/// rides with the last carrier, so no thousandth is lost (the bridge decides what overflow means
/// in game).
pub fn load(
    group: GroupKind,
    cargo: &BTreeMap<GoodId, Amount>,
    goods: &Goods,
    slots_per_carrier: u32,
) -> Vec<Member> {
    let mut members: Vec<Member> = group
        .bodies()
        .iter()
        .map(|&body| Member {
            body,
            alive: true,
            present: true,
            cargo: BTreeMap::new(),
        })
        .collect();
    let carriers: Vec<usize> = (0..members.len())
        .filter(|&i| GroupKind::carries(members[i].body))
        .collect();
    let mut ci = 0;
    let mut room = i64::from(slots_per_carrier);
    for (&g, &amt) in cargo {
        let mut left = amt;
        let per_slot = i64::from(goods.get(g).stack.max(1)) * MILLI;
        while left > 0 {
            let last = ci + 1 >= carriers.len();
            let fit = if last { left } else { (room * per_slot).min(left) };
            if fit > 0 {
                *members[carriers[ci]].cargo.entry(g).or_default() += fit;
                left -= fit;
                room -= (fit + per_slot - 1) / per_slot;
            }
            if left > 0 || room <= 0 {
                if !last {
                    ci += 1;
                    room = i64::from(slots_per_carrier);
                } else {
                    room = 0;
                }
            }
        }
    }
    members
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_trade::{GoodDef, GoodKind};

    #[test]
    fn llamas_fill_chest_by_chest() {
        let mut goods = Goods::new();
        let wheat = goods.add(GoodDef {
            name: "wheat".into(),
            kind: GoodKind::Item,
            need_per_pop_day: 0,
            stack: 64,
        });
        let mut cargo = BTreeMap::new();
        // 300 wheat: 3 slots in llama one (192), the rest in llama two.
        cargo.insert(wheat, 300 * MILLI);
        let m = load(GroupKind::HumanCaravan, &cargo, &goods, 3);
        assert_eq!(m.len(), 3);
        assert!(m[0].cargo.is_empty());
        assert_eq!(m[1].cargo[&wheat], 192 * MILLI);
        assert_eq!(m[2].cargo[&wheat], 108 * MILLI);
        let r = load(GroupKind::RibbitCaravan, &cargo, &goods, 3);
        assert_eq!(r[0].cargo[&wheat], 300 * MILLI);
    }
}
