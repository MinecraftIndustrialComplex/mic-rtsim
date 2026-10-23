// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the phase 1 world trades and how a village joins it: goods and recipes, their Minecraft
//! item ids, the resources a village of a structure has, its starting population, and Ribbits'
//! house templates.
//!
//! **Every table here is a PLACEHOLDER** (choice V5): the goods and recipes are rtsim-world's
//! catalogue (`rtsim_world::catalogue`, which the viewer reads pages back with), mapped onto
//! vanilla items; a village's resources
//! follow its structure (plains farm, taiga shears and smelts, ...), since nothing the bridge
//! observes says what land a village works. P9 and P18 replace them with measured ones.

use std::collections::BTreeMap;

use rtsim_settle::Culture;
use rtsim_trade::labour::{RecipeBook, ResourceId};
use rtsim_trade::{Amount, GoodId, Goods, MILLI};
use rtsim_world::synth;

/// The goods, the recipes, and the item id of every good.
pub struct Catalog {
    pub goods: Goods,
    pub book: RecipeBook,
    /// Minecraft item id per good, by good index.
    items: Vec<&'static str>,
}

/// The Minecraft item each synthetic good is (by the synthetic good's name).
const ITEMS: [(&str, &str); 10] = [
    ("wheat", "minecraft:wheat"),
    ("bread", "minecraft:bread"),
    ("leather", "minecraft:leather"),
    ("iron_ingot", "minecraft:iron_ingot"),
    ("wool", "minecraft:white_wool"),
    ("slime_ball", "minecraft:slime_ball"),
    ("clay_ball", "minecraft:clay_ball"),
    ("emerald", "minecraft:emerald"),
    ("amethyst_shard", "minecraft:amethyst_shard"),
    ("gold_nugget", "minecraft:gold_nugget"),
];

/// The 14 house templates under `data/ribbits/structure/houses` (Ribbits 4.1.6), in name order:
/// rtsim-world's `PlaceHouse::template` indexes this list.
pub const RIBBIT_HOUSES: [&str; 14] = [
    "ribbits:houses/brown_fishing_house",
    "ribbits:houses/brown_sorcerer_house",
    "ribbits:houses/lg_brown_house_1",
    "ribbits:houses/lg_red_house_1",
    "ribbits:houses/md_brown_house_1",
    "ribbits:houses/md_red_house_1",
    "ribbits:houses/red_fishing_house",
    "ribbits:houses/red_sorcerer_house",
    "ribbits:houses/small_house_brown_1",
    "ribbits:houses/small_house_brown_2",
    "ribbits:houses/small_house_brown_3",
    "ribbits:houses/small_house_red_1",
    "ribbits:houses/small_house_red_2",
    "ribbits:houses/small_house_red_3",
];

/// The processor list Ribbits' village pieces use.
pub const RIBBIT_PROCESSORS: &str = "ribbits:main";
/// A ribbit resident's entity type.
pub const RIBBIT_BODY: &str = "ribbits:ribbit";

/// Group kinds the bridge knows (BodyKinds, the Ribbits plugin).
pub const HUMAN_CARAVAN: &str = "mic_rtsim:human_caravan";
pub const RIBBIT_MERCHANT: &str = "mic_rtsim_ribbits:ribbit_merchant";
pub const RIBBIT_BAND: &str = "mic_rtsim_ribbits:ribbit_band";
/// Members of the plugin's band group: its master and three musicians (one per instrument). The
/// world counts 3 (E57); the puppet has the plugin's 4.
pub const RIBBIT_BAND_MEMBERS: u32 = 4;
/// Chest slots of a trader llama of strength 1 (3 rows of 1), and of a ribbit merchant's pack
/// (G15, PLACEHOLDER).
pub const LLAMA_SLOTS: u32 = 3;
pub const LLAMA_STRENGTH: i32 = 1;
pub const PACK_SLOTS: u32 = 27;

impl Catalog {
    pub fn new() -> Catalog {
        let (goods, book) = rtsim_world::catalogue::catalogue();
        let items = goods
            .ids()
            .map(|g| {
                let name = goods.get(g).name.as_str();
                ITEMS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, item)| *item)
                    .expect("every synthetic good has an item")
            })
            .collect();
        Catalog { goods, book, items }
    }

    pub fn item(&self, g: GoodId) -> &'static str {
        self.items[g.index()]
    }

    /// The item of a good, if the catalogue has that good.
    pub fn try_item(&self, g: GoodId) -> Option<&'static str> {
        self.items.get(g.index()).copied()
    }

    pub fn good(&self, item: &str) -> Option<GoodId> {
        self.items
            .iter()
            .position(|i| *i == item)
            .map(|i| GoodId(i as u16))
    }

    pub fn stack(&self, g: GoodId) -> u32 {
        self.goods.get(g).stack.max(1)
    }
}

/// The culture of a village from its culture name (`human`, `ribbit`: rtsim-settle's
/// `Culture::name`). Anything else is not a culture the phase 1 world trades in.
pub fn culture(name: &str) -> Option<Culture> {
    match name {
        "human" => Some(Culture::Human),
        "ribbit" => Some(Culture::Ribbit),
        _ => None,
    }
}

fn res(list: &[(ResourceId, i64)]) -> BTreeMap<ResourceId, Amount> {
    list.iter().map(|&(r, d)| (r, d * MILLI)).collect()
}

/// The resources a village works, per day, from its structure (PLACEHOLDER, V5).
pub fn resources(structure: &str, culture: Culture) -> BTreeMap<ResourceId, Amount> {
    use synth::{CATTLE, EMERALD_ORE, FARMLAND, GEODE, IRON_ORE, SHEEP, SWAMP};
    if culture == Culture::Ribbit {
        return res(&[(SWAMP, 5), (GEODE, 4)]);
    }
    match structure {
        "minecraft:village_plains" => res(&[(FARMLAND, 100), (CATTLE, 4)]),
        "minecraft:village_desert" => res(&[(FARMLAND, 40), (EMERALD_ORE, 2)]),
        "minecraft:village_savanna" => res(&[(CATTLE, 6), (FARMLAND, 30)]),
        "minecraft:village_taiga" => res(&[(SHEEP, 8), (IRON_ORE, 3)]),
        "minecraft:village_snowy" => res(&[(IRON_ORE, 4), (SHEEP, 4)]),
        _ => res(&[(FARMLAND, 60)]),
    }
}

/// A village's population before its first census (PLACEHOLDER, V5): a third of its structure
/// pieces for a human village (houses, streets and decorations), half for a ribbit one.
pub fn population(pieces: u32, culture: Culture) -> u32 {
    match culture {
        Culture::Ribbit => (pieces / 2).clamp(4, 20),
        _ => (pieces / 3).clamp(4, 30),
    }
}
