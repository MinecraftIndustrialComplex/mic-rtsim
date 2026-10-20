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

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
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
#[derive(Default)]
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
