// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reconciliation on load (INDUSTRY.md §5, I8): turning a district's net change while unloaded into
//! per-inventory deltas, and trimming outputs when an input comes up short.
//!
//! - **Removals** come from the inventories that held the good, in proportion to what each held,
//!   with largest-remainder rounding (ties by inventory index). Postbox inventories are drawn on
//!   only after plain stock, because taking from inside a package is the awkward case for the Java
//!   side (overnight choice F14).
//! - **Additions** go to the inventories that received the good while learning, in index order, up
//!   to each one's room; then to any that held it; then to any with room. Room for items uses the
//!   slot model: every good takes `ceil(count / max stack)` slots. What fits nowhere is returned as
//!   unplaced and stays pending in the engine (docs/industry-boundary.md §4.2).
//! - **Trims:** a removal short by `s` units of input c means the replay ran on input it did not
//!   have. Each output o it funded is trimmed by `floor(s × r_o / |r_c|)`, the largest such trim
//!   over the short inputs, and never more than was added.
//!
//! Every function here is pure, so the Java side can be tested against it.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::conserve::Diffuser;
use serde::Serialize;

use crate::ids::{BlockPos, ChunkPos, Good, GoodClass, GoodInfo, GoodVec};
use crate::obs::InvKind;

/// What the planner needs to know about one boundary inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvState {
    pub index: u32,
    pub pos: BlockPos,
    pub chunk: ChunkPos,
    pub kind: InvKind,
    pub item_slots: u32,
    pub fluid_capacity: i64,
    pub energy_capacity: i64,
    /// What it holds now as far as the engine knows: the last sample plus deltas not yet applied.
    pub contents: GoodVec,
    /// Goods it was seen to receive while learning.
    pub received: BTreeSet<Good>,
}

/// Goods the engine knows; unknown goods count as 64-stack items (F12).
pub trait GoodLookup {
    fn info(&self, g: Good) -> Option<&GoodInfo>;

    fn class(&self, g: Good) -> GoodClass {
        self.info(g).map_or(GoodClass::Item, |i| i.class)
    }

    fn max_stack(&self, g: Good) -> i64 {
        self.info(g)
            .filter(|i| i.class == GoodClass::Item)
            .map_or(64, |i| i64::from(i.max_stack.max(1)))
    }
}

impl GoodLookup for BTreeMap<Good, GoodInfo> {
    fn info(&self, g: Good) -> Option<&GoodInfo> {
        self.get(&g)
    }
}

/// One inventory's planned change for one good.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Planned {
    pub inventory: u32,
    pub good: Good,
    pub delta: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// Removals first, then additions; each sorted by inventory, then good.
    pub deltas: Vec<Planned>,
    /// Additions that fit nowhere (positive) and removals nobody held (negative).
    pub unplaced: GoodVec,
}

/// Splits `amount` over `weights` in proportion, never giving one more than its weight when
/// `amount <= Σ weights`; largest remainder first, ties to the lower index.
pub fn apportion(amount: i64, weights: &[i64]) -> Vec<i64> {
    let total: i128 = weights.iter().map(|&w| i128::from(w.max(0))).sum();
    if total == 0 || amount <= 0 {
        return vec![0; weights.len()];
    }
    let a = i128::from(amount).min(total);
    let mut out: Vec<i64> = Vec::with_capacity(weights.len());
    let mut rem: Vec<(i128, usize)> = Vec::with_capacity(weights.len());
    let mut given: i128 = 0;
    for (i, &w) in weights.iter().enumerate() {
        let num = a * i128::from(w.max(0));
        let share = num / total;
        out.push(share as i64);
        given += share;
        rem.push((num % total, i));
    }
    rem.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    for &(_, i) in rem.iter().take((a - given) as usize) {
        out[i] += 1;
    }
    out
}

/// Room for `g` in an inventory holding `contents`.
pub fn room_for(inv: &InvState, contents: &GoodVec, g: Good, goods: &impl GoodLookup) -> i64 {
    match goods.class(g) {
        GoodClass::Item => {
            if !inv.kind.has(InvKind::ITEMS) {
                return 0;
            }
            let mut used: i64 = 0;
            for (h, c) in contents.iter() {
                if goods.class(h) == GoodClass::Item && c > 0 {
                    let ms = goods.max_stack(h);
                    used += (c + ms - 1) / ms;
                }
            }
            let free = (i64::from(inv.item_slots) - used).max(0);
            let ms = goods.max_stack(g);
            let held = contents.get(g);
            let partial = if held > 0 { (ms - held % ms) % ms } else { 0 };
            free * ms + partial
        }
        GoodClass::Fluid => {
            if !inv.kind.has(InvKind::FLUIDS) {
                return 0;
            }
            // One fluid per tank.
            let other = contents
                .iter()
                .any(|(h, c)| h != g && c > 0 && goods.class(h) == GoodClass::Fluid);
            if other {
                0
            } else {
                (inv.fluid_capacity - contents.get(g)).max(0)
            }
        }
        GoodClass::Energy => {
            if !inv.kind.has(InvKind::ENERGY) {
                return 0;
            }
            let held: i64 = contents
                .iter()
                .filter(|(h, _)| goods.class(*h) == GoodClass::Energy)
                .map(|(_, c)| c)
                .sum();
            (inv.energy_capacity - held).max(0)
        }
        // Deposits are not inventories; their deltas are routed to the plugin.
        GoodClass::Deposit => 0,
    }
}

/// Distributes `delta` (per good, the district's change) over `invs`.
pub fn plan(invs: &[InvState], delta: &GoodVec, goods: &impl GoodLookup) -> Plan {
    let mut contents: Vec<GoodVec> = invs.iter().map(|i| i.contents.clone()).collect();
    let mut removals: Vec<Planned> = Vec::new();
    let mut additions: Vec<Planned> = Vec::new();
    let mut unplaced = GoodVec::new();
    // Removals first: they free room for the additions.
    for (g, d) in delta.iter().filter(|e| e.1 < 0) {
        if goods.class(g) == GoodClass::Deposit {
            continue;
        }
        let mut left = -d;
        for postbox in [false, true] {
            if left == 0 {
                break;
            }
            let idx: Vec<usize> = (0..invs.len())
                .filter(|&i| invs[i].kind.has(InvKind::POSTBOX) == postbox && contents[i].get(g) > 0)
                .collect();
            let weights: Vec<i64> = idx.iter().map(|&i| contents[i].get(g)).collect();
            let take = left.min(weights.iter().sum());
            for (k, share) in apportion(take, &weights).into_iter().enumerate() {
                if share > 0 {
                    let i = idx[k];
                    contents[i].add(g, -share);
                    removals.push(Planned {
                        inventory: invs[i].index,
                        good: g,
                        delta: -share,
                    });
                    left -= share;
                }
            }
        }
        if left > 0 {
            unplaced.add(g, -left);
        }
    }
    for (g, d) in delta.iter().filter(|e| e.1 > 0) {
        if goods.class(g) == GoodClass::Deposit {
            continue;
        }
        let mut left = d;
        let tiers: [Box<dyn Fn(usize) -> bool>; 3] = [
            Box::new(|i: usize| invs[i].received.contains(&g)),
            Box::new(|i: usize| invs[i].contents.get(g) > 0),
            Box::new(|_| true),
        ];
        for tier in &tiers {
            for i in 0..invs.len() {
                if left == 0 {
                    break;
                }
                if !tier(i) {
                    continue;
                }
                let r = room_for(&invs[i], &contents[i], g, goods);
                let put = r.min(left);
                if put > 0 {
                    contents[i].add(g, put);
                    additions.push(Planned {
                        inventory: invs[i].index,
                        good: g,
                        delta: put,
                    });
                    left -= put;
                }
            }
        }
        if left > 0 {
            unplaced.add(g, left);
        }
    }
    removals.sort();
    additions.sort();
    // Merge duplicates (an inventory touched by two tiers).
    let merge = |v: Vec<Planned>| -> Vec<Planned> {
        let mut out: Vec<Planned> = Vec::new();
        for p in v {
            match out.last_mut() {
                Some(l) if l.inventory == p.inventory && l.good == p.good => l.delta += p.delta,
                _ => out.push(p),
            }
        }
        out
    };
    let mut deltas = merge(removals);
    deltas.extend(merge(additions));
    Plan { deltas, unplaced }
}

/// Outputs to trim for removal shortfalls, by the model's rates (fixed point). `planned` caps each
/// trim at what was to be added.
pub fn trims(shortfall: &GoodVec, rates: &[(Good, Diffuser)], planned: &GoodVec) -> GoodVec {
    let mut out = GoodVec::new();
    for &(o, ro) in rates {
        let ro = i128::from(ro.rate_fixed());
        if ro <= 0 {
            continue;
        }
        let mut worst: i128 = 0;
        for (c, s) in shortfall.iter() {
            if s <= 0 {
                continue;
            }
            let Some(&(_, rc)) = rates.iter().find(|r| r.0 == c) else {
                continue;
            };
            let rc = i128::from(rc.rate_fixed());
            if rc >= 0 {
                continue;
            }
            worst = worst.max(i128::from(s) * ro / -rc);
        }
        let t = (worst as i64).min(planned.get(o).max(0));
        out.add(o, t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn goods() -> BTreeMap<Good, GoodInfo> {
        BTreeMap::from([
            (Good(1), GoodInfo::item("ore", 64)),
            (Good(2), GoodInfo::item("ingot", 64)),
            (Good(3), GoodInfo::item("pick", 1)),
            (Good(4), GoodInfo::fluid("water")),
        ])
    }

    fn inv(index: u32, slots: u32, contents: GoodVec, received: &[u32]) -> InvState {
        InvState {
            index,
            pos: BlockPos::new(index as i32, 64, 0),
            chunk: ChunkPos::default(),
            kind: InvKind(InvKind::ITEMS),
            item_slots: slots,
            fluid_capacity: 0,
            energy_capacity: 0,
            contents,
            received: received.iter().map(|&g| Good(g)).collect(),
        }
    }

    #[test]
    fn apportion_is_proportional_and_capped() {
        assert_eq!(apportion(10, &[1, 1, 1]), vec![1, 1, 1]);
        assert_eq!(apportion(2, &[1, 1, 1]), vec![1, 1, 0]);
        assert_eq!(apportion(5, &[10, 0, 30]), vec![1, 0, 4]);
        assert_eq!(apportion(0, &[4]), vec![0]);
    }

    #[test]
    fn removals_then_additions_fit() {
        let invs = vec![
            inv(0, 4, GoodVec::from_pairs([(Good(1), 200)]), &[1]),
            inv(1, 2, GoodVec::from_pairs([(Good(2), 64)]), &[2]),
        ];
        let delta = GoodVec::from_pairs([(Good(1), -150), (Good(2), 100)]);
        let p = plan(&invs, &delta, &goods());
        // 64 more ingots fit in inventory 1 (one free slot); the other 36 spill into inventory 0,
        // which the ore removal freed.
        assert_eq!(
            p.deltas,
            vec![
                Planned { inventory: 0, good: Good(1), delta: -150 },
                Planned { inventory: 0, good: Good(2), delta: 36 },
                Planned { inventory: 1, good: Good(2), delta: 64 },
            ]
        );
        assert!(p.unplaced.is_empty());
    }

    #[test]
    fn unstackables_take_whole_slots() {
        let invs = vec![inv(0, 3, GoodVec::new(), &[3])];
        let p = plan(&invs, &GoodVec::from_pairs([(Good(3), 5)]), &goods());
        assert_eq!(p.deltas[0].delta, 3);
        assert_eq!(p.unplaced.get(Good(3)), 2);
    }

    #[test]
    fn trims_follow_the_rates() {
        let rates = [(Good(1), Diffuser::per(-2, 100)), (Good(2), Diffuser::per(3, 100))];
        let planned = GoodVec::from_pairs([(Good(2), 1000)]);
        let t = trims(&GoodVec::from_pairs([(Good(1), 10)]), &rates, &planned);
        assert_eq!(t.get(Good(2)), 15);
        let t = trims(&GoodVec::from_pairs([(Good(1), 10_000)]), &rates, &planned);
        assert_eq!(t.get(Good(2)), 1000, "never trims more than was added");
    }

    proptest! {
        /// The plan changes each good by exactly the delta less what it could not place, takes
        /// nothing an inventory did not hold, and never overfills a slot.
        #[test]
        fn plans_conserve_and_fit(
            held in prop::collection::vec((0i64..300, 0i64..300), 1..6),
            slots in prop::collection::vec(1u32..10, 6),
            d_ore in -600i64..600,
            d_ingot in -600i64..600,
        ) {
            let invs: Vec<InvState> = held.iter().enumerate().map(|(i, &(o, g))| {
                inv(i as u32, slots[i] + 10, GoodVec::from_pairs([(Good(1), o), (Good(2), g)]), &[1, 2])
            }).collect();
            let total_ore: i64 = held.iter().map(|h| h.0).sum();
            let total_ingot: i64 = held.iter().map(|h| h.1).sum();
            let delta = GoodVec::from_pairs([(Good(1), d_ore.max(-total_ore)), (Good(2), d_ingot.max(-total_ingot))]);
            let p = plan(&invs, &delta, &goods());
            let mut applied = GoodVec::new();
            let mut after: Vec<GoodVec> = invs.iter().map(|i| i.contents.clone()).collect();
            for d in &p.deltas {
                applied.add(d.good, d.delta);
                after[d.inventory as usize].add(d.good, d.delta);
            }
            for g in [Good(1), Good(2)] {
                prop_assert_eq!(applied.get(g) + p.unplaced.get(g), delta.get(g));
            }
            for (i, a) in after.iter().enumerate() {
                for (_, c) in a.iter() { prop_assert!(c >= 0); }
                let used: i64 = a.iter().map(|(_, c)| (c + 63) / 64).sum();
                prop_assert!(used <= i64::from(invs[i].item_slots), "inventory {i} overfilled");
            }
        }
    }
}
