// SPDX-License-Identifier: AGPL-3.0-or-later
//! What dragons take, and where it goes (PLAN.md §9; P12's recommendation).
//!
//! **Prey** in phase 2: caravan flows (a hazard on the edge), village livestock counted as market
//! stock, and player infrastructure (INDUSTRY.md §13) when a grudge sends the dragon there. Wild
//! herds wait for the ecology phase (P12).
//!
//! **Conservation.** Dragons have no haven. Whatever they take leaves the world's books as
//! destroyed (`rtsim-world`'s `outside.destroyed` and lines' `book.destroyed`, the industry
//! engine's `books.stolen`), and this ledger says where it went: **eaten** or **burnt**, which are
//! consumption, or **hoarded** (valuables), which is a holder whose contents stay countable until a
//! player robs the hoard. Per good: taken = eaten + burnt + hoarded, and held in hoards = hoarded −
//! robbed.

use std::collections::BTreeMap;

use rtsim_industry::ids::Good;
use rtsim_trade::GoodId;
use serde::Serialize;

/// A good in either namespace: the phase 1 world's (thousandths) or the industry engine's (whole
/// items). In game both are Minecraft items through the bridge's dictionary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    World(GoodId),
    Industry(Good),
}

impl Item {
    /// Units per whole item.
    pub fn unit(self) -> i64 {
        match self {
            Item::World(_) => rtsim_trade::MILLI,
            Item::Industry(_) => 1,
        }
    }

    pub fn key(self) -> String {
        match self {
            Item::World(g) => format!("world:{}", g.0),
            Item::Industry(g) => format!("industry:{}", g.0),
        }
    }
}

/// What a dragon does with a good it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PreyClass {
    /// Eaten: fullness per whole item. `herd` marks live animals kept as a village's stock.
    Food { per_item: i32, herd: bool },
    /// Carried to the hoard.
    Valuable,
    /// Burnt or dropped: destroyed.
    Other,
}

/// The prey table: classes by item name, data. In game an Ice and Fire dragon hunts what its
/// `dragon_targets` tag lists (cow, sheep, pig, chicken, llama, trader llama and more, verified)
/// and a living prey's food points are `round(width × height × 10)` of its box (Uranus
/// `FoodUtils.getFoodPoints`, verified). The livestock values below apply that rule to vanilla's
/// cow (0.9 × 1.4), sheep (0.9 × 1.3) and pig (0.9 × 0.9) boxes (box sizes from vanilla, not
/// checked in the jar). Meat items and every other value are TUNABLE PLACEHOLDERS.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PreyTable {
    pub entries: Vec<(String, PreyClass)>,
}

impl Default for PreyTable {
    fn default() -> Self {
        let food = |n: &str, per_item: i32, herd: bool| (n.to_string(), PreyClass::Food { per_item, herd });
        let val = |n: &str| (n.to_string(), PreyClass::Valuable);
        PreyTable {
            entries: vec![
                // The scenario's livestock goods (rtsim-ecology's synthetic world).
                food("cattle", 13, true),
                food("sheep", 12, true),
                food("pig", 8, true),
                // Meat in a player's boundary inventories.
                food("minecraft:beef", 2, false),
                food("minecraft:porkchop", 2, false),
                food("minecraft:mutton", 2, false),
                food("minecraft:chicken", 1, false),
                food("minecraft:cod", 1, false),
                food("minecraft:salmon", 1, false),
                val("gold_ingot"),
                val("minecraft:gold_ingot"),
                val("minecraft:gold_block"),
                val("minecraft:emerald"),
                val("minecraft:diamond"),
                val("minecraft:amethyst_shard"),
            ],
        }
    }
}

impl PreyTable {
    /// The class of an item by name; a currency is always valuable.
    pub fn class(&self, name: &str, currency: bool) -> PreyClass {
        if currency {
            return PreyClass::Valuable;
        }
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map_or(PreyClass::Other, |e| e.1)
    }
}

/// Where taken goods went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreyLedger {
    pub taken: BTreeMap<Item, i128>,
    pub eaten: BTreeMap<Item, i128>,
    pub burnt: BTreeMap<Item, i128>,
    pub hoarded: BTreeMap<Item, i128>,
    /// Taken from hoards by players (they leave the sim to the player).
    pub robbed: BTreeMap<Item, i128>,
}

impl PreyLedger {
    /// Records `amount` of `item` taken and sends it on by class. Returns the fullness gained
    /// (whole items × the food value; part items round down).
    pub fn take(&mut self, item: Item, amount: i64, class: PreyClass) -> i32 {
        if amount <= 0 {
            return 0;
        }
        *self.taken.entry(item).or_default() += i128::from(amount);
        match class {
            PreyClass::Food { per_item, .. } => {
                *self.eaten.entry(item).or_default() += i128::from(amount);
                (i128::from(amount) * i128::from(per_item) / i128::from(item.unit()))
                    .clamp(0, i128::from(i32::MAX)) as i32
            }
            PreyClass::Valuable => {
                *self.hoarded.entry(item).or_default() += i128::from(amount);
                0
            }
            PreyClass::Other => {
                *self.burnt.entry(item).or_default() += i128::from(amount);
                0
            }
        }
    }

    pub fn rob(&mut self, item: Item, amount: i64) {
        if amount > 0 {
            *self.robbed.entry(item).or_default() += i128::from(amount);
        }
    }

    /// Taken = eaten + burnt + hoarded, per good.
    pub fn check_split(&self) -> Result<(), String> {
        for (item, &t) in &self.taken {
            let g = |m: &BTreeMap<Item, i128>| m.get(item).copied().unwrap_or(0);
            let parts = g(&self.eaten) + g(&self.burnt) + g(&self.hoarded);
            if parts != t {
                return Err(format!("{}: taken {t} but eaten + burnt + hoarded {parts}", item.key()));
            }
        }
        Ok(())
    }

    /// What hoards should hold per good: hoarded − robbed.
    pub fn in_hoards(&self) -> BTreeMap<Item, i128> {
        let mut out = self.hoarded.clone();
        for (k, v) in &self.robbed {
            *out.entry(*k).or_default() -= v;
        }
        out.retain(|_, v| *v != 0);
        out
    }

    pub fn total_world(&self, m: &BTreeMap<Item, i128>) -> BTreeMap<GoodId, i128> {
        m.iter()
            .filter_map(|(k, v)| match k {
                Item::World(g) => Some((*g, *v)),
                Item::Industry(_) => None,
            })
            .collect()
    }

    pub fn total_industry(&self, m: &BTreeMap<Item, i128>) -> BTreeMap<Good, i128> {
        m.iter()
            .filter_map(|(k, v)| match k {
                Item::Industry(g) => Some((*g, *v)),
                Item::World(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_split_balances_and_food_fills() {
        let t = PreyTable::default();
        let mut l = PreyLedger::default();
        let cattle = Item::World(GoodId(3));
        let gold = Item::World(GoodId(7));
        let wool = Item::World(GoodId(5));
        assert_eq!(l.take(cattle, 2_500, t.class("cattle", false)), 32);
        assert_eq!(l.take(gold, 4_000, t.class("gold_ingot", false)), 0);
        assert_eq!(l.take(wool, 1_000, t.class("wool", false)), 0);
        assert_eq!(t.class("emerald", true), PreyClass::Valuable);
        l.rob(gold, 1_000);
        l.check_split().unwrap();
        assert_eq!(l.in_hoards().get(&gold), Some(&3_000));
        assert_eq!(l.burnt.get(&wool), Some(&1_000));
    }
}
