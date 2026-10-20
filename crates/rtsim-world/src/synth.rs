// SPDX-License-Identifier: AGPL-3.0-or-later
//! A synthetic phase 1 world for tests, the scenario runner and the bench. Scaffolding, as the
//! kernel's `synth` is: delete it once real worlds can be loaded.
//!
//! Every number here is a PLACEHOLDER chosen to make a small world trade (P18's soak sets the real
//! ones): six human villages, three ribbit villages near a geode, a pillager haven preying on one
//! road, and a player base. Positions follow PLAN.md §8.2's spacing (vanilla candidates every 544
//! blocks, Ribbits every 400, swamp only).

use std::collections::BTreeMap;

use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::labour::{Recipe, RecipeBook, ResourceId};
use rtsim_trade::{Amount, GoodDef, GoodId, GoodKind, Goods, MILLI};

use crate::config::WorldConfig;
use crate::world::{NodeData, World};

pub const FARMLAND: ResourceId = ResourceId(0);
pub const CATTLE: ResourceId = ResourceId(1);
pub const IRON_ORE: ResourceId = ResourceId(2);
pub const SHEEP: ResourceId = ResourceId(3);
pub const EMERALD_ORE: ResourceId = ResourceId(4);
pub const GEODE: ResourceId = ResourceId(5);
pub const SWAMP: ResourceId = ResourceId(6);

/// The goods of the synthetic world, by name.
pub struct SynthGoods {
    pub wheat: GoodId,
    pub bread: GoodId,
    pub leather: GoodId,
    pub iron: GoodId,
    pub wool: GoodId,
    pub slime: GoodId,
    pub clay: GoodId,
    pub emerald: GoodId,
    pub amethyst: GoodId,
}

pub fn goods() -> (Goods, SynthGoods) {
    let mut g = Goods::new();
    let item = |name: &str, need: Amount, stack: u32| GoodDef {
        name: name.into(),
        kind: GoodKind::Item,
        need_per_pop_day: need,
        stack,
    };
    let ids = SynthGoods {
        wheat: g.add(item("wheat", 0, 64)),
        bread: g.add(item("bread", 400, 64)),
        leather: g.add(item("leather", 50, 64)),
        iron: g.add(item("iron_ingot", 30, 64)),
        wool: g.add(item("wool", 50, 64)),
        slime: g.add(item("slime_ball", 20, 64)),
        clay: g.add(item("clay_ball", 30, 64)),
        emerald: g.add(GoodDef {
            name: "emerald".into(),
            kind: GoodKind::Currency(Culture::Human),
            need_per_pop_day: 0,
            stack: 64,
        }),
        amethyst: g.add(GoodDef {
            name: "amethyst_shard".into(),
            kind: GoodKind::Currency(Culture::Ribbit),
            need_per_pop_day: 0,
            stack: 64,
        }),
    };
    (g, ids)
}

pub fn recipes(s: &SynthGoods) -> RecipeBook {
    let r = |name: &str, output, y: i64, inputs: Vec<(GoodId, i64)>, labour, resource| Recipe {
        name: name.into(),
        output,
        yield_per_batch: y * MILLI,
        inputs: inputs.into_iter().map(|(g, a)| (g, a * MILLI)).collect(),
        labour,
        resource,
    };
    RecipeBook {
        recipes: vec![
            r("farm wheat", s.wheat, 10, vec![(s.wheat, 1)], 1.0, Some(FARMLAND)),
            r("bake bread", s.bread, 1, vec![(s.wheat, 3)], 0.1, None),
            r("tan leather", s.leather, 2, vec![], 1.0, Some(CATTLE)),
            r("smelt iron", s.iron, 1, vec![], 2.0, Some(IRON_ORE)),
            r("shear wool", s.wool, 4, vec![], 1.0, Some(SHEEP)),
            r("mine emerald", s.emerald, 1, vec![], 3.0, Some(EMERALD_ORE)),
            // Budding amethyst regrows clusters of 4 shards (economy research).
            r("harvest amethyst", s.amethyst, 4, vec![], 1.0, Some(GEODE)),
            r("gather slime", s.slime, 2, vec![], 1.0, Some(SWAMP)),
            r("dig clay", s.clay, 8, vec![], 1.0, Some(SWAMP)),
        ],
    }
}

fn res(list: &[(ResourceId, i64)]) -> BTreeMap<ResourceId, Amount> {
    list.iter().map(|&(r, per_day)| (r, per_day * MILLI)).collect()
}

/// Keys of the synthetic world's nodes, in the order [`build`] adds them.
pub const NAMES: [&str; 11] = [
    "Brindle", "Oakford", "Highmoor", "Ashby", "Fernwick", "Coldwell", "Mirewater", "Bogsend",
    "Reedholm", "Pillager camp", "Player farm",
];

pub fn nodes() -> Vec<NodeData> {
    let v = |key, x, z, pop, r: &[(ResourceId, i64)]| NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, z),
        pop,
        resources: res(r),
    };
    let rb = |key, x, z, pop, r: &[(ResourceId, i64)]| NodeData {
        key,
        kind: NodeKind::Village(Culture::Ribbit),
        pos: Pos::new(0, x, z),
        pop,
        resources: res(r),
    };
    vec![
        v(1, 0, 0, 20, &[(FARMLAND, 60)]),
        v(2, 600, 0, 18, &[(CATTLE, 6), (IRON_ORE, 4)]),
        v(3, 1200, -300, 12, &[(EMERALD_ORE, 2), (SHEEP, 8)]),
        v(4, 600, 650, 15, &[(FARMLAND, 40), (SHEEP, 6)]),
        v(5, -600, 300, 14, &[(FARMLAND, 20), (CATTLE, 4)]),
        v(6, 1300, 500, 10, &[(IRON_ORE, 3)]),
        rb(7, -500, -600, 12, &[(GEODE, 6), (SWAMP, 4)]),
        rb(8, -1000, -400, 10, &[(SWAMP, 6)]),
        rb(9, -800, -1050, 10, &[(SWAMP, 5), (FARMLAND, 10)]),
        NodeData {
            key: 10,
            kind: NodeKind::Haven(Culture::Human),
            pos: Pos::new(0, 1000, -800),
            pop: 5,
            resources: BTreeMap::new(),
        },
        NodeData {
            key: 11,
            kind: NodeKind::PlayerBase { owner: 1 },
            pos: Pos::new(0, 300, 300),
            pop: 0,
            resources: BTreeMap::new(),
        },
    ]
}

/// The synthetic world, built. With `raid`, pillagers from the haven take a share of what moves
/// on the Oakford to Highmoor road (C24). PLACEHOLDER share: an eighth.
pub fn build(cfg: WorldConfig, raid: bool) -> (World, SynthGoods) {
    let (goods, ids) = goods();
    let book = recipes(&ids);
    let mut w = World::build(cfg, goods, book, nodes());
    if raid {
        let (oak, high, camp) = (1, 2, 9);
        w.set_predation(oak, high, camp, 125);
        w.set_predation(high, oak, camp, 125);
    }
    (w, ids)
}

/// `n` human and ribbit villages on a jittered grid with every resource somewhere, for the scale
/// bench: about one village per 544-block cell, ribbits in a band along one side.
pub fn grid(n: usize, seed: u64) -> Vec<NodeData> {
    let side = (n as f64).sqrt().ceil() as usize;
    let key = rtsim_kernel::rng::StreamKey::new(
        seed,
        rtsim_kernel::rng::SystemId::named("synth.grid"),
        0,
    );
    let all = [FARMLAND, CATTLE, IRON_ORE, SHEEP, EMERALD_ORE, SWAMP];
    (0..n)
        .map(|i| {
            let (gx, gz) = ((i % side) as i32, (i / side) as i32);
            let mut r = key.slot(i as u64);
            let jx = r.below(200) as i32 - 100;
            let jz = r.below(200) as i32 - 100;
            let ribbit = gx < (side as i32) / 4;
            let mut resources = BTreeMap::new();
            for _ in 0..2 {
                let res = all[r.below(all.len() as u64) as usize];
                resources.insert(res, (2 + r.below(40) as i64) * MILLI);
            }
            if ribbit && r.below(3) == 0 {
                resources.insert(GEODE, (2 + r.below(6) as i64) * MILLI);
            }
            NodeData {
                key: 1000 + i as u64,
                kind: NodeKind::Village(if ribbit { Culture::Ribbit } else { Culture::Human }),
                pos: Pos::new(0, gx * 544 + jx, gz * 544 + jz),
                pop: 8 + r.below(16) as u32,
                resources,
            }
        })
        .collect()
}
