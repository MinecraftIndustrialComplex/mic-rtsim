// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic scenarios for `rtsim-cli sim industry` and the end-to-end tests. **Scaffolding**, like
//! [`crate::fake`].
//!
//! **The iron line** (INDUSTRY.md §14.1, made concrete):
//! - *Ore Coast* drills crushed ore from a finite Ore Excavation vein into a vault. Unlit, unwalled
//!   and unroofed.
//! - Train 1 carries the ore to *Smelter*, which smelts it with coal into ingots. A threshold switch
//!   stops it at a full ingot vault. It is walled with two gaps and has anti-air turrets.
//! - The smelter needs power from the *Power Plant* next door, a coal boiler joined to it by a
//!   PowerGrid line (I13).
//! - Train 2 carries the ingots *Home*.
//!
//! The player learns each district in turn, goes home for most of the run, then visits the
//! smelter and the coast again. Every number here is a scenario choice for the demonstration, not
//! a game fact, except where a comment cites one.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

use crate::engine::Mode;
use crate::fake::{FCarriage, FDistrict, FInventory, FStation, FTrain, FakeWorld, Recipe, TrainState, Visit, exposed, survey};
use crate::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, StationId, TrainId};
use crate::obs::{BlockFlags, InvKind};
use crate::threat::{SynthStream, SyntheticThreats, ThreatClass, ThreatSource, ThreatTarget};
use crate::tunables::Tunables;

pub const ORE: Good = Good(1);
pub const INGOT: Good = Good(2);
pub const COAL: Good = Good(3);
pub const VEIN: Good = Good(4);

/// District indices in the iron line.
pub const ORE_COAST: usize = 0;
pub const SMELTER: usize = 1;
pub const HOME: usize = 2;
pub const POWER: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IronLine {
    pub seed: u64,
    pub days: u64,
    pub threats: bool,
    /// Every district chunk-loaded all the time.
    pub control: bool,
    pub mode: Mode,
    /// Machines tick once on stale stock before a reconcile lands (INDUSTRY.md §19 risk 3), and
    /// reconciles land a tick late, as the real protocol does. Off: an ideal, race-free Java side.
    pub tick_first: bool,
}

/// A Create item vault of `blocks` blocks: 20 slots each (`vaultCapacity`, verified).
fn vault(index: u32, pos: BlockPos, blocks: u32, contents: GoodVec) -> FInventory {
    FInventory {
        index,
        pos,
        kind: InvKind::ITEMS,
        slots: 20 * blocks,
        fluid_cap: 0,
        energy_cap: 0,
        contents,
        destroyed: false,
    }
}

fn day(d: f64) -> Tick {
    Tick((d * TICKS_PER_DAY as f64) as u64)
}

fn district(id: u64, name: &str, x: i32) -> FDistrict {
    FDistrict {
        id: DistrictId(id),
        name: name.to_string(),
        dim: 0,
        inventories: Vec::new(),
        recipes: Vec::new(),
        deposit: None,
        deposit_chunk: ChunkPos { dim: 0, x: x >> 4, z: 0 },
        exposed: Vec::new(),
        survey: None,
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
    }
}

pub fn iron_line(cfg: IronLine, tun: Tunables) -> FakeWorld {
    let mut w = FakeWorld::new(cfg.seed, tun, cfg.mode);
    w.control = cfg.control;
    w.machines_tick_first = cfg.tick_first;
    w.apply_on_load = !cfg.tick_first;
    w.good(ORE.0, GoodInfo::item("create:crushed_raw_iron", 64));
    w.good(INGOT.0, GoodInfo::item("minecraft:iron_ingot", 64));
    w.good(COAL.0, GoodInfo::item("minecraft:coal", 64));
    w.good(VEIN.0, GoodInfo::deposit("createoreexcavation:iron_vein"));

    // Ore Coast: a drill on a finite vein, 16 ore per 200 ticks.
    let mut a = district(1, "Ore Coast", 1_600);
    let drill = BlockPos::new(1_602, 64, 4);
    a.inventories.push(vault(0, BlockPos::new(1_600, 64, 0), 2, GoodVec::new()));
    a.recipes.push(Recipe {
        name: "drill".into(),
        inputs: vec![],
        outputs: vec![(ORE, 0, 16)],
        period: 200,
        target: None,
        deposit_per_batch: 16,
        power_from: None,
        machine: drill,
    });
    a.deposit = Some((VEIN, 14_000));
    a.survey = Some(survey(DistrictId(1), 400, 100, 100, 0, 2, [0.0; 4]));
    a.exposed = vec![
        exposed(drill, 11, BlockFlags::MACHINE | BlockFlags::ROOFLESS | BlockFlags::PERIMETER, None),
        exposed(BlockPos::new(1_600, 64, 0), 12, BlockFlags::INVENTORY | BlockFlags::ROOFLESS, Some(0)),
        exposed(BlockPos::new(1_598, 64, 0), 13, BlockFlags::PERIMETER | BlockFlags::ROOFLESS, None),
        exposed(BlockPos::new(1_598, 64, 2), 13, BlockFlags::PERIMETER | BlockFlags::ROOFLESS, None),
    ];
    a.spawn_rate = 5e-7;
    w.districts.push(a);

    // Smelter: 8 ore + 1 coal into 8 ingots per 100 ticks; stops at 2,000 ingots.
    let mut b = district(2, "Smelter", 3_200);
    let furnace = BlockPos::new(3_204, 64, 4);
    b.inventories.push(vault(0, BlockPos::new(3_200, 64, 0), 2, GoodVec::new()));
    b.inventories.push(vault(1, BlockPos::new(3_200, 64, 8), 1, GoodVec::from_pairs([(COAL, 1_200)])));
    b.inventories.push(vault(2, BlockPos::new(3_208, 64, 0), 2, GoodVec::new()));
    b.recipes.push(Recipe {
        name: "smelt".into(),
        inputs: vec![(ORE, 0, 8), (COAL, 1, 1)],
        outputs: vec![(INGOT, 2, 8)],
        period: 100,
        target: Some((INGOT, 2, 2_000, 1_500)),
        deposit_per_batch: 0,
        power_from: Some((POWER, 0)),
        machine: furnace,
    });
    b.survey = Some(survey(DistrictId(2), 0, 2, 100, 1, 2, [0.0, 0.0, 6.0, 0.0]));
    b.exposed = vec![
        exposed(furnace, 21, BlockFlags::MACHINE, None),
        exposed(BlockPos::new(3_208, 64, 0), 22, BlockFlags::INVENTORY | BlockFlags::PERIMETER, Some(2)),
        exposed(BlockPos::new(3_196, 64, 0), 23, BlockFlags::PERIMETER, None),
        exposed(BlockPos::new(3_196, 65, 0), 23, BlockFlags::PERIMETER, None),
        exposed(BlockPos::new(3_196, 64, 4), 23, BlockFlags::PERIMETER, None),
    ];
    w.districts.push(b);

    // Home: receives ingots into a 16-block vault (320 slots).
    let mut c = district(3, "Home", 0);
    c.inventories.push(vault(0, BlockPos::new(0, 64, 0), 16, GoodVec::new()));
    w.districts.push(c);

    // Power Plant: a coal boiler, 1 coal per 400 ticks.
    let mut d = district(4, "Power Plant", 3_300);
    let boiler = BlockPos::new(3_302, 64, 2);
    d.inventories.push(vault(0, BlockPos::new(3_300, 64, 0), 1, GoodVec::from_pairs([(COAL, 300)])));
    d.recipes.push(Recipe {
        name: "boiler".into(),
        inputs: vec![(COAL, 0, 1)],
        outputs: vec![],
        period: 400,
        target: None,
        deposit_per_batch: 0,
        power_from: None,
        machine: boiler,
    });
    w.districts.push(d);
    w.power_links.push((SMELTER, POWER));

    w.stations = vec![
        FStation { id: StationId(1), district: ORE_COAST, pos: BlockPos::new(1_604, 64, 0), unload: vec![], load: vec![(ORE, 0)] },
        FStation { id: StationId(2), district: SMELTER, pos: BlockPos::new(3_202, 64, 0), unload: vec![(ORE, 0)], load: vec![(INGOT, 2)] },
        FStation { id: StationId(3), district: HOME, pos: BlockPos::new(2, 64, 0), unload: vec![(INGOT, 0)], load: vec![] },
        FStation { id: StationId(4), district: SMELTER, pos: BlockPos::new(3_202, 64, 6), unload: vec![], load: vec![(INGOT, 2)] },
    ];
    let cars = |n: u32| (0..n).map(|i| FCarriage { index: i, slots: 27, cargo: GoodVec::new() }).collect();
    w.trains = vec![
        FTrain { id: TrainId(1), stops: vec![0, 1], travel: 1_200, dwell: 200, carriages: cars(2), state: TrainState::Dwelling { stop: 0, since: 0 } },
        FTrain { id: TrainId(2), stops: vec![3, 2], travel: 2_000, dwell: 200, carriages: cars(1), state: TrainState::Dwelling { stop: 0, since: 0 } },
    ];
    // Station 4 is the ingot platform; the ore platform (2) only unloads ore for train 1.
    w.stations[1].load.clear();

    // The player builds and runs everything at first (all loaded, so the control run and the
    // simulated run share their history while districts learn), then goes home.
    w.visits = vec![
        Visit { from: Tick(0), at: vec![ORE_COAST, SMELTER, HOME, POWER] },
        Visit { from: day(1.5), at: vec![HOME] },
        Visit { from: day(8.0), at: vec![SMELTER, POWER] },
        Visit { from: day(8.5), at: vec![ORE_COAST] },
        Visit { from: day(9.0), at: vec![HOME] },
        // The end: everything loads so every reconcile lands, for the conservation check.
        Visit { from: day(cfg.days as f64), at: vec![ORE_COAST, SMELTER, HOME, POWER] },
    ];

    if cfg.threats {
        w.threats = Some(Box::new(SyntheticThreats::new(
            cfg.seed,
            vec![
                SynthStream {
                    source: ThreatSource::Bandit,
                    target: ThreatTarget::District(DistrictId(2)),
                    class: ThreatClass::Siege,
                    mean_interval_ticks: 2 * TICKS_PER_DAY,
                    count: (6, 12),
                    strength_per_attacker: 1.0,
                    start: day(2.0),
                },
                SynthStream {
                    source: ThreatSource::Dragon,
                    target: ThreatTarget::District(DistrictId(1)),
                    class: ThreatClass::Flying,
                    mean_interval_ticks: 3 * TICKS_PER_DAY,
                    count: (1, 1),
                    strength_per_attacker: 20.0,
                    start: day(2.5),
                },
                SynthStream {
                    source: ThreatSource::Bandit,
                    target: ThreatTarget::Train(TrainId(1)),
                    class: ThreatClass::Siege,
                    mean_interval_ticks: 5 * TICKS_PER_DAY / 2,
                    count: (3, 6),
                    strength_per_attacker: 1.0,
                    start: day(2.0),
                },
            ],
        )));
    }
    w.declare();
    w
}

/// Runs the iron line to its end and lets the final loads settle.
pub fn run_iron_line(w: &mut FakeWorld, cfg: &IronLine) {
    let end = Tick(cfg.days * TICKS_PER_DAY);
    let mut repaired = false;
    while w.t < end + 2_000 {
        w.tick();
        // The player repairs the coast when there (an edit: it goes stale and relearns).
        if !repaired && w.t >= day(8.6) && w.t < end && w.districts[ORE_COAST].loaded {
            w.repair_all(ORE_COAST);
            repaired = true;
        }
    }
}
