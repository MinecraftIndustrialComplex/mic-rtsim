// SPDX-License-Identifier: AGPL-3.0-or-later
//! A synthetic coastal world and a fake ships plugin, for tests, the CLI and the bench.
//! Scaffolding, as the other crates' `synth` modules are: delete it once real worlds load.
//!
//! **The coast** ([`coast`]): land north of a wavy shore, sea south of it, and an island. Six
//! fishing villages along the shore have sea ports; an island village has one too; a mountain
//! village and an inland market town have sky docks, as do two of the coastal villages. A pirate
//! haven on the island's far side keeps sea and sky pirates, and a navy patrols out of the biggest
//! port. Goods and recipes are the phase 1 world's synthetic ones. Every number is a PLACEHOLDER
//! chosen to make a small world trade by sea and air.
//!
//! **The fake plugin** ([`FakeShips`]) answers the engine's commands through the boundary codec, as
//! a plugin would: it materializes bodies, moves them along the waypoints and times it was given,
//! lets them go when they leave every player's view, and plays fights: pirates beside a merchant
//! fight it, and the stronger crew wins, the winner's crew losing a member.
//!
//! **The sea of havens** ([`sea_havens`]): ports, havens and navies scattered over an open sea by
//! the seed, for the havens' lifecycle (EQUILIBRIUM.md §2.4.6a).
//!
//! **The scripted player** ([`Script`]) of `rtsim-cli sim ships` and the paged tests: it watches a
//! sea lane, follows the hunted lane, sends an escort, logs off before a raid, loots a hold and
//! sinks a pirate.

use std::collections::BTreeMap;

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_trade::Amount;
use rtsim_trade::MILLI;
use rtsim_trade::labour::ResourceId;
use rtsim_travel::Medium;
use rtsim_world::synth as ws;
use rtsim_world::{NodeData, World, WorldConfig};

use crate::boundary::{self, By, Cmd, Item, Obs, Role, Uuid};
use crate::engine::Ships;
use crate::tunables::ShipTunables;
use crate::water::{AllWater, SynthCoast};

/// One place of the synthetic coast.
#[derive(Clone, Debug)]
pub struct Site {
    pub name: &'static str,
    pub key: u64,
    pub x: i32,
    pub z: i32,
    pub haven: bool,
    pub pop: u32,
    pub resources: &'static [(ResourceId, i64)],
    pub sea: bool,
    pub sky: bool,
}

pub fn coast() -> SynthCoast {
    SynthCoast {
        cell: 32,
        shore_z: 0,
        amp: 300,
        wavelength: 900,
        islands: vec![(1_800, 2_300, 520)],
        bays: vec![],
    }
}

/// The shore's z at x (land is north of it, at smaller z).
pub fn shore(x: i32) -> i32 {
    (300.0 * (f64::from(x) / 900.0).sin()).round() as i32
}

pub fn sites() -> Vec<Site> {
    use ws::{CATTLE, EMERALD_ORE, FARMLAND, GEODE, IRON_ORE, SHEEP, SWAMP};
    let on_shore = |x: i32| shore(x) - 60;
    vec![
        Site {
            name: "Saltmere",
            key: 101,
            x: 0,
            z: on_shore(0),
            haven: false,
            pop: 24,
            resources: &[(FARMLAND, 120), (SHEEP, 6)],
            sea: true,
            sky: true,
        },
        Site {
            name: "Brightwater",
            key: 102,
            x: 1_300,
            z: on_shore(1_300),
            haven: false,
            pop: 16,
            resources: &[(CATTLE, 8), (IRON_ORE, 4)],
            sea: true,
            sky: false,
        },
        Site {
            name: "Gullhaven",
            key: 103,
            x: 2_700,
            z: on_shore(2_700),
            haven: false,
            pop: 14,
            resources: &[(EMERALD_ORE, 2), (SHEEP, 8)],
            sea: true,
            sky: true,
        },
        Site {
            name: "Cobble Cove",
            key: 104,
            x: -1_300,
            z: on_shore(-1_300),
            haven: false,
            pop: 15,
            resources: &[(FARMLAND, 60), (CATTLE, 4)],
            sea: true,
            sky: false,
        },
        Site {
            name: "Westreach",
            key: 105,
            x: -2_600,
            z: on_shore(-2_600),
            haven: false,
            pop: 12,
            resources: &[(IRON_ORE, 3), (SWAMP, 3)],
            sea: true,
            sky: false,
        },
        Site {
            name: "Farpoint",
            key: 106,
            x: 4_000,
            z: on_shore(4_000),
            haven: false,
            pop: 10,
            resources: &[(GEODE, 4), (FARMLAND, 40)],
            sea: true,
            sky: false,
        },
        Site {
            name: "Isla Verde",
            key: 107,
            x: 1_750,
            z: 2_300 - 400,
            haven: false,
            pop: 12,
            resources: &[(FARMLAND, 50), (SHEEP, 5)],
            sea: true,
            sky: true,
        },
        Site {
            name: "Highcrag",
            key: 108,
            x: 1_200,
            z: -1_900,
            haven: false,
            pop: 14,
            resources: &[(IRON_ORE, 6), (EMERALD_ORE, 2)],
            sea: false,
            sky: true,
        },
        Site {
            name: "Millbrook",
            key: 109,
            x: -1_400,
            z: -1_500,
            haven: false,
            pop: 18,
            resources: &[(FARMLAND, 100), (CATTLE, 6)],
            sea: false,
            sky: true,
        },
        Site {
            name: "Blackwater",
            key: 110,
            x: 1_850,
            z: 2_300 + 420,
            haven: true,
            pop: 6,
            resources: &[],
            sea: false,
            sky: false,
        },
    ]
}

/// The coastal world's names by node index.
pub fn names() -> Vec<&'static str> {
    sites().iter().map(|s| s.name).collect()
}

/// Ports are bigger than phase 1's inland villages, so the sea carries visible trade.
/// PLACEHOLDER: three times the population and resources.
const PORT_SCALE: i64 = 3;

fn node_data(s: &Site) -> NodeData {
    NodeData {
        key: s.key,
        kind: if s.haven {
            NodeKind::Haven(Culture::Human)
        } else {
            NodeKind::Village(Culture::Human)
        },
        pos: Pos::new(0, s.x, s.z),
        pop: s.pop * PORT_SCALE as u32,
        resources: s
            .resources
            .iter()
            .map(|&(r, d)| (r, d * MILLI * PORT_SCALE))
            .collect(),
    }
}

/// Steps the world, the ships and the fake plugin to `to` in steps of `dt` ticks: commands go to
/// the plugin through the codec and its reports come back at the step's end.
pub fn drive(
    w: &mut World,
    ships: &mut Ships,
    fake: &mut FakeShips,
    infra: &mut dyn rtsim_ecology::infra::Infrastructure,
    mask: &dyn crate::water::WaterMask,
    to: Tick,
    dt: u64,
) {
    let mut t = ships.now().max(w.now());
    while t < to {
        t = (t + dt.max(1)).min(to);
        ships.run_until(w, infra, t);
        let cmds = ships.take_commands();
        let mut obs = fake.apply(&cmds, t);
        obs.extend(fake.step(t));
        for o in obs {
            ships.ingest(w, &o, mask, t);
        }
    }
}

/// What [`coastal`] built, by name.
pub struct Coastal {
    pub world: World,
    pub ships: Ships,
    pub mask: SynthCoast,
    pub haven: u32,
    pub navy: Option<u32>,
}

/// The world's configuration for the coast (checks off: the scenario checks at its own pace).
pub fn world_config(seed: u64) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    cfg
}

/// Builds the coastal world: villages, ports and docks, the network, a haven with two sea
/// pirate ships and one sky pirate ship, and a navy out of Saltmere.
pub fn coastal(seed: u64, tun: ShipTunables) -> Coastal {
    let mask = coast();
    let (world, ships, haven, navy) = coastal_on(seed, tun, &mask);
    Coastal {
        world,
        ships,
        mask,
        haven,
        navy,
    }
}

/// [`coastal`]'s world on any water mask (the world model's, in its tests): the world, the
/// ships, the haven and the navy.
pub fn coastal_on(
    seed: u64,
    tun: ShipTunables,
    mask: &dyn crate::water::WaterMask,
) -> (World, Ships, u32, Option<u32>) {
    let (goods, ids) = ws::goods();
    let book = ws::recipes(&ids);
    let ss = sites();
    let data: Vec<NodeData> = ss.iter().map(node_data).collect();
    let mut w = World::build(world_config(seed), goods, book, data);
    let mut ships = Ships::new(seed, tun, Tick::ZERO);
    for (i, s) in ss.iter().enumerate() {
        if s.sea {
            ships.add_port(
                &mut w,
                i,
                Medium::Sea,
                (s.x, 63, shore(s.x) + 40),
                4,
                mask,
                Tick::ZERO,
            );
        }
        if s.sky {
            ships.add_port(
                &mut w,
                i,
                Medium::Sky,
                (s.x + 30, 120, s.z),
                4,
                mask,
                Tick::ZERO,
            );
        }
    }
    let hi = ss.iter().position(|s| s.haven).expect("a haven");
    let hs = &ss[hi];
    let haven = ships
        .add_haven(&mut w, hi, 3, (hs.x, 63, hs.z + 60), 1, Tick::ZERO)
        .expect("a haven node");
    ships.build(&mut w, mask, Tick::ZERO);
    let navy = ships.add_navy(&mut w, 0, Medium::Sea, Tick::ZERO);
    (w, ships, haven, navy)
}

/// A bench world: `side × side` ports in open sea on a 1,200-block grid with sea docks, no
/// observers, one haven in the middle with one sea pirate. Scaffolding for the ship-count sweep.
pub fn ocean_grid(side: usize, seed: u64, tun: ShipTunables) -> (World, Ships, AllWater) {
    let (goods, ids) = ws::goods();
    let book = ws::recipes(&ids);
    let all = [
        ws::FARMLAND,
        ws::CATTLE,
        ws::IRON_ORE,
        ws::SHEEP,
        ws::EMERALD_ORE,
        ws::SWAMP,
    ];
    let mut data = Vec::new();
    for i in 0..side * side {
        let (gx, gz) = ((i % side) as i32, (i / side) as i32);
        let r = all[i % all.len()];
        let r2 = all[(i * 7 + 3) % all.len()];
        data.push(NodeData {
            key: 1_000 + i as u64,
            kind: NodeKind::Village(Culture::Human),
            pos: Pos::new(
                0,
                gx * 1_200 + (i as i32 * 37) % 200,
                gz * 1_200 + (i as i32 * 53) % 200,
            ),
            pop: 12,
            resources: [(r, 40 * MILLI), (r2, 4 * MILLI)].into_iter().collect(),
        });
    }
    let mid = (side as i32 / 2) * 1_200 + 600;
    data.push(NodeData {
        key: 9_999,
        kind: NodeKind::Haven(Culture::Human),
        pos: Pos::new(0, mid, mid),
        pop: 4,
        resources: BTreeMap::new(),
    });
    let n = data.len();
    let mut w = World::build(world_config(seed), goods, book, data.clone());
    let mask = AllWater(32);
    let mut ships = Ships::new(seed, tun, Tick::ZERO);
    for (i, d) in data.iter().enumerate().take(n - 1) {
        ships.add_port(
            &mut w,
            i,
            Medium::Sea,
            (d.pos.x, 63, d.pos.z),
            4,
            &mask,
            Tick::ZERO,
        );
    }
    ships.add_haven(&mut w, n - 1, 1, (mid, 63, mid), 1, Tick::ZERO);
    ships.build(&mut w, &mask, Tick::ZERO);
    (w, ships, mask)
}

/// What [`sea_havens`] built.
pub struct SeaHavens {
    pub world: World,
    pub ships: Ships,
    pub mask: AllWater,
    /// The havens in founding order: the first sits off a navy's base, as does the second when
    /// there are two navies.
    pub havens: Vec<u32>,
    /// The navies (a base whose patrol found no lane is left out).
    pub navies: Vec<u32>,
    /// The ports' positions: port `i` is world node `i` (the havens' nodes follow them), and the
    /// navies are based at the first.
    pub ports: Vec<(i32, i32)>,
}

const SYS_SEA_HAVENS: SystemId = SystemId::named("synth.sea_havens");

/// The open sea's half side, blocks: [`sea_havens`] places everything in `[-6000, 6000]²`.
pub const SEA_HALF: i32 = 6_000;

/// A sea of several havens for the lifecycle (EQUILIBRIUM.md §2.4.6a, slice S8b), laid out by the
/// seed: 12 to 20 port villages scattered over a 12,000-block open sea ([`AllWater`]) at least
/// 1,400 blocks apart, every third with a sky dock as well; 1 or 2 navies based at the first
/// ports; and 3 to 5 havens, the first (and the second, with two navies) 700 blocks off a navy's
/// base, the rest at least 1,000 blocks from any port and 2,500 from another haven, and 3,000
/// from a navy's base when they can be. A haven keeps a sea pirate, and one in three a sky pirate
/// too. The same seed builds the same world. Scaffolding, as [`ocean_grid`]; every number is a
/// PLACEHOLDER chosen to give some havens patrolled reaches and others free ones.
pub fn sea_havens(seed: u64, tun: ShipTunables) -> SeaHavens {
    let mut rng = StreamKey::new(seed, SYS_SEA_HAVENS, 0).slot(0);
    let mut pick = |n: u64| rng.below(n);
    let ports_n = 12 + pick(9) as usize;
    let havens_n = 3 + pick(3) as usize;
    let navies_n = 1 + pick(2) as usize;
    let span = 2 * SEA_HALF as u64 + 1;
    let far = |a: (i32, i32), b: (i32, i32), d: i64| {
        let (dx, dz) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
        dx * dx + dz * dz >= d * d
    };
    // Ports: rejection sampled, the spacing relaxed if the sea fills up.
    let mut ports: Vec<(i32, i32)> = Vec::new();
    let mut spacing = 1_400;
    let mut tries = 0;
    while ports.len() < ports_n {
        let p = (pick(span) as i32 - SEA_HALF, pick(span) as i32 - SEA_HALF);
        tries += 1;
        if tries % 2_000 == 0 {
            spacing = spacing * 9 / 10;
        }
        if ports.iter().all(|&q| far(p, q, spacing)) {
            ports.push(p);
        }
    }
    let bases: Vec<(i32, i32)> = ports[..navies_n].to_vec();
    let mut havens: Vec<(i32, i32)> = Vec::new();
    for i in 0..havens_n {
        let mut tries = 0u32;
        let p = loop {
            tries += 1;
            if i < navies_n {
                // Off a navy's base, 700 blocks out at a seeded bearing (and not on a port).
                let a = pick(3_600) as f64 / 3_600.0 * std::f64::consts::TAU;
                let b = bases[i];
                let p = (
                    (f64::from(b.0) + 700.0 * a.cos()).round() as i32,
                    (f64::from(b.1) + 700.0 * a.sin()).round() as i32,
                );
                let p = (
                    p.0.clamp(-SEA_HALF, SEA_HALF),
                    p.1.clamp(-SEA_HALF, SEA_HALF),
                );
                if ports.iter().all(|&q| far(p, q, 400)) || tries > 200 {
                    break p;
                }
                continue;
            }
            let p = (pick(span) as i32 - SEA_HALF, pick(span) as i32 - SEA_HALF);
            let strict = tries < 2_000;
            let ok = ports.iter().all(|&q| far(p, q, 1_000))
                && havens
                    .iter()
                    .all(|&q| far(p, q, if strict { 2_500 } else { 1_200 }))
                && (!strict || bases.iter().all(|&q| far(p, q, 3_000)));
            if ok || tries > 20_000 {
                break p;
            }
        };
        havens.push(p);
    }
    let (goods, ids) = ws::goods();
    let book = ws::recipes(&ids);
    let all = [
        ws::FARMLAND,
        ws::CATTLE,
        ws::IRON_ORE,
        ws::SHEEP,
        ws::EMERALD_ORE,
        ws::SWAMP,
    ];
    let mut data = Vec::new();
    for (i, &(x, z)) in ports.iter().enumerate() {
        let r = all[pick(all.len() as u64) as usize];
        let r2 = all[(i * 7 + 3) % all.len()];
        data.push(NodeData {
            key: 2_000 + i as u64,
            kind: NodeKind::Village(Culture::Human),
            pos: Pos::new(0, x, z),
            pop: 12,
            resources: [(r, 40 * MILLI), (r2, 4 * MILLI)].into_iter().collect(),
        });
    }
    for (i, &(x, z)) in havens.iter().enumerate() {
        data.push(NodeData {
            key: 8_000 + i as u64,
            kind: NodeKind::Haven(Culture::Human),
            pos: Pos::new(0, x, z),
            pop: 4,
            resources: BTreeMap::new(),
        });
    }
    let media: Vec<u32> = (0..havens_n)
        .map(|_| if pick(3) == 0 { 3 } else { 1 })
        .collect();
    let mut w = World::build(world_config(seed), goods, book, data);
    let mask = AllWater(32);
    let mut ships = Ships::new(seed, tun, Tick::ZERO);
    for (i, &(x, z)) in ports.iter().enumerate() {
        ships.add_port(&mut w, i, Medium::Sea, (x, 63, z), 4, &mask, Tick::ZERO);
        if i % 3 == 2 {
            ships.add_port(
                &mut w,
                i,
                Medium::Sky,
                (x + 30, 120, z),
                4,
                &mask,
                Tick::ZERO,
            );
        }
    }
    let mut hs = Vec::new();
    for (i, &(x, z)) in havens.iter().enumerate() {
        let node = ports_n + i;
        if let Some(h) = ships.add_haven(&mut w, node, media[i], (x, 63, z), 1, Tick::ZERO) {
            hs.push(h);
        }
    }
    ships.build(&mut w, &mask, Tick::ZERO);
    let navies = (0..navies_n)
        .filter_map(|i| ships.add_navy(&mut w, i, Medium::Sea, Tick::ZERO))
        .collect();
    SeaHavens {
        world: w,
        ships,
        mask,
        havens: hs,
        navies,
        ports,
    }
}

// ---- the fake plugin ----------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Body {
    pub epoch: u32,
    pub role: Role,
    pub medium: Medium,
    pub crew: u32,
    pub hull_milli: u32,
    pub waypoints: Vec<(i32, i32)>,
    pub times: [i64; 4],
    pub harbour: u64,
    pub at_spawn: (f64, f64, f64),
    pub spawned: Tick,
    pub target: u64,
    pub cargo: Vec<Item>,
    /// Leaving: despawn after this tick.
    pub departing: Option<Tick>,
    pub fought: bool,
}

impl Body {
    /// Where the fake has it at `t`: along its waypoints by the voyage's phases, as the engine's
    /// timetable has it (a real plugin steers its body there).
    pub fn pos(&self, t: Tick) -> (f64, f64) {
        if self.waypoints.len() < 2 || self.role == Role::Pirate {
            return (self.at_spawn.0, self.at_spawn.2);
        }
        let p = crate::geom::Polyline::new(self.waypoints.clone());
        let len = p.length();
        let h = self.harbour.min(len / 3);
        let [dep, cru, app, arr] = self.times;
        let tt = t.0 as i64;
        let frac = |a: i64, b: i64| {
            if b <= a {
                1.0
            } else {
                ((tt - a).clamp(0, b - a)) as f64 / (b - a) as f64
            }
        };
        let s = if tt < dep {
            0
        } else if tt < cru {
            (h as f64 * frac(dep, cru)).round() as u64
        } else if tt < app {
            h + ((len - 2 * h) as f64 * frac(cru, app)).round() as u64
        } else {
            len - h + (h as f64 * frac(app, arr)).round() as u64
        };
        let (x, z) = p.at(s);
        (f64::from(x), f64::from(z))
    }
}

/// What the fake does with what it is told, and what it saw, for the CLI.
#[derive(Clone, Default)]
pub struct FakeShips {
    pub bodies: BTreeMap<u64, Body>,
    pub players: BTreeMap<Uuid, (f64, f64)>,
    pub view: f64,
    pub seq: u64,
    pub log: Vec<String>,
    pub materialized: u64,
    pub dematerialized: u64,
    pub fights: u64,
    /// Commands seen, by kind.
    pub commands: BTreeMap<u16, u64>,
}

impl FakeShips {
    pub fn new(view: f64) -> FakeShips {
        FakeShips {
            view,
            ..FakeShips::default()
        }
    }

    pub fn set_player(&mut self, uuid: Uuid, x: f64, z: f64) {
        self.players.insert(uuid, (x, z));
    }

    fn near_player(&self, p: (f64, f64)) -> bool {
        self.players.values().any(|q| {
            let (dx, dz) = (p.0 - q.0, p.1 - q.1);
            (dx * dx + dz * dz).sqrt() <= self.view + 32.0
        })
    }

    /// Applies the engine's commands (each through the codec, as bytes would travel) and returns
    /// what the plugin reports back at `at`.
    pub fn apply(&mut self, cmds: &[Cmd], at: Tick) -> Vec<Obs> {
        let mut out = Vec::new();
        for c in cmds {
            let mut b = Vec::new();
            self.seq += 1;
            boundary::encode_cmd(self.seq, c, &mut b);
            let (_, _, c) = boundary::decode_cmd(&b).expect("the engine writes valid records");
            *self.commands.entry(c.kind()).or_default() += 1;
            match c {
                Cmd::Materialize(m) => {
                    self.materialized += 1;
                    self.bodies.insert(
                        m.agent,
                        Body {
                            epoch: m.epoch,
                            role: m.role,
                            medium: m.medium,
                            crew: m.crew,
                            hull_milli: m.hull_milli,
                            waypoints: m.waypoints.clone(),
                            times: m.times,
                            harbour: u64::from(m.harbour_blocks),
                            at_spawn: m.pos,
                            spawned: at,
                            target: m.target,
                            cargo: m.cargo.clone(),
                            departing: None,
                            fought: false,
                        },
                    );
                    out.push(Obs::Materialized(boundary::Materialized {
                        agent: m.agent,
                        epoch: m.epoch,
                        dimension: m.dimension,
                        entity: (m.agent, u64::from(m.epoch)),
                    }));
                }
                Cmd::Timetable(t) => {
                    if let Some(b) = self.bodies.get_mut(&t.agent)
                        && b.epoch == t.epoch
                    {
                        b.waypoints = t.waypoints.clone();
                        b.times = t.times;
                        b.harbour = u64::from(t.harbour_blocks);
                    }
                }
                Cmd::Depart(d) => {
                    if let Some(b) = self.bodies.get_mut(&d.agent)
                        && b.epoch == d.epoch
                    {
                        b.departing = Some(at + 300);
                    }
                }
                Cmd::SetCargo(s) => {
                    if let Some(b) = self.bodies.get_mut(&s.agent)
                        && b.epoch == s.epoch
                    {
                        b.cargo = s.items.clone();
                    }
                }
                Cmd::Steer(st) => {
                    if let Some(b) = self.bodies.get_mut(&st.agent)
                        && b.epoch == st.epoch
                        && st.flags & boundary::STEER_ATTACK != 0
                    {
                        b.target = st.target;
                        b.fought = false;
                        b.spawned = at;
                        b.at_spawn = st.pos;
                    }
                }
                Cmd::Despawn(d) => {
                    if self
                        .bodies
                        .get(&d.agent)
                        .is_some_and(|b| b.epoch == d.epoch)
                    {
                        self.bodies.remove(&d.agent);
                    }
                }
            }
        }
        out
    }

    /// One step at `at`: fights beside merchants, bodies that left view or finished departing.
    pub fn step(&mut self, at: Tick) -> Vec<Obs> {
        let mut out = Vec::new();
        // Fights: a hostile pirate 200 ticks after it came.
        let agents: Vec<u64> = self.bodies.keys().copied().collect();
        for a in &agents {
            let Some(p) = self.bodies.get(a).cloned() else {
                continue;
            };
            if p.role != Role::Pirate || p.target == 0 || p.fought || at < p.spawned + 200 {
                continue;
            }
            let Some(v) = self.bodies.get(&p.target).cloned() else {
                continue;
            };
            self.fights += 1;
            let (pa, va) = (f64::from(p.crew), f64::from(v.crew) * 0.5);
            let pirates_win = pa > va;
            let (p_after, v_after) = if pirates_win {
                (p.crew.saturating_sub(1).max(1), v.crew / 2)
            } else {
                (p.crew / 2, v.crew.saturating_sub(1).max(1))
            };
            out.push(Obs::Fight(boundary::Fight {
                attacker: *a,
                defender: p.target,
                outcome: if pirates_win { 1 } else { 2 },
                attacker_crew: (p.crew, p_after),
                defender_crew: (v.crew, v_after),
                attacker_hull_milli: 800,
                defender_hull_milli: if pirates_win { 500 } else { 900 },
            }));
            if pirates_win && !v.cargo.is_empty() {
                let items: Vec<Item> = v
                    .cargo
                    .iter()
                    .map(|i| Item {
                        item: i.item,
                        count: (i.count / 2).max(1),
                    })
                    .collect();
                out.push(Obs::CargoLooted(boundary::CargoLooted {
                    agent: p.target,
                    epoch: v.epoch,
                    by: By::Ship,
                    player: (0, 0),
                    ship: *a,
                    op: 1 << 63 | self.seq,
                    items,
                }));
            }
            self.log.push(format!(
                "tick {}: fight: pirates {} {} against {} crew",
                at.0,
                if pirates_win { "beat" } else { "lost to" },
                v.crew,
                p.crew
            ));
            if let Some(b) = self.bodies.get_mut(a) {
                b.fought = true;
                b.crew = p_after;
            }
            if let Some(b) = self.bodies.get_mut(&p.target) {
                b.crew = v_after;
            }
        }
        for a in agents {
            let Some(b) = self.bodies.get(&a) else {
                continue;
            };
            let pos = b.pos(at);
            let gone_departing = b.departing.is_some_and(|d| at >= d);
            let out_of_view = !self.near_player(pos);
            if gone_departing || out_of_view {
                let b = self.bodies.remove(&a).expect("present");
                self.dematerialized += 1;
                out.push(Obs::Dematerialized(boundary::Dematerialized {
                    agent: a,
                    epoch: b.epoch,
                    reason: 3,
                    dimension: 0,
                    hull_milli: b.hull_milli,
                    crew: b.crew,
                    pos: (pos.0, 63.0, pos.1),
                }));
            }
        }
        out
    }

    /// A scripted player loots part of a body's hold.
    pub fn player_loots(&mut self, agent: u64, player: Uuid, share: f64) -> Option<Obs> {
        let b = self.bodies.get_mut(&agent)?;
        let items: Vec<Item> = b
            .cargo
            .iter()
            .map(|i| Item {
                item: i.item,
                count: ((f64::from(i.count) * share).floor() as i32).max(0),
            })
            .filter(|i| i.count > 0)
            .collect();
        if items.is_empty() {
            return None;
        }
        self.seq += 1;
        Some(Obs::CargoLooted(boundary::CargoLooted {
            agent,
            epoch: b.epoch,
            by: By::Player,
            player,
            ship: 0,
            op: 1 << 63 | self.seq,
            items,
        }))
    }

    /// A scripted player sinks a body.
    pub fn player_sinks(&mut self, agent: u64, player: Uuid) -> Option<Obs> {
        let b = self.bodies.remove(&agent)?;
        self.seq += 1;
        Some(Obs::Sunk(boundary::Sunk {
            agent,
            epoch: b.epoch,
            by: By::Player,
            dimension: 0,
            player,
            ship: 0,
            op: 1 << 63 | self.seq,
            pos: b.at_spawn,
        }))
    }
}

/// Total thousandths held in a cargo map.
pub fn total(c: &BTreeMap<rtsim_trade::GoodId, Amount>) -> Amount {
    c.values().sum()
}

// ---- the scripted player ------------------------------------------------------------------------

/// The scripted player's UUID.
pub const SCRIPT_PLAYER: Uuid = (0x5EA, 0x5A11);

fn day(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

/// A straight sea lane's middle (where the scripted player first watches).
pub fn watch_point(s: &Ships) -> (i32, i32) {
    let l = (0..s.lanes.len())
        .find(|&l| {
            let ln = &s.lanes[l];
            ln.medium == Medium::Sea && ln.path.pts.len() == 2 && ln.path.length() > 1_000
        })
        .unwrap_or(0);
    let ln = &s.lanes[l];
    ln.path.at(ln.path.length() / 2)
}

/// The scripted player on the coast ([`coastal`]): it watches a straight sea lane for two days
/// (ships enter its view), then follows the lane the pirates hunt (a raid in view, loot sailing to
/// the haven); from day 5 its escort joins the hunted lane for a day; after day 7.2 it logs off
/// for a quarter day when a raid is coming (the fight is resolved unobserved); on day 7 it loots
/// half a merchant's hold, and after day 9 it sinks the first pirate in view. What it did so far
/// is its state, so a run may stop and go on (the same steps, as long as each stop is on the
/// hour), or be copied with the engine it drives.
#[derive(Clone, Debug, Default)]
pub struct Script {
    pub looted: bool,
    pub sank: bool,
    /// The escort's lane and since when (`u32::MAX` once it has left).
    pub escort: Option<(u32, Tick)>,
    /// Logged off until then.
    pub away: Option<Tick>,
    /// Conservation failures, checked every hour.
    pub conserved: Vec<String>,
}

impl Script {
    /// The scripted run from `from` to `to`, hour by hour.
    pub fn run(&mut self, c: &mut Coastal, fake: &mut FakeShips, from: Tick, to: Tick) {
        const PLAYER: Uuid = SCRIPT_PLAYER;
        let watch = watch_point(&c.ships);
        let mut t = from;
        while t < to {
            let d = day(t);
            if self.away.is_some_and(|u| t < u) {
                let next = (t + 250).min(to);
                drive(
                    &mut c.world,
                    &mut c.ships,
                    fake,
                    &mut NoInfrastructure,
                    &c.mask,
                    next,
                    50,
                );
                t = next;
                continue;
            }
            let at = if d < 2.0 {
                Some(watch)
            } else {
                c.ships
                    .hunted_lanes()
                    .first()
                    .map(|&l| {
                        let ln = &c.ships.lanes[l as usize];
                        ln.path.at(ln.path.length() / 2)
                    })
                    .or(Some(watch))
            };
            if let Some(p) = at {
                fake.set_player(PLAYER, f64::from(p.0), f64::from(p.1));
                c.ships
                    .observe_player(&mut c.world, PLAYER, Pos::new(0, p.0, p.1), t);
            }
            let next = (t + 1_000).min(to);
            while t < next {
                let step = (t + 50).min(next);
                drive(
                    &mut c.world,
                    &mut c.ships,
                    fake,
                    &mut NoInfrastructure,
                    &c.mask,
                    step,
                    50,
                );
                t = step;
                // From day 7: a raid is coming on a ship in view, and the player logs off before
                // it strikes: the fight is resolved unobserved (Lanchester, never better than the
                // fights seen).
                if day(t) >= 7.2
                    && self.away.is_none()
                    && c.ships.pending_by_kind().contains_key("raid")
                {
                    fake.players.clear();
                    let p = c.ships.player(&mut c.world, PLAYER);
                    c.ships.player_left(&mut c.world, p);
                    self.away = Some(t + TICKS_PER_DAY / 4);
                    break;
                }
            }
            if !self.looted && day(t) >= 7.0 {
                let target = fake
                    .bodies
                    .iter()
                    .find(|(_, b)| b.role == Role::Merchant && !b.cargo.is_empty())
                    .map(|(a, _)| *a);
                if let Some(a) = target
                    && let Some(o) = fake.player_loots(a, PLAYER, 0.5)
                {
                    c.ships.ingest(&mut c.world, &o, &c.mask, t);
                    self.looted = true;
                }
            }
            // Day 5: the player's escort ships (for a future plugin) join the hunted lane for a
            // day: its hazard falls while they are there.
            if day(t) >= 5.0 && self.escort.is_none() {
                let hunted = c
                    .ships
                    .hunted_lanes()
                    .into_iter()
                    .find(|&l| c.ships.threat(l).is_some_and(|th| th.share > 0));
                if let Some(l) = hunted {
                    c.ships.set_escort(&mut c.world, l, 1, 4.0, t);
                    self.escort = Some((l, t));
                }
            }
            if let Some((l, since)) = self.escort
                && t.0 >= since.0 + TICKS_PER_DAY
                && l != u32::MAX
            {
                c.ships.set_escort(&mut c.world, l, 1, 0.0, t);
                self.escort = Some((u32::MAX, since));
            }
            if !self.sank && day(t) >= 9.0 {
                let target = fake
                    .bodies
                    .iter()
                    .find(|(_, b)| b.role == Role::Pirate)
                    .map(|(a, _)| *a);
                if let Some(a) = target
                    && let Some(o) = fake.player_sinks(a, PLAYER)
                {
                    c.ships.ingest(&mut c.world, &o, &c.mask, t);
                    self.sank = true;
                }
            }
            if let Err(e) = c.ships.check(&c.world) {
                self.conserved.push(format!("day {:.2}: {e}", day(t)));
            }
        }
    }
}
