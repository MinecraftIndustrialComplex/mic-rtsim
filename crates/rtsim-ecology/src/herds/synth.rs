// SPDX-License-Identifier: AGPL-3.0-or-later
//! A synthetic valley, its herds and a fake herd plugin, for the tests, `rtsim-cli sim herds` and
//! the bench. **Scaffolding**, like the other synthetic worlds: delete it once real worlds and the
//! vanilla adapter exist. Every number here is a scenario choice, not a game fact.
//!
//! **The valley** ([`Valley`]): north is −z and colder (0.5 °C a cell, ±10 °C over the year);
//! forage grows and stands where it is wet, which is the north from spring into summer and the
//! south from autumn into winter, so herds go north in spring and south in autumn. An east-west ridge two cells deep crosses the middle
//! with one pass a cell wide; going round its ends is a long detour. Outside the map is
//! impassable.
//!
//! **The fake plugin** ([`FakeHerds`]) answers commands through the boundary codec as a vanilla
//! adapter would: groups materialize whole (C25), steer, and are captured on despawn; a script can
//! kill, shear and lead animals; fenced cells stop a group that walks into them, and the plugin
//! reports the obstacle.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{StreamKey, SystemId};

use super::boundary::{
    self, Capture, Cmd, HerdDict, KillerKind, MEMBER_SHEARED, MemberSpawn, Obs, TakenHow, Uuid,
};
use super::climate::{Climate, ForageRates};
use super::engine::{Herds, NewHerd};
use super::forage::{CELL_BLOCKS, cell_of};
use super::herd::herd_of;
use super::species::HerdSpecies;

/// The valley climate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Valley {
    /// Half the map's width and depth, cells.
    pub half_x: i32,
    pub half_z: i32,
    /// The ridge's rows (cell z, inclusive) and half its length (cell x).
    pub ridge: (i32, i32),
    pub ridge_half: i32,
    /// The pass: the ridge's one open cell column.
    pub pass: i32,
    pub seed: u64,
}

impl Default for Valley {
    fn default() -> Self {
        Valley {
            half_x: 48,
            half_z: 64,
            ridge: (-4, -3),
            ridge_half: 30,
            pass: 0,
            seed: 7,
        }
    }
}

impl Valley {
    fn noise(&self, cx: i32, cz: i32) -> f64 {
        let x = StreamKey::new(self.seed, SystemId::named("synth.valley"), 0)
            .block((cx as u32 as u64) << 32 | cz as u32 as u64, 0)[0];
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Whether a cell is in the ridge (closed).
    pub fn is_ridge(&self, cx: i32, cz: i32) -> bool {
        cz >= self.ridge.0 && cz <= self.ridge.1 && cx.abs() <= self.ridge_half && cx != self.pass
    }

    /// Warmth: 1 at the height of summer (between mid and late summer), −1 at winter's.
    fn season(sub: u32) -> f64 {
        (std::f64::consts::TAU * (f64::from(sub) - 4.5) / 12.0).cos()
    }

    /// Wetness of the north against the south: the north's rains peak in late spring, the
    /// south's in late autumn, two sub-seasons ahead of the warmth, so herds that follow the
    /// green-up are north for summer and south for winter.
    fn rains(sub: u32) -> f64 {
        (std::f64::consts::TAU * (f64::from(sub) - 2.5) / 12.0).cos()
    }
}

impl Climate for Valley {
    fn forage(&self, _: u16, cx: i32, cz: i32, sub: u32) -> ForageRates {
        let s = Self::rains(sub);
        let north = (-f64::from(cz) / 40.0).clamp(-1.0, 1.0);
        let wet = (s * north + 1.0) / 2.0;
        let temp = f64::from(self.temperature(0, cx, cz, sub));
        // Standing forage follows the wet season: a dry or frozen cell holds little.
        let cap = 4_000.0 * (0.75 + 0.5 * self.noise(cx, cz)) * (0.1 + 0.9 * wet * wet);
        if temp < 2.0 {
            return ForageRates {
                growth_per_day: 0.0,
                capacity: cap,
                decay_per_day: 0.03,
            };
        }
        ForageRates {
            growth_per_day: 0.03 + 0.12 * wet,
            capacity: cap,
            decay_per_day: if wet < 0.3 { 0.01 } else { 0.0 },
        }
    }

    fn temperature(&self, _: u16, _: i32, cz: i32, sub: u32) -> f32 {
        (8.0 + 0.5 * f64::from(cz) + 10.0 * Self::season(sub)) as f32
    }

    fn terrain(&self, _: u16, cx: i32, cz: i32) -> f32 {
        if cx.abs() > self.half_x || cz.abs() > self.half_z || self.is_ridge(cx, cz) {
            return f32::INFINITY;
        }
        // Rough ground beside the ridge and at the map's edge.
        if (cz - self.ridge.0).abs() <= 2 && cx.abs() <= self.ridge_half + 2 {
            return 1.5;
        }
        1.0
    }
}

/// The scenario's herds: about 33,000 animals in six herds, in their spring pastures.
pub fn scenario_herds() -> Vec<NewHerd> {
    let h = |name: &str, species, x, z, count| NewHerd {
        name: name.to_string(),
        species,
        frame: 0,
        pos: (x, z),
        count,
        seed: None,
    };
    vec![
        h("Northdown flock", HerdSpecies::Sheep, 64, 1_400, 12_000),
        h("Greyfell flock", HerdSpecies::Sheep, -2_400, 1_800, 8_000),
        h("Riverbend herd", HerdSpecies::Cow, 2_200, 2_000, 6_000),
        h("Windmane herd", HerdSpecies::Horse, -1_000, 3_200, 2_500),
        h("Crag goats", HerdSpecies::Goat, 1_500, -1_200, 3_000),
        h("Oakroot sounder", HerdSpecies::Pig, -3_400, 600, 1_500),
    ]
}

/// The valley with the scenario's herds, from tick `at`.
pub fn valley(seed: u64, tun: super::HerdTunables, at: Tick) -> Herds {
    let mut h = Herds::new(seed, tun, Box::new(Valley::default()), at);
    for n in scenario_herds() {
        h.add_herd(n, at);
    }
    h
}

/// `n` herds of `members` each spread over the valley, for the bench.
pub fn bench_herds(seed: u64, n: usize, members: u64, at: Tick) -> Herds {
    let mut h = Herds::new(
        seed,
        super::HerdTunables::default(),
        Box::new(Valley::default()),
        at,
    );
    let key = StreamKey::new(seed, SystemId::named("synth.bench_herds"), 0);
    for i in 0..n {
        let mut r = key.slot(i as u64);
        let v = Valley::default();
        let x = (r.below((2 * v.half_x - 4) as u64) as i32 - v.half_x + 2) * CELL_BLOCKS;
        let z = (r.below((2 * v.half_z - 4) as u64) as i32 - v.half_z + 2) * CELL_BLOCKS;
        let z = if v.is_ridge(x / CELL_BLOCKS, z / CELL_BLOCKS) {
            z + 3 * CELL_BLOCKS
        } else {
            z
        };
        h.add_herd(
            NewHerd {
                name: format!("bench herd {i}"),
                species: HerdSpecies::ALL[i % HerdSpecies::COUNT],
                frame: 0,
                pos: (x, z),
                count: members,
                seed: None,
            },
            at,
        );
    }
    h
}

// ---- the fake plugin ------------------------------------------------------------------------

/// A body the fake plugin holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub spawn: MemberSpawn,
    pub sheared: bool,
}

/// A group in the fake level.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub agent: u64,
    pub epoch: u32,
    pub species: HerdSpecies,
    pub bodies: Vec<Body>,
    pub target: (i32, i32),
    pub since: Tick,
    /// An obstacle was reported against this target already.
    pub reported: Option<(i32, i32)>,
}

/// What a vanilla adapter would do, through the codec.
#[derive(Clone)]
pub struct FakeHerds {
    pub dict: HerdDict,
    pub groups: BTreeMap<u64, Group>,
    /// Fenced cells: a group walking into one is stopped there.
    pub fences: BTreeSet<(i32, i32)>,
    /// Every group ever materialized: (agent, epoch, members, colours) for the identity checks.
    pub history: Vec<(u64, u32, Vec<(u64, u32)>)>,
    pub obs: Vec<Obs>,
    next_op: u64,
}

impl Default for FakeHerds {
    fn default() -> Self {
        FakeHerds {
            dict: HerdDict::default(),
            groups: BTreeMap::new(),
            fences: BTreeSet::new(),
            history: Vec::new(),
            obs: Vec::new(),
            next_op: 0,
        }
    }
}

impl FakeHerds {
    fn op(&mut self) -> u64 {
        self.next_op += 1;
        (1 << 63) | self.next_op
    }

    /// Applies commands (round-tripped through the codec) and queues the answers.
    pub fn apply(&mut self, cmds: Vec<Cmd>, at: Tick) {
        for c in cmds {
            let mut buf = Vec::new();
            boundary::encode_cmd(1, &c, &self.dict, &mut buf);
            let (_, _, c) = boundary::decode_cmd(&buf, &self.dict).expect("codec");
            match c {
                Cmd::Materialize(m) => {
                    self.history.push((
                        m.agent,
                        m.epoch,
                        m.members.iter().map(|b| (b.member, b.variant)).collect(),
                    ));
                    self.obs.push(Obs::Materialized(boundary::Materialized {
                        agent: m.agent,
                        epoch: m.epoch,
                        dimension: m.dimension,
                        spawned: m.members.len() as u32,
                    }));
                    self.groups.insert(
                        m.agent,
                        Group {
                            agent: m.agent,
                            epoch: m.epoch,
                            species: m.species,
                            bodies: m
                                .members
                                .iter()
                                .map(|s| Body {
                                    spawn: *s,
                                    sheared: s.flags & MEMBER_SHEARED != 0,
                                })
                                .collect(),
                            target: m.target,
                            since: at,
                            reported: None,
                        },
                    );
                }
                Cmd::Steer(s) => {
                    if let Some(g) = self.groups.get_mut(&s.agent).filter(|g| g.epoch == s.epoch) {
                        g.target = s.target;
                    }
                }
                Cmd::Despawn(d) => {
                    if let Some(g) = self.groups.get(&d.agent).filter(|g| g.epoch == d.epoch) {
                        let g = g.clone();
                        self.groups.remove(&d.agent);
                        self.obs.push(Obs::Dematerialized(boundary::Dematerialized {
                            agent: g.agent,
                            epoch: g.epoch,
                            reason: 1,
                            members: g
                                .bodies
                                .iter()
                                .map(|b| Capture {
                                    member: b.spawn.member,
                                    flags: if b.sheared { MEMBER_SHEARED } else { 0 },
                                    health: 8.0,
                                    x: b.spawn.x,
                                    z: b.spawn.z,
                                })
                                .collect(),
                        }));
                    }
                }
            }
        }
    }

    /// The observations queued since the last call, each round-tripped through the codec.
    pub fn take_obs(&mut self) -> Vec<Obs> {
        std::mem::take(&mut self.obs)
            .into_iter()
            .map(|o| {
                let mut buf = Vec::new();
                boundary::encode_obs(1, 0, &o, &mut buf);
                boundary::decode_obs(&buf).expect("codec").3
            })
            .collect()
    }

    /// The player kills `n` animals of the group of `agent` (the first `n` bodies).
    pub fn kill(&mut self, agent: u64, n: usize, player: Uuid) -> Vec<u64> {
        let mut killed = Vec::new();
        for _ in 0..n {
            let op = self.op();
            let Some(g) = self.groups.get_mut(&agent) else {
                break;
            };
            if g.bodies.is_empty() {
                break;
            }
            let b = g.bodies.remove(0);
            killed.push(b.spawn.member);
            let epoch = g.epoch;
            self.obs.push(Obs::Killed(boundary::Killed {
                agent,
                epoch,
                member: b.spawn.member,
                by: KillerKind::Player,
                killer: player,
                op,
            }));
        }
        killed
    }

    /// The player shears `n` unsheared adult sheep of the group.
    pub fn shear(&mut self, agent: u64, n: usize, player: Uuid) -> Vec<u64> {
        let mut out = Vec::new();
        let Some(g) = self.groups.get_mut(&agent) else {
            return out;
        };
        for b in g.bodies.iter_mut() {
            if out.len() >= n {
                break;
            }
            if !b.sheared && b.spawn.vanilla_age >= 0 {
                b.sheared = true;
                out.push(b.spawn.member);
                self.obs.push(Obs::Sheared(boundary::Sheared {
                    agent,
                    epoch: g.epoch,
                    member: b.spawn.member,
                    player,
                }));
            }
        }
        out
    }

    /// The player puts a lead on the group's last animal: it leaves the sim (A10).
    pub fn lead(&mut self, agent: u64, player: Uuid) -> Option<u64> {
        let op = self.op();
        let g = self.groups.get_mut(&agent)?;
        let b = g.bodies.pop()?;
        self.obs.push(Obs::Taken(boundary::Taken {
            agent,
            epoch: g.epoch,
            member: b.spawn.member,
            how: TakenHow::Lead,
            player,
            op,
        }));
        Some(b.spawn.member)
    }

    /// Walks every group with its herd: a group about to enter a fenced cell is stopped there,
    /// and the plugin reports the obstacle once per target.
    pub fn walk(&mut self, herds: &Herds, at: Tick) {
        let agents: Vec<u64> = self.groups.keys().copied().collect();
        for agent in agents {
            let Some(h) = herd_of(agent).filter(|h| herds.herd(*h).is_some()) else {
                continue;
            };
            let pos = herds.position(h, at);
            let here = cell_of(pos.0, pos.1);
            let g = self.groups.get_mut(&agent).expect("listed");
            let t = g.target;
            let d = (((i64::from(t.0) - pos.0) as f64).powi(2)
                + ((i64::from(t.1) - pos.1) as f64).powi(2))
            .sqrt();
            if d < 1.0 {
                continue;
            }
            // The cell the group is about to enter, a cell ahead along its heading.
            let s = f64::from(CELL_BLOCKS) / d;
            let ahead = cell_of(
                pos.0 + ((i64::from(t.0) - pos.0) as f64 * s) as i64,
                pos.1 + ((i64::from(t.1) - pos.1) as f64 * s) as i64,
            );
            let next = if ahead == here {
                cell_of(i64::from(t.0), i64::from(t.1))
            } else {
                ahead
            };
            if self.fences.contains(&next) && g.reported != Some(t) {
                g.reported = Some(t);
                self.obs.push(Obs::Obstacle(boundary::Obstacle {
                    agent,
                    epoch: g.epoch,
                    failures: g.bodies.len() as u32,
                    present: g.bodies.len() as u32,
                    stuck: (pos.0 as i32, pos.1 as i32),
                    target: t,
                }));
            }
        }
    }

    pub fn group_of(&self, agent: u64) -> Option<&Group> {
        self.groups.get(&agent)
    }
}
