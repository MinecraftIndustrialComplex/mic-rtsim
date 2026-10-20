// SPDX-License-Identifier: AGPL-3.0-or-later
//! A synthetic forest, a goblin camp, a player's refinery and a fake conflict plugin, for the
//! tests, `rtsim-cli sim conflict` and the bench. **Scaffolding**, as the other crates' `synth`
//! modules are: delete it once a real plugin and real worlds exist.
//!
//! - The world is `rtsim_world::synth`'s phase 1 world (six human villages, three ribbit ones, a
//!   pillager haven, a player base). The goblin camp of Gloomhollow joins it as a haven in the
//!   Thornwood, south-east of Ashby.
//! - The refinery is an industry district (`rtsim-industry`'s fake world) at the forest's edge:
//!   a vault of iron and bread, a distiller and a smelter, half-walled, no turrets.
//! - [`FakeConflict`] plays the plugin: every command and observation goes through the boundary
//!   codec; a scripted player answers envoys (ignore, pay, refuse, attack) and fights bands that
//!   materialize near it (a share killed per wave, the war chief or not).

use std::collections::BTreeMap;

use rtsim_industry::engine::Mode;
use rtsim_industry::fake::{self, FDistrict, FInventory, FakeWorld, Visit, exposed, survey};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{BlockFlags, InvKind};
use rtsim_industry::threat::ThreatClass;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::Pos;
use rtsim_world::synth::SynthGoods;
use rtsim_world::{World, WorldConfig};

use crate::boundary::{self, Cmd, Obs, Uuid};
use crate::culture::Cultures;
use crate::engine::{Conflict, body_names, item_of};
use crate::tunables::ConflictTunables;

/// The goblin camp's key and place.
pub const CAMP: u64 = 0xC0B1_0001;
pub const CAMP_POS: Pos = Pos::new(0, 900, 1000);
/// The player's refinery district and place.
pub const REFINERY: DistrictId = DistrictId(1);
pub const REFINERY_POS: Pos = Pos::new(0, 700, 780);
/// Where the player logs the Thornwood.
pub const FOREST_POS: Pos = Pos::new(0, 880, 900);
/// The scenario player.
pub const PLAYER: Uuid = (0x5EED, 0xB0B);

pub const IRON: Good = Good(1);
pub const BREAD: Good = Good(2);
pub const GASOLINE: Good = Good(3);

pub fn world_config(seed: u64) -> WorldConfig {
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    cfg
}

/// The phase 1 synthetic world (no pillager raids on its roads).
pub fn world(seed: u64) -> (World, SynthGoods) {
    rtsim_world::synth::build(world_config(seed), false)
}

/// A conflict engine on the built-in cultures, with the plugin's dictionary as the fake plugin
/// names things (items by the world's catalogue, bodies by the culture data).
pub fn conflict(seed: u64, w: &World) -> Conflict {
    let mut c = Conflict::new(seed, ConflictTunables::default(), Cultures::builtin(), Tick::ZERO);
    define_names(&mut c, w);
    c
}

/// Java's dictionary as [`FakeConflict`] uses it: item ids as the engine's, body ids as the
/// engine's, the goblin camp structure as 4:1, Destroy smog as 0x20:1.
pub fn define_names(c: &mut Conflict, w: &World) {
    for (g, d) in w.goods.iter() {
        c.define(2, item_of(g), &d.name);
    }
    for (i, n) in body_names(&c.cultures).iter().enumerate() {
        c.define(3, i as u32 + 1, n);
    }
    c.define(4, 1, "goblinstyranny:goblin_camp");
    c.define(4, 2, "mic_rtsim:conflict/bandit_camp");
    c.define(4, 3, "minecraft:pillager_outpost");
    c.define(boundary::DICT_POLLUTION, 1, "destroy:smog");
    c.define(boundary::DICT_POLLUTION, 2, "destroy:radioactivity");
}

/// The refinery: a vault, a distiller and a smelter at the Thornwood's edge, half-walled (6 gaps
/// in 40 perimeter columns), no turrets, loaded as `visits` say.
pub fn refinery(seed: u64, visits: Vec<Visit>) -> FakeWorld {
    let mut tun = Tunables::default();
    // The industry defaults carry nothing and break nothing for ground attackers (natural mobs,
    // vanilla). Raiders of a faction do both: the scenario lets a melee raider carry off 8 items
    // and break a block, an archer carry 4 (TUNABLE, PLACEHOLDER; choice K2-11).
    tun.threat.carry_per_attacker[ThreatClass::GroundMelee.index()] = 8;
    tun.threat.carry_per_attacker[ThreatClass::GroundRanged.index()] = 4;
    tun.threat.blocks_per_attacker[ThreatClass::GroundMelee.index()] = 1;
    let mut w = FakeWorld::new(seed, tun, Mode::Apply);
    w.good(IRON.0, GoodInfo::item("minecraft:iron_ingot", 64));
    w.good(BREAD.0, GoodInfo::item("minecraft:bread", 64));
    w.good(GASOLINE.0, GoodInfo::item("destroy:gasoline_bucket", 16));
    let (bx, bz) = (REFINERY_POS.x, REFINERY_POS.z);
    let vault = BlockPos::new(bx, 64, bz);
    let distiller = BlockPos::new(bx + 4, 64, bz);
    let smelter = BlockPos::new(bx + 6, 64, bz + 2);
    let mut d = FDistrict {
        id: REFINERY,
        name: "Thornwood refinery".to_string(),
        dim: 0,
        inventories: vec![FInventory {
            index: 0,
            pos: vault,
            kind: InvKind::ITEMS,
            slots: 54,
            fluid_cap: 0,
            energy_cap: 0,
            contents: GoodVec::from_pairs([(IRON, 512), (BREAD, 256), (GASOLINE, 32)]),
            destroyed: false,
        }],
        recipes: vec![
            fake::Recipe {
                name: "distiller".into(),
                inputs: vec![],
                outputs: vec![(GASOLINE, 0, 1)],
                period: 1_200,
                target: Some((GASOLINE, 0, 256, 200)),
                deposit_per_batch: 0,
                power_from: None,
                machine: distiller,
            },
            fake::Recipe {
                name: "smelter".into(),
                inputs: vec![],
                outputs: vec![(IRON, 0, 2)],
                period: 600,
                target: Some((IRON, 0, 1_600, 1_400)),
                deposit_per_batch: 0,
                power_from: None,
                machine: smelter,
            },
        ],
        deposit: None,
        deposit_chunk: ChunkPos {
            dim: 0,
            x: bx >> 4,
            z: bz >> 4,
        },
        exposed: Vec::new(),
        survey: Some(survey(REFINERY, 30, 6, 40, 0, 2, [0.0; 4])),
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
    let edge = BlockFlags::PERIMETER | BlockFlags::ROOFLESS;
    d.exposed = vec![
        exposed(distiller, 31, BlockFlags::MACHINE | BlockFlags::ROOFLESS, None),
        exposed(smelter, 32, BlockFlags::MACHINE | BlockFlags::ROOFLESS, None),
        exposed(vault, 33, BlockFlags::INVENTORY, Some(0)),
    ];
    for i in 0..8 {
        d.exposed
            .push(exposed(BlockPos::new(bx - 4 + i, 64, bz - 5), 40 + i as u32, edge, None));
    }
    w.districts.push(d);
    w.visits = visits;
    w.declare();
    w
}

// ---- the fake plugin --------------------------------------------------------------------------

/// What the scripted player says to an envoy that materializes near it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Ignore,
    Pay,
    Refuse,
    Attack,
}

/// What the scripted player does to a hostile band that materializes near it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fight {
    /// Share of each wave killed, `after` ticks after it spawns.
    pub kill_share: f64,
    pub after: u64,
    pub kill_chief: bool,
    /// Fights only waves the raid director sends at the base (an arc), not bands passing by.
    pub only_at_base: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Body {
    epoch: u32,
    faction: u64,
    members: BTreeMap<u32, u32>,
    chief: bool,
}

/// The plugin's side, headless: commands through the codec in, observations through the codec
/// out, each some ticks late (docs/boundary.md §1.2).
pub struct FakeConflict {
    pub player: Uuid,
    pub answer: Answer,
    pub fight: Option<Fight>,
    bodies: BTreeMap<u64, Body>,
    queue: Vec<(Tick, u64, Obs)>,
    seq: u64,
    obs_seq: u64,
    cmd_seq: u64,
    ops: u64,
    pub wire_bytes: u64,
    /// (tick, command name and the gist) for the CLI.
    pub commands: Vec<(u64, String)>,
    /// Ticks a recalled band takes to leave view. PLACEHOLDER 300.
    pub leave_ticks: u64,
}

const SYS_FAKE: SystemId = SystemId::named("conflict.fake_plugin");

impl FakeConflict {
    pub fn new(player: Uuid) -> Self {
        FakeConflict {
            player,
            answer: Answer::Ignore,
            fight: None,
            bodies: BTreeMap::new(),
            queue: Vec::new(),
            seq: 0,
            obs_seq: 0,
            cmd_seq: 0,
            ops: 0,
            wire_bytes: 0,
            commands: Vec::new(),
            leave_ticks: 300,
        }
    }

    pub fn push(&mut self, at: Tick, o: Obs) {
        self.seq += 1;
        self.queue.push((at, self.seq, o));
    }

    fn op(&mut self) -> u64 {
        self.ops += 1;
        let r = StreamKey::new(self.player.1, SYS_FAKE, self.player.0)
            .slot(self.ops)
            .next_u64();
        (1 << 63) | (r >> 1)
    }

    pub fn next_due(&self) -> Option<Tick> {
        self.queue.iter().map(|e| e.0).min()
    }

    /// Observations due by `to`, in time then push order, each through the codec.
    pub fn due(&mut self, to: Tick) -> Vec<(Tick, Obs)> {
        let mut take: Vec<(Tick, u64, Obs)> = Vec::new();
        let mut keep = Vec::new();
        for e in self.queue.drain(..) {
            if e.0 <= to {
                take.push(e);
            } else {
                keep.push(e);
            }
        }
        self.queue = keep;
        take.sort_by_key(|e| (e.0, e.1));
        take.into_iter()
            .map(|(at, _, o)| {
                self.obs_seq += 1;
                let mut buf = Vec::new();
                boundary::encode_obs(self.obs_seq, at.0 as i64, &o, &mut buf);
                self.wire_bytes += buf.len() as u64;
                let (_, back, _) = boundary::decode_obs(&buf).expect("own encoding");
                (at, back)
            })
            .collect()
    }

    /// Applies the engine's commands, through the codec.
    pub fn apply(&mut self, cmds: Vec<Cmd>, at: Tick) {
        for c in cmds {
            self.cmd_seq += 1;
            let mut buf = Vec::new();
            boundary::encode_cmd(self.cmd_seq, &c, &mut buf);
            self.wire_bytes += buf.len() as u64;
            let (_, c, _) = boundary::decode_cmd(&buf).expect("own encoding");
            self.one(c, at);
        }
    }

    fn one(&mut self, c: Cmd, at: Tick) {
        let gist = match &c {
            Cmd::Territory(t) => format!("territory of {:x}, {} blocks", t.faction, t.radius),
            Cmd::Notice(n) => format!("notice, signal {}, grievance {:.1}", n.signal, n.grievance),
            Cmd::Envoy(e) => format!(
                "envoy {:x}: demand {} of item {} or stop signal {}",
                e.agent, e.tribute_count, e.tribute_item, e.signal
            ),
            Cmd::Materialize(m) => format!(
                "band {:x} wave {} purpose {}: {} members, arc {} blocks at {:.0} degrees",
                m.agent,
                m.wave,
                m.purpose,
                m.members.iter().map(|s| s.count).sum::<u32>(),
                m.arc_radius,
                m.bearing_deg
            ),
            Cmd::Steer(s) => format!("steer {:x} to ({:.0}, {:.0})", s.agent, s.pos.0, s.pos.2),
            Cmd::Recall(r) => format!("recall {:x}, reason {}", r.agent, r.reason),
            Cmd::Settled(s) => format!("demand of {:x} settled, outcome {}", s.faction, s.outcome),
        };
        self.commands.push((at.0, format!("{}: {gist}", c.name())));
        match c {
            Cmd::Envoy(e) => {
                self.bodies.insert(
                    e.agent,
                    Body {
                        epoch: e.epoch,
                        faction: e.faction,
                        members: e.members.iter().copied().collect(),
                        chief: false,
                    },
                );
                self.push(
                    at + 1,
                    Obs::Materialized(boundary::BandMaterialized {
                        agent: e.agent,
                        epoch: e.epoch,
                        dimension: e.dimension,
                        wave: 0,
                        spawned: e.members.iter().map(|m| m.1).sum(),
                    }),
                );
                match self.answer {
                    Answer::Pay if e.tribute_count > 0 => {
                        let op = self.op();
                        self.push(
                            at + 200,
                            Obs::TributePaid(boundary::TributePaid {
                                faction: e.faction,
                                agent: e.agent,
                                epoch: e.epoch,
                                player: self.player,
                                op,
                                items: vec![boundary::Item {
                                    item: e.tribute_item,
                                    count: e.tribute_count,
                                }],
                            }),
                        );
                    }
                    Answer::Refuse | Answer::Attack => {
                        self.push(
                            at + 200,
                            Obs::EnvoyAnswer(boundary::EnvoyAnswer {
                                agent: e.agent,
                                epoch: e.epoch,
                                answer: if self.answer == Answer::Refuse {
                                    boundary::ANSWER_REFUSED
                                } else {
                                    boundary::ANSWER_ATTACKED
                                },
                                player: self.player,
                            }),
                        );
                    }
                    _ => {}
                }
            }
            Cmd::Materialize(m) => {
                let b = self.bodies.entry(m.agent).or_insert(Body {
                    epoch: m.epoch,
                    faction: m.faction,
                    members: BTreeMap::new(),
                    chief: false,
                });
                if b.epoch != m.epoch {
                    b.epoch = m.epoch;
                    b.members.clear();
                    b.chief = false;
                }
                for s in &m.members {
                    *b.members.entry(s.body).or_default() += s.count;
                }
                b.chief |= m.flags & boundary::BAND_CHIEF != 0;
                self.push(
                    at + 1,
                    Obs::Materialized(boundary::BandMaterialized {
                        agent: m.agent,
                        epoch: m.epoch,
                        dimension: m.dimension,
                        wave: u32::from(m.wave),
                        spawned: m.members.iter().map(|s| s.count).sum(),
                    }),
                );
                if let Some(f) = self
                    .fight
                    .filter(|f| m.flags & boundary::BAND_HOSTILE != 0 && (!f.only_at_base || m.arc_radius > 0))
                {
                    for s in &m.members {
                        let k = (f64::from(s.count) * f.kill_share).floor() as u32;
                        if k > 0 {
                            if let Some(b) = self.bodies.get_mut(&m.agent)
                                && let Some(n) = b.members.get_mut(&s.body)
                            {
                                *n = n.saturating_sub(k);
                            }
                            self.push(
                                at + f.after,
                                Obs::Casualties(boundary::BandCasualties {
                                    agent: m.agent,
                                    epoch: m.epoch,
                                    by: boundary::BY_PLAYER,
                                    body: s.body,
                                    count: k,
                                    killer: self.player,
                                }),
                            );
                        }
                    }
                    if f.kill_chief && m.flags & boundary::BAND_CHIEF != 0 && m.wave == 0 {
                        let op = self.op();
                        self.push(
                            at + f.after + 20,
                            Obs::ChiefKilled(boundary::ChiefKilled {
                                agent: m.agent,
                                epoch: m.epoch,
                                by: boundary::BY_PLAYER,
                                killer: self.player,
                                op,
                            }),
                        );
                    }
                }
            }
            Cmd::Recall(r) => {
                if let Some(b) = self.bodies.remove(&r.agent)
                    && b.epoch == r.epoch
                {
                    self.push(
                        at + self.leave_ticks,
                        Obs::Dematerialized(boundary::BandDematerialized {
                            agent: r.agent,
                            epoch: r.epoch,
                            reason: 1,
                            survivors: b.members.iter().map(|(&k, &v)| (k, v)).collect(),
                        }),
                    );
                }
            }
            Cmd::Territory(_) | Cmd::Notice(_) | Cmd::Steer(_) | Cmd::Settled(_) => {}
        }
    }

    pub fn embodied(&self) -> usize {
        self.bodies.len()
    }
}

// ---- many factions, for the bench ---------------------------------------------------------------

/// A grid world of `villages` villages with `factions` camps between them (goblins, bandits and
/// raiders in turn), each with an emitting source at its territory's edge and a player who logs
/// there once. Nobody watches. For the faction-count sweep.
pub fn many(villages: usize, factions: usize, seed: u64) -> (World, Conflict) {
    let (goods, _) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&rtsim_world::synth::goods().1);
    let data = rtsim_world::synth::grid(villages, seed);
    let mut w = World::build(world_config(seed), goods, book, data);
    let mut c = conflict(seed, &w);
    let side = (villages as f64).sqrt().ceil() as i32;
    let span = side * 544;
    let fside = (factions as f64).sqrt().ceil() as i32;
    let names = ["goblin", "bandit", "raider"];
    let player = c.player(&mut w, (0xBE, 0x7C));
    for k in 0..factions {
        let (gx, gz) = ((k as i32) % fside, (k as i32) / fside);
        let x = 272 + gx * span / fside.max(1);
        let z = 272 + gz * span / fside.max(1);
        let pos = Pos::new(0, x, z);
        let key = 0xFAC0_0000 + k as u64;
        let Some(f) = c.add_faction(&mut w, key, names[k % 3], pos, Tick::ZERO) else {
            continue;
        };
        let r = c.factions[f as usize].radius as i32;
        c.set_emission(
            &mut w,
            0xD150_0000 + k as u64,
            crate::culture::SignalKind::Smog,
            Pos::new(0, x + r, z),
            Some(DistrictId(0xD150_0000 + k as u64)),
            12.0,
            Tick::ZERO,
        );
        let sig = crate::engine::Signal {
            faction: Some(key),
            kind: crate::culture::SignalKind::Logging,
            amount: 40.0,
            actor: player,
            pos: Pos::new(0, x - r / 2, z),
            district: None,
        };
        c.signal(&mut w, sig, Tick(1));
    }
    (w, c)
}
