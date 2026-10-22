// SPDX-License-Identifier: AGPL-3.0-or-later
//! A synthetic dragon world and a fake Ice and Fire plugin, for the tests, `rtsim-cli sim
//! dragons` and the bench. **Scaffolding**, like `rtsim-world`'s and `rtsim-industry`'s synthetic
//! worlds: delete it once real worlds can be loaded.
//!
//! - [`goods`], [`recipes`], [`nodes`]: a phase 1 world whose villages keep livestock (cattle and
//!   sheep) as market stock and trade it along caravan roads (P12's recommendation).
//! - [`lairs`]: roosts and caves with their dragons.
//! - [`FakeIaf`]: what the plugin would do with each command, through the boundary codec: bodies
//!   that materialize, fly off when told, and come back captured; a scripted player who wounds or
//!   kills what it meets.
//! - [`grid`]: a bigger world for the bench.
//!
//! Every number here is a scenario choice, not a game fact.

use std::collections::BTreeMap;

use rtsim_industry::fake::{self, FDistrict, FInventory, FakeWorld, Visit, exposed, survey};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{BlockFlags, InvKind};
use rtsim_industry::threat::ThreatClass;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::labour::{Recipe, RecipeBook, ResourceId};
use rtsim_trade::{Amount, GoodDef, GoodId, GoodKind, Goods, MILLI};
use rtsim_world::{NodeData, World, WorldConfig};

use crate::boundary::{self, Cmd, Obs, SpeciesDict, Uuid};
use crate::dragon::{DragonId, LairId, LairKind, agent_of};
use crate::engine::{Dragons, NewDragon};
use crate::species::{Gender, Species};
use crate::tunables::DragonTunables;

pub const FARMLAND: ResourceId = ResourceId(0);
pub const PASTURE: ResourceId = ResourceId(1);
pub const IRON_ORE: ResourceId = ResourceId(2);
pub const SHEEPWALK: ResourceId = ResourceId(3);
pub const EMERALD_ORE: ResourceId = ResourceId(4);
pub const GOLD: ResourceId = ResourceId(5);

pub struct DragonGoods {
    pub wheat: GoodId,
    pub bread: GoodId,
    pub cattle: GoodId,
    pub sheep: GoodId,
    pub leather: GoodId,
    pub wool: GoodId,
    pub iron: GoodId,
    pub gold: GoodId,
    pub emerald: GoodId,
    pub amethyst: GoodId,
}

pub fn goods() -> (Goods, DragonGoods) {
    let mut g = Goods::new();
    let item = |name: &str, need: Amount, stack: u32| GoodDef {
        name: name.into(),
        kind: GoodKind::Item,
        need_per_pop_day: need,
        stack,
    };
    let ids = DragonGoods {
        wheat: g.add(item("wheat", 0, 64)),
        bread: g.add(item("bread", 300, 64)),
        // Live animals: a village's herd is its stock of these (P12).
        cattle: g.add(item("cattle", 150, 1)),
        sheep: g.add(item("sheep", 120, 1)),
        leather: g.add(item("leather", 40, 64)),
        wool: g.add(item("wool", 40, 64)),
        iron: g.add(item("iron_ingot", 20, 64)),
        gold: g.add(item("gold_ingot", 5, 64)),
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

pub fn recipes(s: &DragonGoods) -> RecipeBook {
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
            r(
                "farm wheat",
                s.wheat,
                10,
                vec![(s.wheat, 1)],
                1.0,
                Some(FARMLAND),
            ),
            r("bake bread", s.bread, 1, vec![(s.wheat, 3)], 0.1, None),
            r("raise cattle", s.cattle, 1, vec![], 1.0, Some(PASTURE)),
            r("raise sheep", s.sheep, 2, vec![], 1.0, Some(SHEEPWALK)),
            r("tan leather", s.leather, 2, vec![], 1.0, Some(PASTURE)),
            r("shear wool", s.wool, 4, vec![], 1.0, Some(SHEEPWALK)),
            r("smelt iron", s.iron, 1, vec![], 2.0, Some(IRON_ORE)),
            r("pan gold", s.gold, 1, vec![], 3.0, Some(GOLD)),
            r("mine emerald", s.emerald, 1, vec![], 3.0, Some(EMERALD_ORE)),
        ],
    }
}

fn village(key: u64, x: i32, z: i32, pop: u32, r: &[(ResourceId, i64)]) -> NodeData {
    NodeData {
        key,
        kind: NodeKind::Village(Culture::Human),
        pos: Pos::new(0, x, z),
        pop,
        resources: r
            .iter()
            .map(|&(id, per_day)| (id, per_day * MILLI))
            .collect(),
    }
}

/// The scenario's villages, by key.
pub const VILLAGES: [(u64, &str); 8] = [
    (1, "Brindle"),
    (2, "Oakford"),
    (3, "Highmoor"),
    (4, "Ashby"),
    (5, "Fernwick"),
    (6, "Coldwell"),
    (7, "Marrowdale"),
    (8, "Thornbury"),
];

/// The player's base (C26), a market node with no storehouse.
pub const PLAYER_BASE_KEY: u64 = 9;

pub fn village_name(key: u64) -> &'static str {
    VILLAGES
        .iter()
        .find(|v| v.0 == key)
        .map_or("the player's farm", |v| v.1)
}

pub fn nodes() -> Vec<NodeData> {
    vec![
        village(1, 0, 0, 20, &[(FARMLAND, 120)]),
        village(2, 700, 0, 16, &[(PASTURE, 10), (IRON_ORE, 4)]),
        village(3, 1400, -400, 12, &[(SHEEPWALK, 10), (EMERALD_ORE, 2)]),
        village(4, 700, 700, 15, &[(FARMLAND, 80), (SHEEPWALK, 6)]),
        village(5, -700, 400, 14, &[(FARMLAND, 40), (PASTURE, 6)]),
        village(6, 1500, 600, 10, &[(IRON_ORE, 3), (GOLD, 2)]),
        village(7, -600, -700, 12, &[(PASTURE, 10), (SHEEPWALK, 4)]),
        village(8, 100, -1200, 12, &[(FARMLAND, 60), (GOLD, 1)]),
        NodeData {
            key: PLAYER_BASE_KEY,
            kind: NodeKind::PlayerBase { owner: 1 },
            pos: Pos::new(0, 300, 1300),
            pop: 0,
            resources: BTreeMap::new(),
        },
    ]
}

pub fn world(seed: u64) -> (World, DragonGoods) {
    let (g, ids) = goods();
    let book = recipes(&ids);
    let w = World::build(WorldConfig::new(seed), g, book, nodes());
    (w, ids)
}

/// A lair of the scenario and who lives in it.
pub struct LairSpec {
    pub key: u64,
    pub name: &'static str,
    pub species: Species,
    pub kind: LairKind,
    pub x: i32,
    pub z: i32,
    pub y: i32,
    /// (name, gender, age in days, fullness).
    pub dragons: Vec<(&'static str, Gender, u64, i32)>,
}

pub fn lairs() -> Vec<LairSpec> {
    use Gender::*;
    vec![
        LairSpec {
            key: 101,
            name: "Ashpeak Roost",
            species: Species::Fire,
            kind: LairKind::Roost,
            x: 1000,
            z: 1100,
            y: 110,
            dragons: vec![("Cinderjaw", Female, 110, 70), ("Emberling", Male, 32, 80)],
        },
        LairSpec {
            key: 102,
            name: "Rimefang Roost",
            species: Species::Ice,
            kind: LairKind::Roost,
            x: -500,
            z: -1500,
            y: 120,
            dragons: vec![("Frostmaw", Male, 105, 64)],
        },
        LairSpec {
            key: 103,
            name: "Stormcrag Cave",
            species: Species::Lightning,
            kind: LairKind::Cave,
            x: -1500,
            z: 300,
            y: 40,
            dragons: vec![("Stormtalon", Female, 90, 75)],
        },
        LairSpec {
            key: 104,
            name: "Cinder Hollow",
            species: Species::Fire,
            kind: LairKind::Cave,
            x: 2300,
            z: -600,
            y: 35,
            dragons: vec![("Ashclaw", Male, 80, 62)],
        },
        LairSpec {
            key: 105,
            name: "Old Scorch Roost",
            species: Species::Fire,
            kind: LairKind::Roost,
            x: 2600,
            z: 1600,
            y: 115,
            dragons: vec![],
        },
    ]
}

/// The scenario's dragons placed in the world: lairs join, their dragons are adopted (as the
/// plugin's DRAGON_UNTAGGED would bring them), and the first dragon of a lair is the parent of
/// the second.
pub fn populate(
    w: &mut World,
    d: &mut Dragons,
    specs: &[LairSpec],
    at: Tick,
) -> Vec<(LairId, Vec<DragonId>)> {
    let mut out = Vec::new();
    for s in specs {
        let l = d.add_lair(
            w,
            s.key,
            s.name,
            s.species,
            s.kind,
            Pos::new(0, s.x, s.z),
            s.y,
            at,
        );
        let mut ids = Vec::new();
        for (k, &(name, gender, age, fullness)) in s.dragons.iter().enumerate() {
            let id = d.add_dragon(
                w,
                NewDragon {
                    name: name.to_string(),
                    species: s.species,
                    gender,
                    variant: (s.key as u8 + k as u8) % 4,
                    age_ticks: age * TICKS_PER_DAY,
                    lair: Some(l),
                    fullness,
                    health: None,
                    seed: None,
                    at: None,
                },
                at,
            );
            ids.push(id);
        }
        if ids.len() >= 2 {
            d.set_parent(w, ids[1], ids[0], at);
        }
        out.push((l, ids));
    }
    out
}

/// `n` lairs of `per_lair` dragons spread over `world`'s extent, for the bench.
pub fn bench_lairs(w: &World, n: usize, per_lair: usize, seed: u64) -> Vec<LairSpec> {
    let (mut x0, mut x1, mut z0, mut z1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for node in &w.nodes {
        x0 = x0.min(node.pos.x);
        x1 = x1.max(node.pos.x);
        z0 = z0.min(node.pos.z);
        z1 = z1.max(node.pos.z);
    }
    let key = rtsim_kernel::rng::StreamKey::new(
        seed,
        rtsim_kernel::rng::SystemId::named("synth.lairs"),
        0,
    );
    (0..n)
        .map(|i| {
            let mut r = key.slot(i as u64);
            let x = x0 + r.below((x1 - x0).max(1) as u64) as i32;
            let z = z0 + r.below((z1 - z0).max(1) as u64) as i32;
            let species = Species::ALL[r.below(3) as usize];
            LairSpec {
                key: 10_000 + i as u64,
                name: "bench lair",
                species,
                kind: LairKind::Roost,
                x,
                z,
                y: 100,
                dragons: (0..per_lair)
                    .map(|k| {
                        (
                            "bench dragon",
                            if k % 2 == 0 {
                                Gender::Female
                            } else {
                                Gender::Male
                            },
                            60 + r.below(60),
                            30 + r.below(60) as i32,
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

/// A world of `n` villages on a grid (the phase 1 bench's layout) with livestock and gold.
pub fn grid(n: usize, seed: u64) -> World {
    let (g, ids) = goods();
    let book = recipes(&ids);
    let side = (n as f64).sqrt().ceil() as usize;
    let key = rtsim_kernel::rng::StreamKey::new(
        seed,
        rtsim_kernel::rng::SystemId::named("synth.dgrid"),
        0,
    );
    let all = [FARMLAND, PASTURE, IRON_ORE, SHEEPWALK, EMERALD_ORE, GOLD];
    let data = (0..n)
        .map(|i| {
            let (gx, gz) = ((i % side) as i32, (i / side) as i32);
            let mut r = key.slot(i as u64);
            let mut resources = BTreeMap::new();
            for _ in 0..2 {
                let res = all[r.below(all.len() as u64) as usize];
                resources.insert(res, (2 + r.below(20) as i64) * MILLI);
            }
            resources.insert(FARMLAND, (20 + r.below(60) as i64) * MILLI);
            NodeData {
                key: 1000 + i as u64,
                kind: NodeKind::Village(Culture::Human),
                pos: Pos::new(
                    0,
                    gx * 544 + r.below(200) as i32 - 100,
                    gz * 544 + r.below(200) as i32 - 100,
                ),
                pop: 8 + r.below(16) as u32,
                resources,
            }
        })
        .collect();
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    World::build(cfg, g, book, data)
}

pub fn dragons(seed: u64, tun: DragonTunables) -> Dragons {
    Dragons::new(seed, tun, Tick::ZERO)
}

/// The farm district's id and goods.
pub const FARM: DistrictId = DistrictId(1);
pub const WHEAT: Good = Good(1);
pub const BEEF: Good = Good(2);
pub const GOLD_ITEM: Good = Good(3);
/// The oil farm's fuel (OILGATE.md OG-5), in mB.
pub const DIESEL: Good = Good(4);

/// The farm: a roofless, unwalled industry district at the player's home (300, 1300), with a
/// vault of wheat, beef and gold, a harvester and a butcher; `turrets` per threat class; loaded
/// as `visits` say (the industry fake world's own timetable).
pub fn farm(seed: u64, turrets: [f32; 4], visits: Vec<Visit>) -> FakeWorld {
    oil_farm(seed, turrets, visits, 0)
}

/// The farm with a diesel tank of `diesel_mb` beside its vault (none when 0): what a dry dragon
/// drinks (OG-5).
pub fn oil_farm(seed: u64, turrets: [f32; 4], visits: Vec<Visit>, diesel_mb: i64) -> FakeWorld {
    let mut tun = Tunables::default();
    // The industry default carries nothing for flying attackers, on the strength of a dragon
    // `stealItems` key; the Ice and Fire jar check found `stealItems` is a pixie key, not a
    // dragon one (docs/overnight-choices-dragons.md). The scenario lets a dragon carry off 32 items a pass, so it eats
    // meat and hoards gold. TUNABLE, PLACEHOLDER.
    tun.threat.carry_per_attacker[ThreatClass::Flying.index()] = 32;
    let mut w = FakeWorld::new(seed, tun);
    w.good(WHEAT.0, GoodInfo::item("minecraft:wheat", 64));
    w.good(BEEF.0, GoodInfo::item("minecraft:beef", 64));
    w.good(GOLD_ITEM.0, GoodInfo::item("minecraft:gold_ingot", 64));
    let (bx, bz) = (300, 1300);
    let vault = BlockPos::new(bx, 64, bz);
    let harvester = BlockPos::new(bx + 4, 64, bz);
    let butcher = BlockPos::new(bx + 6, 64, bz + 2);
    let mut d = FDistrict {
        id: FARM,
        name: "Home Farm".to_string(),
        dim: 0,
        inventories: vec![FInventory {
            index: 0,
            pos: vault,
            kind: InvKind::ITEMS,
            slots: 40,
            fluid_cap: 0,
            energy_cap: 0,
            contents: GoodVec::from_pairs([(WHEAT, 600), (BEEF, 300), (GOLD_ITEM, 64)]),
            destroyed: false,
        }],
        recipes: vec![
            fake::Recipe {
                name: "harvester".into(),
                inputs: vec![],
                outputs: vec![(WHEAT, 0, 8)],
                period: 600,
                target: Some((WHEAT, 0, 1_200, 1_000)),
                deposit_per_batch: 0,
                power_from: None,
                machine: harvester,
            },
            fake::Recipe {
                name: "butcher".into(),
                inputs: vec![],
                outputs: vec![(BEEF, 0, 1)],
                period: 2_400,
                target: Some((BEEF, 0, 600, 500)),
                deposit_per_batch: 0,
                power_from: None,
                machine: butcher,
            },
        ],
        deposit: None,
        deposit_chunk: ChunkPos {
            dim: 0,
            x: bx >> 4,
            z: bz >> 4,
        },
        exposed: Vec::new(),
        survey: Some(survey(FARM, 40, 4, 64, 0, 2, turrets)),
        spawn_rate: 0.0,
        loaded: false,
        epochs: Default::default(),
        held: Vec::new(),
        sweep: 0,
        threshold_on: Vec::new(),
        last_ran: Vec::new(),
        broken: Default::default(),
        revision: 0,
        ever_loaded: false,
    };
    let roofless = BlockFlags::ROOFLESS;
    d.exposed = vec![
        exposed(harvester, 11, BlockFlags::MACHINE | roofless, None),
        exposed(butcher, 12, BlockFlags::MACHINE | roofless, None),
        exposed(vault, 13, BlockFlags::INVENTORY, Some(0)),
    ];
    for i in 0..6 {
        d.exposed.push(exposed(
            BlockPos::new(bx - 3 + i, 64, bz - 3),
            20 + i as u32,
            roofless,
            None,
        ));
    }
    if diesel_mb > 0 {
        w.good(DIESEL.0, GoodInfo::fluid("createdieselgenerators:diesel"));
        let tank = BlockPos::new(bx, 64, bz + 3);
        d.inventories.push(FInventory {
            index: 1,
            pos: tank,
            kind: InvKind::FLUIDS,
            slots: 0,
            fluid_cap: diesel_mb.max(64_000),
            energy_cap: 0,
            contents: GoodVec::from_pairs([(DIESEL, diesel_mb)]),
            destroyed: false,
        });
        d.exposed
            .push(exposed(tank, 14, BlockFlags::INVENTORY | roofless, Some(1)));
    }
    w.districts.push(d);
    w.visits = visits;
    w.declare();
    w
}

// ---- the fake plugin --------------------------------------------------------------------------

/// What a scripted player does to a dragon that materializes near them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reaction {
    /// Watches it.
    Ignore,
    /// Shoots it for this share of its max health, `after` ticks into the visit.
    Wound { share: f32, after: u64 },
    /// Kills it `after` ticks into the visit.
    Kill { after: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    pub agent: u64,
    pub epoch: u32,
    pub species: Species,
    pub health: f32,
    pub max_health: f32,
    pub hunger: i32,
    pub hunger_at: Tick,
    pub age_ticks: i32,
    pub pos: (f64, f64, f64),
    pub entity: Uuid,
    pub leaving: bool,
}

/// The plugin's side, headless: commands through the codec in, observations through the codec
/// out, each a tick or more late (docs/boundary.md §1.2). Cloned, it is the same plugin at the
/// same moment (a restore test copies it with the engine's save).
#[derive(Clone)]
pub struct FakeIaf {
    pub dict: SpeciesDict,
    pub bodies: BTreeMap<u64, Body>,
    pub hunger_tick_rate: u64,
    queue: Vec<(Tick, u64, Obs)>,
    seq: u64,
    obs_seq: u64,
    cmd_seq: u64,
    pub wire_bytes: u64,
    pub commands: Vec<(u64, String)>,
    /// What the scripted player does to the next dragons it meets, and as whom.
    pub reaction: Reaction,
    pub player: Uuid,
    /// Ticks a departing dragon takes to leave view. PLACEHOLDER 300.
    pub leave_ticks: u64,
    /// The village the player is at (its key), so a dragon materializing there to harass eats
    /// from its herd as Ice and Fire's own hunting would: `(village, prey body, count)`.
    pub village_here: Option<(u64, u32, u32)>,
    /// Fire a body materialized with MAT_OIL breathes, reported 300 ticks into its visit
    /// (DRAGON_BREATHS, OG-4). 0: none.
    pub breaths: u32,
    /// The ground's oil, as CDG's sampler would find it: `(dimension, chunk x, chunk z)` to
    /// bottomless. OIL_SURVEY answers from it (source 2).
    pub ground_oil: BTreeMap<(u32, i32, i32), bool>,
    /// PLACE_LAIR commands carried out: each places a roost 48 blocks east of the asked point and
    /// reports it (LAIR_FOUND), as the bridge's placement and its next lair scan would.
    pub placed: u64,
    /// Oil nests (OG-9e) by id: worms alive or owed, as the plugin's nest data would hold them.
    /// PLACE_NEST answers NEST_FOUND with a heart under the chunk's centre, `depth` blocks below
    /// y 64, unless `nest_no_room`; NEST_RENEW adds its count.
    pub nests: BTreeMap<u64, u32>,
    pub nest_no_room: bool,
    /// NEST_SORTIE commands received.
    pub sorties: u32,
}

impl FakeIaf {
    pub fn new(player: Uuid) -> Self {
        FakeIaf {
            dict: SpeciesDict::default(),
            bodies: BTreeMap::new(),
            hunger_tick_rate: 3_000,
            queue: Vec::new(),
            seq: 0,
            obs_seq: 0,
            cmd_seq: 0,
            wire_bytes: 0,
            commands: Vec::new(),
            reaction: Reaction::Ignore,
            player,
            leave_ticks: 300,
            village_here: None,
            breaths: 0,
            ground_oil: BTreeMap::new(),
            placed: 0,
            nests: BTreeMap::new(),
            nest_no_room: false,
            sorties: 0,
        }
    }

    /// Queues an observation the scripted world makes (a hoard robbed, a dragon untagged).
    pub fn push(&mut self, at: Tick, o: Obs) {
        self.seq += 1;
        self.queue.push((at, self.seq, o));
    }

    /// Applies the engine's commands at `at` (each sent through the codec first).
    pub fn apply(&mut self, cmds: Vec<Cmd>, at: Tick) {
        for c in cmds {
            self.cmd_seq += 1;
            let mut buf = Vec::new();
            boundary::encode_cmd(self.cmd_seq, &c, &self.dict, &mut buf);
            self.wire_bytes += buf.len() as u64;
            let (_, _, back) = boundary::decode_cmd(&buf, &self.dict).expect("decodable command");
            assert_eq!(back, c, "the codec changed a command");
            self.commands.push((at.0, format!("{c:?}")));
            match back {
                Cmd::Materialize(m) => {
                    let entity = (m.agent, u64::from(m.epoch));
                    let max = crate::species::SpeciesData::of(m.species)
                        .max_health_at(m.age_ticks.max(0) as u64);
                    self.bodies.insert(
                        m.agent,
                        Body {
                            agent: m.agent,
                            epoch: m.epoch,
                            species: m.species,
                            health: if m.health > 0.0 { m.health } else { max },
                            max_health: max,
                            hunger: m.hunger,
                            hunger_at: at,
                            age_ticks: m.age_ticks,
                            pos: m.pos,
                            entity,
                            leaving: false,
                        },
                    );
                    self.push(
                        at + 1,
                        Obs::Materialized(boundary::Materialized {
                            agent: m.agent,
                            epoch: m.epoch,
                            dimension: m.dimension,
                            entity,
                        }),
                    );
                    if self.breaths > 0 && m.flags & boundary::MAT_OIL != 0 {
                        self.push(
                            at + 300,
                            Obs::Breaths(boundary::Breaths {
                                agent: m.agent,
                                epoch: m.epoch,
                                count: self.breaths,
                            }),
                        );
                    }
                    if let Some((village, prey, count)) = self.village_here
                        && m.flags & boundary::MAT_SLEEPING == 0
                    {
                        self.push(
                            at + 600,
                            Obs::Ate(boundary::Ate {
                                agent: m.agent,
                                epoch: m.epoch,
                                prey,
                                count,
                                village,
                                pos: m.pos,
                            }),
                        );
                    }
                    match self.reaction {
                        Reaction::Ignore => {}
                        Reaction::Wound { share, after } => {
                            let amount = max * share;
                            let after_h = (if m.health > 0.0 { m.health } else { max }) - amount;
                            self.push(
                                at + after,
                                Obs::Hurt(boundary::Hurt {
                                    agent: m.agent,
                                    epoch: m.epoch,
                                    attacker_kind: boundary::AttackerKind::Player,
                                    attacker: self.player,
                                    hits: 6,
                                    amount,
                                    health_after: after_h.max(1.0),
                                }),
                            );
                        }
                        Reaction::Kill { after } => self.push(
                            at + after,
                            Obs::Died(boundary::Died {
                                agent: m.agent,
                                epoch: m.epoch,
                                killer_kind: boundary::AttackerKind::Player,
                                killer: self.player,
                                dimension: m.dimension,
                                pos: m.pos,
                            }),
                        ),
                    }
                }
                Cmd::Steer(_) => {}
                Cmd::Depart(dp) => {
                    if let Some(b) = self.bodies.get_mut(&dp.agent)
                        && b.epoch == dp.epoch
                    {
                        b.leaving = true;
                        b.pos = dp.target;
                        let t = at + self.leave_ticks.min(u64::from(dp.timeout_ticks));
                        self.capture(dp.agent, 1, t);
                    }
                }
                Cmd::Adopt(a) => self.push(
                    at + 1,
                    Obs::Materialized(boundary::Materialized {
                        agent: a.agent,
                        epoch: a.epoch,
                        dimension: 0,
                        entity: a.entity,
                    }),
                ),
                Cmd::PlaceLair(p) => {
                    self.placed += 1;
                    let (x, z) = (p.x + 48, p.z);
                    self.push(
                        at + 40,
                        Obs::LairFound(boundary::LairFound {
                            lair: 0x504C_0000_0000 | self.placed,
                            late: true,
                            dimension: p.dimension,
                            species: p.species,
                            kind: p.kind,
                            min: (x - 16, 90, z - 16),
                            max: (x + 16, 110, z + 16),
                            home: (x, 100, z),
                        }),
                    );
                }
                Cmd::OilSurvey(sv) => {
                    let r = i64::from(sv.radius);
                    let mut found: Vec<boundary::OilChunkRec> = self
                        .ground_oil
                        .range(
                            (sv.dimension, i32::MIN, i32::MIN)..=(sv.dimension, i32::MAX, i32::MAX),
                        )
                        .filter(|((_, cx, cz), _)| {
                            let dx = i64::from(*cx) * 16 + 8 - i64::from(sv.x);
                            let dz = i64::from(*cz) * 16 + 8 - i64::from(sv.z);
                            dx * dx + dz * dz <= r * r
                        })
                        .map(|(&(_, cx, cz), &deep)| boundary::OilChunkRec {
                            cx,
                            cz,
                            amount: 1,
                            flags: if deep { boundary::OIL_BOTTOMLESS } else { 0 },
                        })
                        .collect();
                    while !found.is_empty() {
                        let rest = found.split_off(found.len().min(boundary::OIL_CHUNKS_MAX));
                        self.push(
                            at + 20,
                            Obs::OilChunks(boundary::OilChunks {
                                dimension: sv.dimension,
                                source: boundary::OIL_SOURCE_CDG,
                                chunks: std::mem::replace(&mut found, rest),
                            }),
                        );
                    }
                }
                Cmd::PlaceNest(p) => {
                    let placed = !self.nest_no_room;
                    if placed {
                        self.nests.entry(p.nest).or_insert(0);
                    }
                    self.push(
                        at + 20,
                        Obs::NestFound(boundary::NestFound {
                            nest: p.nest,
                            placed,
                            dimension: p.dimension,
                            heart: if placed {
                                (p.cx * 16 + 8, 64 - p.depth as i32, p.cz * 16 + 8)
                            } else {
                                (0, 0, 0)
                            },
                        }),
                    );
                }
                Cmd::NestRenew(n) => *self.nests.entry(n.nest).or_insert(0) += n.count,
                Cmd::NestSortie(_) => self.sorties += 1,
                Cmd::Discard(_) => {}
            }
        }
    }

    /// A player kills `count` of nest `nest`'s worms (NEST_LOSS, cause Player).
    pub fn kill_worms(&mut self, nest: u64, count: u32, at: Tick) {
        let have = self.nests.entry(nest).or_insert(0);
        let count = count.min(*have);
        *have -= count;
        if count > 0 {
            let player = self.player;
            self.push(
                at,
                Obs::NestLoss(boundary::NestLoss {
                    nest,
                    count,
                    cause: rtsim_kernel::renewal::LossCause::Player.code(),
                    player,
                }),
            );
        }
    }

    /// The body leaves view or the player leaves: captured and despawned (DEMATERIALIZED).
    pub fn capture(&mut self, agent: u64, reason: u32, at: Tick) {
        let Some(b) = self.bodies.remove(&agent) else {
            return;
        };
        // Its hunger fell while it was a body, as Ice and Fire's own logic would.
        let lost = at.since(b.hunger_at) / self.hunger_tick_rate.max(1);
        let hunger = (i64::from(b.hunger) - lost as i64).max(0) as i32;
        self.push(
            at,
            Obs::Dematerialized(boundary::Dematerialized {
                agent,
                epoch: b.epoch,
                reason,
                dimension: 0,
                health: b.health,
                max_health: b.max_health,
                hunger,
                age_ticks: b.age_ticks,
                pos: b.pos,
            }),
        );
    }

    /// Every body is captured: the player went away (reason 3).
    pub fn player_leaves(&mut self, at: Tick) {
        let agents: Vec<u64> = self.bodies.keys().copied().collect();
        for a in agents {
            self.capture(a, 3, at);
        }
    }

    /// Observations due by `at`, in order, each through the codec.
    pub fn due(&mut self, at: Tick) -> Vec<(Tick, Obs)> {
        self.queue.sort_by_key(|q| (q.0, q.1));
        let n = self.queue.partition_point(|q| q.0 <= at);
        let mut out = Vec::new();
        for (t, _, o) in self.queue.drain(..n) {
            if let Obs::Hurt(h) = &o
                && let Some(b) = self.bodies.get_mut(&h.agent)
            {
                b.health = h.health_after;
            }
            if let Obs::Died(dd) = &o
                && self.bodies.remove(&dd.agent).is_none()
            {
                continue;
            }
            if let Obs::Hurt(h) = &o
                && !self.bodies.contains_key(&h.agent)
            {
                continue;
            }
            if let Obs::Ate(a) = &o {
                match self.bodies.get_mut(&a.agent) {
                    // Ice and Fire's food points for a cow-sized animal (13, see the prey
                    // table), into the body's own hunger.
                    Some(b) => b.hunger = (b.hunger + 13 * a.count as i32).min(100),
                    None => continue,
                }
            }
            self.obs_seq += 1;
            let mut buf = Vec::new();
            boundary::encode_obs(self.obs_seq, t.0 as i64, &o, &self.dict, &mut buf);
            self.wire_bytes += buf.len() as u64;
            let (_, _, _, back) =
                boundary::decode_obs(&buf, &self.dict).expect("decodable observation");
            assert_eq!(back, o, "the codec changed an observation");
            out.push((t, back));
        }
        out
    }

    pub fn next_due(&self) -> Option<Tick> {
        self.queue.iter().map(|q| q.0).min()
    }

    pub fn is_embodied(&self, d: DragonId) -> bool {
        self.bodies.contains_key(&agent_of(d))
    }
}
