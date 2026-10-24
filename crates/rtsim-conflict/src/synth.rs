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

use std::collections::{BTreeMap, BTreeSet};

use rtsim_industry::fake::{self, FDistrict, FInventory, FakeWorld, Visit, exposed, survey};
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{BlockFlags, InvKind};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::Pos;
use rtsim_trade::tunables::Ratio;
use rtsim_trade::{GoodId, Price};
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
/// The key of the player base the scenario's dwell counter marks at the refinery once it is
/// built (P30: a place a player spends much time is a base, a market node caravans visit).
pub const REFINERY_BASE: u64 = 0xBA5E_0001;
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
    conflict_with(seed, w, Cultures::builtin())
}

/// [`conflict`] on the given cultures.
pub fn conflict_with(seed: u64, w: &World, cultures: Cultures) -> Conflict {
    let mut c = Conflict::new(seed, ConflictTunables::default(), cultures, Tick::ZERO);
    define_names(&mut c, w);
    c
}

/// The built-in cultures with the Frostmaw refilling by rate again (MZB-286): the renewal off and
/// `recruits_per_day` back at 1/6, the rate it had before M6. For the engine tests of the refill
/// path (a renewal faction with a rate of 0 and no ledger never refills), which the grove still
/// uses.
pub fn rate_den_cultures() -> Cultures {
    let mut cultures = Cultures::builtin();
    let i = cultures.index_of("frostmaw").expect("the Frostmaw") as usize;
    let mut def = cultures.list[i].def.clone();
    def.renewal = None;
    def.recruits_per_day = 1.0 / 6.0;
    cultures.list[i] = crate::culture::Culture::resolve(def).expect("a Frostmaw with a rate");
    cultures
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

/// Sets good `g`'s price at node `i`'s market to `price` (millionths of a coin), flat whatever its
/// stock: a market that prices a good no recipe makes there, as the gold nuggets of the
/// Umvuthana's tribute (MOWZIE.md ledger 12). A storehouse only; a base keeps no curves.
pub fn set_price(w: &mut World, i: usize, g: GoodId, price: Price, at: Tick) {
    w.run_until(at);
    let region = w.nodes[i].region;
    w.markets.apply(region, at, |m, _| {
        let c = &mut m.goods[g.index()].curve;
        c.anchor = price;
        c.r = Ratio::new(0, 1);
    });
}

/// The refinery: a vault, a distiller and a smelter at the Thornwood's edge, half-walled (6 gaps
/// in 40 perimeter columns), no turrets, loaded as `visits` say.
pub fn refinery(seed: u64, visits: Vec<Visit>) -> FakeWorld {
    // The industry defaults carry nothing and break nothing for ground attackers (natural mobs,
    // vanilla). A faction's raiders carry and break by their units' own data (a goblin warrior 8
    // items and a block, an archer 4 items), which their threats carry (choice C2-1, K2-13's
    // recommendation), so the refinery keeps the industry defaults.
    let tun = Tunables::default();
    let mut w = FakeWorld::new(seed, tun);
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
        exposed(
            distiller,
            31,
            BlockFlags::MACHINE | BlockFlags::ROOFLESS,
            None,
        ),
        exposed(
            smelter,
            32,
            BlockFlags::MACHINE | BlockFlags::ROOFLESS,
            None,
        ),
        exposed(vault, 33, BlockFlags::INVENTORY, Some(0)),
    ];
    for i in 0..8 {
        d.exposed.push(exposed(
            BlockPos::new(bx - 4 + i, 64, bz - 5),
            40 + i as u32,
            edge,
            None,
        ));
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

/// A body in a camp's world: the jar's own entity, tagged with its slot and generation, or a stray
/// (generation 0: untagged, or tagged with a generation the books do not hold).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampBody {
    pub uuid: Uuid,
    /// The engine's body id.
    pub body: u32,
    pub slot: u32,
    pub generation: u32,
    /// Loaded outside the footprint (the census's `BODY_ELSEWHERE`).
    pub elsewhere: bool,
    /// The camp's chief body (a Frostmaw): its death is a `DEATH_CHIEF`.
    pub chief: bool,
    /// The chief body's crystal is on it: a steal takes it.
    pub crystal: bool,
    /// A player stands near it: a MASK takes the warrior farthest from every player, so a body
    /// with none near goes first, and one near a player only when every warrior at home is.
    pub near: bool,
}

/// A camp as the plugin sees it (MOWZIE.md §4.9, emulated): its epoch while it is in (0 while it
/// is out) and the bodies of it the world holds. The bodies stay when the camp goes out: vanilla
/// saves them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CampWorld {
    pub epoch: u64,
    pub bodies: Vec<CampBody>,
    /// A non-spectator player stands within 160 blocks of the throne: a chief SPAWN or SEAT_HEIR
    /// waits (`holdSpawn`, MZB-433). Set it with [`FakeConflict::watch_throne`], which lets the
    /// held lines go at once when the player leaves.
    pub watched: bool,
}

/// How long a chief SPAWN or SEAT_HEIR waits on a player near the throne (`HOLD_MAX`, MZB-433).
pub const HOLD_MAX: u64 = 6_000;

/// The lines of a reconcile the plugin holds on `holdSpawn`, with the reconcile they belong to and
/// the tick they arrived.
#[derive(Clone, Debug)]
struct HeldLines {
    faction: u64,
    epoch: u64,
    op: u64,
    since: Tick,
    lines: Vec<(u32, boundary::ReconcileLine)>,
}

/// One line the plugin applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppliedLine {
    pub at: Tick,
    pub faction: u64,
    pub op: u64,
    /// The line's index in its reconcile: with `op`, what I4 counts.
    pub index: u32,
    pub line: boundary::ReconcileLine,
    /// `LINE_DONE`, `LINE_NOTHING` or `LINE_STALE`.
    pub outcome: u8,
}

/// A record a body or a marker carries until the engine has it: one line's answer, with the
/// reconcile it answers (MOWZIE.md §4.9, *Orders and answers*).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holder {
    pub faction: u64,
    pub epoch: u64,
    pub op: u64,
    pub line: boundary::LineAnswer,
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
    /// Plays every notice and answers NOTICE_SHOWN a tick later (MOWZIE.md §4.11); off, the
    /// notices stay unplayed and the engine sends them again each session.
    pub play_notices: bool,
    /// The world's difficulty, sent as DIFFICULTY at each [`FakeConflict::session_start`].
    pub difficulty: u32,
    /// The camps' worlds, by faction key (MOWZIE.md §4.9).
    pub camps: BTreeMap<u64, CampWorld>,
    /// Every camp line applied, in order.
    pub applied: Vec<AppliedLine>,
    /// Answers to CAMP_RECONCILEs that are applied but never reach the engine: a crash between the
    /// world's save and the journal's (§4.9's crash table). Counted down, one per answer.
    pub lose_answers: u32,
    /// What the entities carry (MOWZIE.md §4.9): a line applied and done leaves its record on a
    /// body or a marker until the engine has it, so that a record lost on its way to the engine
    /// (`lose_answers`, a restart's dropped queue) is still on its holder.
    pub holders: Vec<Holder>,
    /// The deaths a marker carries where the body was, until the engine has them.
    pub death_holders: Vec<boundary::CampDeath>,
    /// Journals the holders' records again as the real plugin does (MOWZIE.md §4.9): at a session
    /// start ([`FakeConflict::journal_holders`], which [`CampRun::restart`] calls) and at each
    /// camp-in, before the census, which waits for the footprint's holders (MZB-174). Off, a
    /// record lost is lost, and a party that comes home before a census meanwhile leaves a ghost
    /// body (three censuses write it off): the case the crash table says the holders prevent.
    pub holders_again: bool,
    /// The most lines a CAMP_BODIES record carries (0: all of a reconcile's): one reconcile has
    /// several records, as holders in several chunks answer it (docs/boundary.md §9.6).
    pub split_answers: usize,
    /// Sends every answer twice: a replay.
    pub replay_answers: bool,
    /// Ticks an answer takes to reach the engine.
    pub answer_ticks: u64,
    /// Every owed hunt the engine sent, in order (OWED_HUNT, MZB-425).
    pub hunts_sent: Vec<boundary::OwedHunt>,
    /// The hunts the plugin keeps, the latest for each faction and district, until it applies
    /// them (MZB-426).
    pub hunts: Vec<boundary::OwedHunt>,
    /// Applies each hunt a tick after it arrives, killing up to this many animals (HUNT_APPLIED
    /// even when it kills none); `None` keeps the hunts unapplied.
    pub hunt_kills: Option<u32>,
    uuids: u64,
    /// Members that die on the way out of a recall: taken off the survivors BAND_DEMATERIALIZED
    /// counts, at the recall, first stacks first (0: none; MZB-131).
    pub walk_off_deaths: u32,
    /// The chief lines waiting on a player near the throne (`holdSpawn`): applied when the player
    /// leaves ([`FakeConflict::watch_throne`]) or `HOLD_MAX` ticks after they arrived, answered
    /// stale at the camp's out.
    held: Vec<HeldLines>,
}

/// A warrior a MASK may take: tagged, not the chief, in the footprint.
fn warrior_at_home(b: &CampBody) -> bool {
    b.generation != 0 && !b.chief && !b.elsewhere
}

/// A line that waits on `holdSpawn` (MZB-433): a chief's SPAWN, the heir's SEAT_HEIR and the MASK
/// that goes with it, which Java does after the heir is made.
fn waits_on_throne(l: &boundary::ReconcileLine) -> bool {
    match l.action {
        boundary::ACTION_SEAT_HEIR | boundary::ACTION_MASK => true,
        boundary::ACTION_SPAWN => l.flags & boundary::LINE_CHIEF != 0,
        _ => false,
    }
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
            play_notices: true,
            difficulty: boundary::DIFFICULTY_NORMAL,
            camps: BTreeMap::new(),
            applied: Vec::new(),
            lose_answers: 0,
            holders: Vec::new(),
            death_holders: Vec::new(),
            holders_again: false,
            split_answers: 0,
            replay_answers: false,
            answer_ticks: 1,
            hunts_sent: Vec::new(),
            hunts: Vec::new(),
            hunt_kills: None,
            uuids: 0,
            walk_off_deaths: 0,
            held: Vec::new(),
        }
    }

    /// A session's start, as the plugin's: the world's difficulty, at `at`.
    pub fn session_start(&mut self, at: Tick) {
        let difficulty = self.difficulty;
        self.push(at, Obs::Difficulty(boundary::Difficulty { difficulty }));
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
        let held = self.held.iter().map(|h| h.since + HOLD_MAX);
        self.queue.iter().map(|e| e.0).chain(held).min()
    }

    /// Observations due by `to`, in time then push order, each through the codec.
    pub fn due(&mut self, to: Tick) -> Vec<(Tick, Obs)> {
        self.release_held(to);
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
                if let Obs::CampDeath(d) = &back {
                    self.death_holders
                        .retain(|h| !(h.faction == d.faction && h.op == d.op));
                }
                if let Obs::CampBodies(b) = &back {
                    // The engine has the record: its holders drop it (the acknowledgement).
                    self.holders.retain(|h| {
                        !(h.faction == b.faction
                            && h.op == b.op
                            && b.lines.iter().any(|l| l.line == h.line.line))
                    });
                }
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
            Cmd::AmbushSpot(a) => format!(
                "ambush spot of {:x} at ({:.0}, {:.0}), flags {}",
                a.agent, a.spot.0, a.spot.2, a.flags
            ),
            Cmd::Reconcile(r) => format!(
                "camp {:x} epoch {}: {} lines",
                r.faction,
                r.epoch,
                r.lines.len()
            ),
            Cmd::TrainRobbed(t) => format!("train {:x} robbed by {:x}", t.train, t.faction),
            Cmd::ChiefBody(b) => format!(
                "chief of {:x} with {:x}, health {:.1}",
                b.faction, b.agent, b.health
            ),
            Cmd::March(m) => format!(
                "march {:x} kind {} phase {}: {} waypoints",
                m.agent,
                m.kind,
                m.phase,
                m.waypoints.len()
            ),
            Cmd::OwedHunt(h) => format!(
                "hunt of {:x} near district {}: {} animals within {} blocks",
                h.faction, h.district, h.count, h.radius
            ),
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
                if let Some(f) = self.fight.filter(|f| {
                    m.flags & boundary::BAND_HOSTILE != 0 && (!f.only_at_base || m.arc_radius > 0)
                }) {
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
                        // The chief's body dies with it: one unit of the first stack, which is no
                        // survivor when the band leaves (MZB-272).
                        if let Some(b) = self.bodies.get_mut(&m.agent)
                            && let Some(n) = b.members.values_mut().next()
                        {
                            *n = n.saturating_sub(1);
                        }
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
                    // Some die on the way out: the first stacks lose them first.
                    let mut dead = self.walk_off_deaths;
                    let survivors = b
                        .members
                        .iter()
                        .map(|(&k, &v)| {
                            let lost = v.min(dead);
                            dead -= lost;
                            (k, v - lost)
                        })
                        .collect();
                    self.push(
                        at + self.leave_ticks,
                        Obs::Dematerialized(boundary::BandDematerialized {
                            agent: r.agent,
                            epoch: r.epoch,
                            reason: 1,
                            survivors,
                        }),
                    );
                }
            }
            Cmd::Reconcile(r) => self.reconcile(&r, at),
            Cmd::OwedHunt(h) => {
                self.hunts_sent.push(h);
                self.hunts
                    .retain(|e| !(e.faction == h.faction && e.district == h.district));
                self.hunts.push(h);
                if let Some(k) = self.hunt_kills {
                    self.apply_hunts(k, at + self.answer_ticks);
                }
            }
            Cmd::Notice(n) if self.play_notices => {
                self.push(
                    at + 1,
                    Obs::NoticeShown(boundary::NoticeShown {
                        faction: n.faction,
                        serial: n.serial,
                        player: n.player,
                    }),
                );
            }
            Cmd::Territory(_)
            | Cmd::Notice(_)
            | Cmd::Steer(_)
            | Cmd::Settled(_)
            | Cmd::AmbushSpot(_)
            | Cmd::TrainRobbed(_)
            | Cmd::ChiefBody(_)
            | Cmd::March(_) => {}
        }
    }

    pub fn embodied(&self) -> usize {
        self.bodies.len()
    }

    /// Applies every hunt the plugin keeps, each killing up to `kills` animals, at `at`: the
    /// answer is HUNT_APPLIED whatever it killed (MZB-426).
    pub fn apply_hunts(&mut self, kills: u32, at: Tick) {
        for h in std::mem::take(&mut self.hunts) {
            self.push(
                at,
                Obs::HuntApplied(boundary::HuntApplied {
                    faction: h.faction,
                    district: h.district,
                    op: h.op,
                    killed: kills.min(h.count),
                }),
            );
        }
    }

    // ---- camps (MOWZIE.md §4.9, emulated) -------------------------------------------------

    fn camp_uuid(&mut self) -> Uuid {
        self.uuids += 1;
        (
            0xCA3F_0000_0000_0000 | (self.player.1 & 0xFFFF_FFFF),
            self.uuids,
        )
    }

    /// The camp's world, made when first named.
    pub fn camp_world(&mut self, faction: u64) -> &mut CampWorld {
        self.camps.entry(faction).or_default()
    }

    /// A body the jar placed in a camp, untagged (or a body a player put down, or one tagged with
    /// a generation nobody holds: `tagged`): a stray. Its UUID.
    pub fn camp_stray(&mut self, faction: u64, body: u32) -> Uuid {
        self.camp_tagged_body(faction, body, 0, 0)
    }

    /// A body tagged `slot` and `generation` placed in a camp's world, as saved by vanilla.
    pub fn camp_tagged_body(
        &mut self,
        faction: u64,
        body: u32,
        slot: u32,
        generation: u32,
    ) -> Uuid {
        let uuid = self.camp_uuid();
        self.camp_world(faction).bodies.push(CampBody {
            uuid,
            body,
            slot,
            generation,
            elsewhere: false,
            chief: false,
            crystal: false,
            near: false,
        });
        uuid
    }

    /// The camp's entities have all come in: a new epoch (a fresh Java op id) and the census of
    /// its bodies, at `at`. Its epoch.
    pub fn camp_in(&mut self, faction: u64, at: Tick) -> u64 {
        self.camp_in_with(faction, 0, at)
    }

    /// [`FakeConflict::camp_in`] with the census's flags (`CENSUS_*`).
    pub fn camp_in_with(&mut self, faction: u64, flags: u32, at: Tick) -> u64 {
        // The census waits until every holder in the footprint has journaled its record again: the
        // records reach the engine first, carrying the epoch that is still the latest (MZB-174).
        if self.holders_again {
            self.journal_holders_of(Some(faction), at);
        }
        let epoch = self.op();
        let camp = self.camp_world(faction);
        camp.epoch = epoch;
        // The chief at home says whether its crystal is on it (MZB-269).
        let flags = flags
            | camp.bodies.iter().find(|b| b.chief).map_or(0, |b| {
                boundary::CENSUS_CHIEF
                    | if b.crystal {
                        boundary::CENSUS_CRYSTAL
                    } else {
                        0
                    }
            });
        let bodies: Vec<boundary::CensusBody> = camp
            .bodies
            .iter()
            .map(|b| boundary::CensusBody {
                uuid: b.uuid,
                body: b.body,
                slot: b.slot,
                generation: b.generation,
                flags: (if b.generation == 0 {
                    boundary::BODY_STRAY
                } else {
                    0
                }) | (if b.elsewhere {
                    boundary::BODY_ELSEWHERE
                } else {
                    0
                }) | (if b.chief { boundary::BODY_CHIEF } else { 0 }),
            })
            .collect();
        self.push(
            at,
            Obs::CampCensus(boundary::CampCensus {
                faction,
                epoch,
                flags,
                health: 0.0,
                throne: (0.0, 0.0, 0.0),
                direction: 0.0,
                traded_slot: 0,
                traded_generation: 0,
                bodies,
                traded: Vec::new(),
            }),
        );
        epoch
    }

    /// A footprint chunk dropped below FULL: the camp is out, at `at`. A camp that is out stays so.
    pub fn camp_out(&mut self, faction: u64, at: Tick) {
        let camp = self.camp_world(faction);
        if camp.epoch == 0 {
            return;
        }
        let epoch = std::mem::take(&mut camp.epoch);
        self.push(at, Obs::CampOut(boundary::CampOut { faction, epoch }));
        // An epoch's end answers the lines still held stale (MZB-433).
        for h in self.take_held(Some(faction)) {
            self.apply_lines(h.faction, h.epoch, h.op, false, h.lines, at);
        }
    }

    /// A player comes within 160 blocks of the throne, or leaves it: the chief lines held on it
    /// go at `at` when the player leaves (MZB-433's `holdSpawn`).
    pub fn watch_throne(&mut self, faction: u64, watched: bool, at: Tick) {
        self.camp_world(faction).watched = watched;
        if !watched {
            for h in self.take_held(Some(faction)) {
                self.apply_held(h, at);
            }
        }
    }

    /// The held lines of `faction` (every camp's with `None`), taken out of the plugin.
    fn take_held(&mut self, faction: Option<u64>) -> Vec<HeldLines> {
        let (taken, kept) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|h| faction.is_none_or(|f| f == h.faction));
        self.held = kept;
        taken
    }

    /// The lines whose `HOLD_MAX` is up by `to` go, each at its own deadline.
    fn release_held(&mut self, to: Tick) {
        let (mut over, kept): (Vec<HeldLines>, Vec<HeldLines>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|h| h.since + HOLD_MAX <= to);
        self.held = kept;
        over.sort_by_key(|h| h.since);
        for h in over {
            let at = h.since + HOLD_MAX;
            self.apply_held(h, at);
        }
    }

    /// Held lines go: applied if the camp is still in at their epoch, else answered stale.
    fn apply_held(&mut self, h: HeldLines, at: Tick) {
        let live = self
            .camps
            .get(&h.faction)
            .is_some_and(|c| c.epoch != 0 && c.epoch == h.epoch);
        self.apply_lines(h.faction, h.epoch, h.op, live, h.lines, at);
    }

    /// Whether the camp is in.
    pub fn camp_is_in(&self, faction: u64) -> bool {
        self.camps.get(&faction).is_some_and(|c| c.epoch != 0)
    }

    /// A thief takes the crystal off the camp's chief body (the steal mixin, MZB-276): the body
    /// stands robbed and CHIEF_STATE with the flag off is on its way at `at`. `false` when the camp
    /// holds no chief with a crystal.
    pub fn steal(&mut self, faction: u64, at: Tick) -> bool {
        let camp = self.camp_world(faction);
        let Some(b) = camp.bodies.iter_mut().find(|b| b.chief && b.crystal) else {
            return false;
        };
        b.crystal = false;
        self.push(
            at,
            Obs::ChiefState(boundary::ChiefState {
                faction,
                flags: 0,
                health: 100.0,
            }),
        );
        true
    }

    /// A body of the camp died, or was carried off (`by`): gone from the world, and a CAMP_DEATH
    /// at `at`. `false` when the camp holds no such body.
    pub fn camp_kill(&mut self, faction: u64, uuid: Uuid, by: u32, at: Tick) -> bool {
        let camp = self.camp_world(faction);
        let Some(i) = camp.bodies.iter().position(|b| b.uuid == uuid) else {
            return false;
        };
        let b = camp.bodies.remove(i);
        let op = self.op();
        let death = boundary::CampDeath {
            faction,
            by,
            body: b.body,
            flags: (if b.generation == 0 {
                boundary::DEATH_STRAY
            } else {
                0
            }) | (if b.chief { boundary::DEATH_CHIEF } else { 0 }),
            slot: b.slot,
            generation: b.generation,
            killer: if by == boundary::BY_PLAYER {
                self.player
            } else {
                (0, 0)
            },
            op,
        };
        if b.generation != 0 {
            // A member's body leaves a marker with its death (a stray's leaves none: nothing is
            // booked for it).
            self.death_holders.push(death);
        }
        self.push(at, Obs::CampDeath(death));
        true
    }

    /// Whether an observation about the camp is on its way to the engine (a census, an answer, a
    /// death, an out): the engine's books are behind the world's until it lands.
    pub fn camp_pending(&self, faction: u64) -> bool {
        self.queue.iter().any(|(_, _, o)| match o {
            Obs::CampCensus(x) => x.faction == faction,
            Obs::CampBodies(x) => x.faction == faction,
            Obs::CampDeath(x) => x.faction == faction,
            Obs::CampOut(x) => x.faction == faction,
            _ => false,
        })
    }

    /// A copy of the observations not yet delivered, in the order they are queued.
    pub fn queued(&self) -> Vec<Obs> {
        self.queue.iter().map(|e| e.2.clone()).collect()
    }

    /// A shutdown before the journal's write: every observation not yet delivered is lost. The
    /// world keeps what it did (a body killed, a line applied), the engine never hears of it.
    pub fn drop_queued(&mut self) -> Vec<Obs> {
        self.queue.drain(..).map(|e| e.2).collect()
    }

    /// The camp's bodies carrying `slot` and `generation`.
    pub fn camp_held(&self, faction: u64, slot: u32, generation: u32) -> Vec<CampBody> {
        self.camps.get(&faction).map_or_else(Vec::new, |c| {
            c.bodies
                .iter()
                .filter(|b| b.generation != 0 && b.slot == slot && b.generation == generation)
                .copied()
                .collect()
        })
    }

    /// One CAMP_RECONCILE: applied while the camp is in at its epoch, every line in the tick it
    /// arrives, else every line answered stale and nothing applied; answered with CAMP_BODIES
    /// (§4.9, *Orders and answers*). With a player near the throne the chief's SPAWN and the heir's
    /// pair wait (`holdSpawn`, MZB-433) and are answered when they go.
    fn reconcile(&mut self, r: &boundary::CampReconcile, at: Tick) {
        let live = self
            .camps
            .get(&r.faction)
            .is_some_and(|c| c.epoch != 0 && c.epoch == r.epoch);
        let watched = live && self.camps.get(&r.faction).is_some_and(|c| c.watched);
        let (later, now): (Vec<_>, Vec<_>) = r
            .lines
            .iter()
            .copied()
            .enumerate()
            .map(|(i, l)| (i as u32, l))
            .partition(|(_, l)| watched && waits_on_throne(l));
        if !later.is_empty() {
            self.held.push(HeldLines {
                faction: r.faction,
                epoch: r.epoch,
                op: r.op,
                since: at,
                lines: later,
            });
        }
        self.apply_lines(r.faction, r.epoch, r.op, live, now, at);
    }

    /// Lines of one reconcile applied now (`live`) or answered stale, and answered. The heir's pair
    /// is one unit (MZB-408): if either line cannot be done, both are stale and nothing changes.
    fn apply_lines(
        &mut self,
        faction: u64,
        epoch: u64,
        op: u64,
        live: bool,
        lines: Vec<(u32, boundary::ReconcileLine)>,
        at: Tick,
    ) {
        if lines.is_empty() {
            return;
        }
        // The warrior to mask must be at home, and the heir's body not stand yet.
        let pair = lines.iter().any(|(_, l)| l.action == boundary::ACTION_MASK);
        let pair_ok = !pair || (live && self.pair_possible(faction, &lines));
        let mut answers = Vec::new();
        for (index, l) in &lines {
            let mut taken = None;
            let outcome = if !live
                || (!pair_ok
                    && matches!(l.action, boundary::ACTION_MASK | boundary::ACTION_SEAT_HEIR))
            {
                boundary::LINE_STALE
            } else if l.action == boundary::ACTION_MASK {
                let (outcome, slot, generation) = self.mask(faction);
                taken = Some((slot, generation));
                outcome
            } else {
                self.apply_line(faction, l)
            };
            self.applied.push(AppliedLine {
                at,
                faction,
                op,
                index: *index,
                line: *l,
                outcome,
            });
            // A MASK names the warrior the plugin took, not the line's own (zero) slot.
            let (slot, generation) = taken.unwrap_or((l.slot, l.generation));
            let answer = boundary::LineAnswer {
                line: *index,
                slot,
                generation,
                action: l.action,
                outcome,
            };
            if outcome == boundary::LINE_DONE {
                // A change leaves its record where it was made; one that changed nothing does not.
                self.holders.push(Holder {
                    faction,
                    epoch,
                    op,
                    line: answer,
                });
            }
            answers.push(answer);
        }
        if self.lose_answers > 0 {
            self.lose_answers -= 1;
            return;
        }
        for answer in self.records(faction, epoch, op, answers) {
            if self.replay_answers {
                self.push(at + self.answer_ticks, answer.clone());
            }
            self.push(at + self.answer_ticks, answer);
        }
    }

    /// The lines as CAMP_BODIES records of at most `split_answers` lines each.
    fn records(
        &self,
        faction: u64,
        epoch: u64,
        op: u64,
        lines: Vec<boundary::LineAnswer>,
    ) -> Vec<Obs> {
        let n = if self.split_answers == 0 {
            lines.len().max(1)
        } else {
            self.split_answers
        };
        lines
            .chunks(n)
            .map(|c| {
                Obs::CampBodies(boundary::CampBodies {
                    faction,
                    epoch,
                    op,
                    lines: c.to_vec(),
                })
            })
            .collect()
    }

    /// A session's start, as the holders' records load: each record a holder still has, journaled
    /// again at `at` with its reconcile's op id, grouped by op and split as answers are
    /// (MOWZIE.md §4.9: the engine skips what it has, and books the rest by the action it carries).
    pub fn journal_holders(&mut self, at: Tick) {
        self.journal_holders_of(None, at + self.answer_ticks);
    }

    /// The holders of one camp (every camp's with `None`) journal their records again at `at`: the
    /// records first, grouped by op, then the deaths.
    fn journal_holders_of(&mut self, only: Option<u64>, at: Tick) {
        let mut groups: Vec<((u64, u64, u64), Vec<boundary::LineAnswer>)> = Vec::new();
        for h in self
            .holders
            .iter()
            .filter(|h| only.is_none_or(|f| f == h.faction))
        {
            let k = (h.faction, h.epoch, h.op);
            match groups.iter_mut().find(|g| g.0 == k) {
                Some(g) => g.1.push(h.line),
                None => groups.push((k, vec![h.line])),
            }
        }
        for ((faction, epoch, op), lines) in groups {
            for o in self.records(faction, epoch, op, lines) {
                self.push(at, o);
            }
        }
        for d in self.death_holders.clone() {
            if only.is_none_or(|f| f == d.faction) {
                self.push(at, Obs::CampDeath(d));
            }
        }
    }

    /// Whether the heir's pair in `lines` can be done whole: a warrior to mask, and no body of the
    /// seat's slot and generation standing already.
    fn pair_possible(&mut self, faction: u64, lines: &[(u32, boundary::ReconcileLine)]) -> bool {
        let camp = self.camp_world(faction);
        let warrior = camp.bodies.iter().any(warrior_at_home);
        let standing = lines
            .iter()
            .map(|(_, l)| l)
            .filter(|l| l.action == boundary::ACTION_SEAT_HEIR)
            .any(|l| {
                camp.bodies
                    .iter()
                    .any(|b| l.generation != 0 && b.slot == l.slot && b.generation == l.generation)
            });
        warrior && !standing
    }

    /// MASK: the plugin takes the warrior at home farthest from every player, for the heir
    /// (MZB-408). The emulator has no distances: the last one tagged that no player stands near,
    /// else (every warrior at home has a player near) the last one tagged, as Java takes the
    /// farthest all the same and answers STALE only with none at home. No warrior here has a line
    /// in flight or held: a line is applied the tick it arrives, and a held line is a chief's.
    /// Its outcome, slot and generation; no warrior to take is LINE_NOTHING with a zero
    /// generation.
    fn mask(&mut self, faction: u64) -> (u8, u32, u32) {
        let camp = self.camp_world(faction);
        let Some(i) = camp
            .bodies
            .iter()
            .rposition(|b| warrior_at_home(b) && !b.near)
            .or_else(|| camp.bodies.iter().rposition(warrior_at_home))
        else {
            return (boundary::LINE_NOTHING, 0, 0);
        };
        let b = camp.bodies.remove(i);
        (boundary::LINE_DONE, b.slot, b.generation)
    }

    /// One line, applied to the camp's world: its target state, so that applying it twice changes
    /// nothing.
    fn apply_line(&mut self, faction: u64, l: &boundary::ReconcileLine) -> u8 {
        use boundary::{
            ACTION_ADOPT, ACTION_HAND_BACK, ACTION_REMOVE, ACTION_SEAT_HEIR, ACTION_SET_OUT,
            ACTION_SPAWN, ACTION_TAKE, LINE_DONE, LINE_NOTHING,
        };
        let uuid = self.camp_uuid();
        let camp = self.camp_world(faction);
        let held = camp
            .bodies
            .iter()
            .position(|b| l.generation != 0 && b.slot == l.slot && b.generation == l.generation);
        match l.action {
            ACTION_SPAWN | ACTION_HAND_BACK => {
                if held.is_some() {
                    return LINE_NOTHING;
                }
                camp.bodies.push(CampBody {
                    uuid,
                    body: l.body,
                    slot: l.slot,
                    generation: l.generation,
                    elsewhere: false,
                    chief: l.flags & boundary::LINE_CHIEF != 0,
                    crystal: l.flags & boundary::LINE_CRYSTAL != 0,
                    near: false,
                });
                LINE_DONE
            }
            ACTION_SEAT_HEIR => {
                // The heir takes the throne: the chief's body is the one of this generation. A
                // chief body of another generation, dead away or not, is replaced (MZB-410).
                if held.is_some() {
                    return LINE_NOTHING;
                }
                camp.bodies.retain(|b| b.slot != l.slot);
                camp.bodies.push(CampBody {
                    uuid,
                    body: l.body,
                    slot: l.slot,
                    generation: l.generation,
                    elsewhere: false,
                    chief: true,
                    crystal: l.flags & boundary::LINE_CRYSTAL != 0,
                    near: false,
                });
                LINE_DONE
            }
            ACTION_ADOPT => {
                match camp
                    .bodies
                    .iter_mut()
                    .find(|b| b.generation == 0 && (b.uuid.0, b.uuid.1) == (l.a, l.b))
                {
                    Some(b) => {
                        b.slot = l.slot;
                        b.generation = l.generation;
                        LINE_DONE
                    }
                    None => LINE_NOTHING,
                }
            }
            ACTION_TAKE | ACTION_SET_OUT | ACTION_REMOVE => {
                let at = if l.action == ACTION_REMOVE && l.generation == 0 {
                    camp.bodies
                        .iter()
                        .position(|b| (b.uuid.0, b.uuid.1) == (l.a, l.b))
                } else {
                    held
                };
                match at {
                    Some(i) => {
                        camp.bodies.remove(i);
                        LINE_DONE
                    }
                    None => LINE_NOTHING,
                }
            }
            _ => LINE_NOTHING,
        }
    }
}

// ---- many factions, for the bench ---------------------------------------------------------------

/// A grid world of `villages` villages with `factions` camps between them (goblins, bandits and
/// raiders in turn), each with an emitting source inside its territory and a player who logs
/// there once. Nobody watches. For the faction-count sweep. The camps are havens built with the
/// world (not joined one by one), so a big point builds fast.
pub fn many(villages: usize, factions: usize, seed: u64) -> (World, Conflict) {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let mut data = rtsim_world::synth::grid(villages, seed);
    let side = (villages as f64).sqrt().ceil() as i32;
    let span = side * 544;
    let fside = (factions as f64).sqrt().ceil() as i32;
    let camp = |k: usize| {
        let (gx, gz) = ((k as i32) % fside, (k as i32) / fside);
        let step = span / fside.max(1);
        Pos::new(0, step / 2 + gx * step, step / 2 + gz * step)
    };
    for k in 0..factions {
        data.push(rtsim_world::NodeData {
            key: 0xFAC0_0000 + k as u64,
            kind: rtsim_settle::NodeKind::Haven(rtsim_settle::Culture::Human),
            pos: camp(k),
            pop: 10,
            resources: BTreeMap::new(),
        });
    }
    let mut w = World::build(world_config(seed), goods, book, data);
    let mut c = conflict(seed, &w);
    let names = ["goblin", "bandit", "raider"];
    let player = c.player(&mut w, (0xBE, 0x7C));
    for k in 0..factions {
        let pos = camp(k);
        let key = 0xFAC0_0000 + k as u64;
        let Some(f) = c.add_faction(&mut w, key, names[k % 3], pos, Tick::ZERO) else {
            continue;
        };
        let r = c.factions[f as usize].radius as i32;
        c.set_emission(
            &mut w,
            0xD150_0000 + k as u64,
            crate::culture::SignalKind::Smog,
            Pos::new(0, pos.x + r / 4, pos.z),
            Some(DistrictId(0xD150_0000 + k as u64)),
            12.0,
            Tick::ZERO,
        );
        let sig = crate::engine::Signal {
            faction: Some(key),
            kind: crate::culture::SignalKind::Logging,
            amount: 40.0,
            actor: player,
            pos: Pos::new(0, pos.x - r / 2, pos.z),
            district: None,
        };
        c.signal(&mut w, sig, Tick(1));
    }
    (w, c)
}

// ---- a camp against the emulated plugin (MOWZIE.md §4.9) --------------------------------------

/// Dictionary kind 4's id the camp runs name their structure by.
pub const CAMP_STRUCTURE: u32 = 10;

const SYS_CHAOS: SystemId = SystemId::named("conflict.camp_chaos");

/// One camp of a culture whose bodies a census counts, played against the emulated plugin
/// ([`FakeConflict`]'s camp worlds): found by FACTION_FOUND with the jar's own bodies standing in
/// it, as a plugin would. For the camp tests and `rtsim-cli sim mowzie` run 10.
pub struct CampRun {
    pub w: World,
    pub c: Conflict,
    pub plugin: FakeConflict,
    pub player: rtsim_knowledge::ActorId,
    pub key: u64,
    pub f: u32,
    /// Every command the engine sent, with when.
    pub cmds: Vec<(Tick, Cmd)>,
    /// The engine's body id of the camp's pool members.
    pub body: u32,
    /// The ghosts now: members the books hold as bodies at home that the world has no body for,
    /// with nothing on its way to tell the books (a record queued or on a holder). Three censuses
    /// write one off; one drawn first is rescued (its set-out finds nothing to do and books the
    /// body gone). See [`CampRun::track`].
    pub ghosts: BTreeSet<(u32, u32)>,
    /// Ghosts that left the books: each was written off, and a body-less member leaves the books
    /// no other way. The engine's write-offs must be exactly these.
    pub ghosts_gone: u32,
    applied_keys: BTreeSet<(u64, u32)>,
    applied_checked: usize,
    /// Everything a check found wrong, with when.
    pub failures: Vec<String>,
    /// The player stands at the refinery (observed every 1,200 ticks) while this is set: a band
    /// that comes near is materialized and fought as the plugin's scripted player does.
    pub watch: bool,
    /// Whether the chaos run's saves are made (off, they are skipped: the run to compare with).
    pub round_trips: bool,
    /// Whether the chaos run checks the books after each event (off, it checks nothing until
    /// asked: the run to compare with, for an audit that must change nothing).
    pub audits: bool,
    /// The infrastructure the run's engine acts on (none by default: a raid is skipped). A test
    /// puts walls here to have a raid on a district fought abstractly (MZB-266).
    pub infra: Box<dyn rtsim_ecology::infra::Infrastructure>,
    /// What the chaos run did, with when: for a failure's report.
    pub events: Vec<(Tick, String)>,
    next_obs: Tick,
}

impl CampRun {
    /// A camp of `culture` (`frostmaw` or `umvuthana`) found with `found` of its own bodies
    /// standing in it, at the first tick. The camp is out: nothing has loaded its chunks yet.
    pub fn new(seed: u64, culture: &str, found: u16) -> CampRun {
        CampRun::with_cultures(seed, Cultures::builtin(), culture, found)
    }

    /// [`CampRun::new`] on the given cultures.
    pub fn with_cultures(seed: u64, cultures: Cultures, culture: &str, found: u16) -> CampRun {
        CampRun::found(seed, cultures, culture, found, false)
    }

    /// [`CampRun::new`] with its chief found already robbed of its crystal (the census byte says
    /// so, MZB-257).
    pub fn robbed(seed: u64, culture: &str, found: u16) -> CampRun {
        CampRun::found(seed, Cultures::builtin(), culture, found, true)
    }

    fn found(seed: u64, cultures: Cultures, culture: &str, found: u16, robbed: bool) -> CampRun {
        let (mut w, _) = world(seed);
        let mut c = conflict_with(seed, &w, cultures);
        let ci = c.cultures.index_of(culture).expect("a built-in culture");
        let structure = c.cultures.get(ci).def.structures[0].clone();
        c.define(4, CAMP_STRUCTURE, &structure);
        let player = c.player(&mut w, PLAYER);
        let mut plugin = FakeConflict::new(PLAYER);
        let key = CAMP;
        let body = {
            let cu = c.cultures.get(ci);
            let unit = [
                crate::culture::PartyKind::Raid,
                crate::culture::PartyKind::Invasion,
            ]
            .into_iter()
            .find_map(|k| cu.parties[k.index()].first().map(|u| u.0))
            .expect("a war party");
            crate::engine::body_id(&c.cultures, &cu.unit(unit).body)
        };
        for _ in 0..found {
            plugin.camp_stray(key, body);
        }
        // A chief's unit (one with a row of its own without its crystal) found in its camp is the
        // jar's chief, crystal on it: the census byte says so (MZB-257, MZB-272).
        let chief = {
            let cu = c.cultures.get(ci);
            let u = [
                crate::culture::PartyKind::Raid,
                crate::culture::PartyKind::Invasion,
            ]
            .into_iter()
            .find_map(|k| cu.parties[k.index()].first().map(|u| u.0))
            .expect("a war party");
            cu.bare[usize::from(u)] != u && found > 0
        };
        if chief && let Some(b) = plugin.camp_world(key).bodies.first_mut() {
            b.chief = true;
            b.crystal = !robbed;
        }
        c.ingest(
            &mut w,
            &mut rtsim_ecology::infra::NoInfrastructure,
            &Obs::FactionFound(boundary::FactionFound {
                faction: key,
                flags: 0,
                dimension: u32::from(CAMP_POS.frame),
                structure: CAMP_STRUCTURE,
                pos: (CAMP_POS.x, 64, CAMP_POS.z),
                home_bodies: found,
                census: if chief {
                    (boundary::CENSUS_CHIEF | if robbed { 0 } else { boundary::CENSUS_CRYSTAL })
                        as u8
                } else {
                    0
                },
            }),
            Tick(1),
        );
        let f = c.faction_of_key(key).expect("found");
        let mut r = CampRun {
            w,
            c,
            plugin,
            player,
            key,
            f,
            cmds: Vec::new(),
            body,
            ghosts: BTreeSet::new(),
            ghosts_gone: 0,
            applied_keys: BTreeSet::new(),
            applied_checked: 0,
            failures: Vec::new(),
            watch: false,
            round_trips: true,
            audits: true,
            infra: Box::new(rtsim_ecology::infra::NoInfrastructure),
            events: Vec::new(),
            next_obs: Tick::ZERO,
        };
        r.flush(Tick(1));
        r
    }

    /// Looks at the books against the world for the ghosts (`ghosts`, `ghosts_gone`): a member
    /// whose slot is a body at home with no body in the world, and no record queued for the camp
    /// that could tell the books. A record only a holder has is cover when the plugin journals
    /// holders again (`holders_again`): it reaches the engine at the next camp-in, before the
    /// census, or at the next session start, and books by the action it carries (MZB-174). Without
    /// it the member is a ghost until three censuses write it off, or a record or a draw changes
    /// the slot first. The set is looked at after every step of [`CampRun::run`] while `audits` is
    /// on, so a ghost, which lives two days at least, is seen.
    pub fn track(&mut self) {
        let Some(camp) = self.c.camp(self.f) else {
            return;
        };
        let held: BTreeSet<(u32, u32)> = self
            .plugin
            .camps
            .get(&self.key)
            .map(|c| {
                c.bodies
                    .iter()
                    .filter(|b| b.generation != 0)
                    .map(|b| (b.slot, b.generation))
                    .collect()
            })
            .unwrap_or_default();
        let quiet = !self.plugin.camp_pending(self.key);
        // A record on a holder tells the books at the next camp-in, if holders journal again.
        let covered = |slot: u32, generation: u32| {
            self.plugin.holders_again
                && (self.plugin.holders.iter().any(|h| {
                    h.faction == self.key && h.line.slot == slot && h.line.generation == generation
                }) || self
                    .plugin
                    .death_holders
                    .iter()
                    .any(|d| d.faction == self.key && d.slot == slot && d.generation == generation))
        };
        let mut now = BTreeSet::new();
        for &(id, generation) in &self.ghosts {
            match camp.index(id, generation) {
                None => self.ghosts_gone += 1,
                Some(i)
                    if camp.slots[i].state == crate::engine::SlotState::Body
                        && !held.contains(&(id, generation))
                        && !covered(id, generation) =>
                {
                    now.insert((id, generation));
                }
                // Drawn, its body is back, or a holder's record is on its way.
                Some(_) => {}
            }
        }
        if quiet {
            for s in &camp.slots {
                let pair = (s.id, s.generation);
                if s.state == crate::engine::SlotState::Body
                    && !held.contains(&pair)
                    && !covered(s.id, s.generation)
                {
                    now.insert(pair);
                }
            }
        }
        self.ghosts = now;
    }

    /// Ends a chaos run: answers stop being lost or replayed, the watching player leaves, every
    /// war party gets home, and the camp is censused six times, 0.8 days apart, so that every
    /// ghost has had its three misses over more than two days: afterwards `ghosts` is empty and
    /// the engine's write-offs are `ghosts_gone` exactly.
    pub fn settle(&mut self) {
        self.plugin.lose_answers = 0;
        self.plugin.replay_answers = false;
        self.watch = false;
        for _ in 0..60 {
            if !self.c.parties.iter().any(|(_, p)| p.kind.is_war()) {
                break;
            }
            let to = self.c.now() + TICKS_PER_DAY;
            self.run(to);
        }
        for _ in 0..6 {
            self.camp_out();
            let to = self.c.now() + 3;
            self.run(to);
            self.camp_in();
            let to = self.c.now() + TICKS_PER_DAY * 4 / 5;
            self.run(to);
            self.audit();
        }
    }

    /// The engine, the plugin and the player to `to`.
    pub fn run(&mut self, to: Tick) {
        let mut t = self.c.now();
        while t < to {
            let next = [
                self.c.next_due(),
                self.plugin.next_due(),
                self.watch.then_some(self.next_obs),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(to)
            .clamp(t + 1, to);
            for (at, o) in self.plugin.due(next) {
                self.c.ingest(&mut self.w, &mut *self.infra, &o, at);
            }
            self.c.run_until(&mut self.w, &mut *self.infra, next);
            if self.watch && next >= self.next_obs {
                self.c
                    .observe_player(&mut self.w, PLAYER, REFINERY_POS, next);
                self.next_obs = next + 1_200;
            }
            self.flush(next);
            if self.audits {
                self.track();
            }
            t = next;
        }
    }

    /// What the engine decided, to the plugin.
    pub fn flush(&mut self, at: Tick) {
        let cmds = self.c.take_commands();
        self.cmds.extend(cmds.iter().cloned().map(|c| (at, c)));
        self.plugin.apply(cmds, at);
    }

    /// The camp comes in now: the plugin's census at the next tick.
    pub fn camp_in(&mut self) -> u64 {
        let at = self.c.now() + 1;
        self.plugin.camp_in(self.key, at)
    }

    /// The camp goes out now.
    pub fn camp_out(&mut self) {
        let at = self.c.now() + 1;
        self.plugin.camp_out(self.key, at);
    }

    /// A raid, as an order: `None` when the camp cannot field it.
    pub fn raid(&mut self) -> Option<rtsim_kernel::handle::Handle<crate::engine::Party>> {
        let at = self.c.now();
        let kind = if self
            .c
            .cultures
            .get(self.c.factions[self.f as usize].culture)
            .parties[crate::culture::PartyKind::Raid.index()]
        .is_empty()
        {
            crate::culture::PartyKind::Invasion
        } else {
            crate::culture::PartyKind::Raid
        };
        let p = self.c.order_party(
            &mut self.w,
            self.key,
            kind,
            crate::engine::Target::District(REFINERY, REFINERY_POS),
            None,
            at,
        );
        self.flush(at);
        p
    }

    /// The engine to `to` with its commands held back: what it decided, for a crash test to apply
    /// (or lose) as it likes. The plugin's own queue is not delivered.
    pub fn decide(&mut self, to: Tick) -> Vec<Cmd> {
        self.c
            .run_until(&mut self.w, &mut rtsim_ecology::infra::NoInfrastructure, to);
        self.c.take_commands()
    }

    /// The camp comes in and the census reaches the engine, whose commands are held back (as
    /// [`CampRun::decide`]).
    pub fn census_held(&mut self) -> Vec<Cmd> {
        let at = self.c.now() + 1;
        self.plugin.camp_in(self.key, at);
        for (t, o) in self.plugin.due(at) {
            self.c.ingest(
                &mut self.w,
                &mut rtsim_ecology::infra::NoInfrastructure,
                &o,
                t,
            );
        }
        self.decide(at)
    }

    /// The player kills `n` of the camp's members at the refinery, a signal against the camp: the
    /// feud's grievance (and with it a war party, in time).
    pub fn kill(&mut self, n: f64) {
        let at = self.c.now();
        let sig = crate::engine::Signal {
            faction: Some(self.key),
            kind: crate::culture::SignalKind::Killing,
            amount: n,
            actor: self.player,
            pos: REFINERY_POS,
            district: Some(REFINERY),
        };
        self.c.signal(&mut self.w, sig, at);
        self.flush(at);
    }

    /// A session restart with nothing to replay: [`CampRun::restart_to_replay`], and the replay
    /// over at once, as the host ends it when the journal has caught up (`replay_done`).
    pub fn restart(&mut self) {
        self.restart_to_replay();
        self.c.replay_done();
    }

    /// A session restart that leaves the replay open: the plugin's observations not yet journaled
    /// are lost (the holders' records are journaled again if [`FakeConflict::holders_again`]),
    /// every camp is out, and the engine starts a session, whose replay window (`replaying`) the
    /// caller closes with `replay_done` once it has ingested the records of the last session.
    pub fn restart_to_replay(&mut self) {
        let at = self.c.now();
        // The journal's tail is lost; what the holders carry is not.
        self.plugin.drop_queued();
        // What the plugin held is in its memory, which a session's end clears.
        self.plugin.take_held(None);
        for camp in self.plugin.camps.values_mut() {
            camp.epoch = 0;
        }
        self.c.session_start();
        self.flush(at);
        if self.plugin.holders_again {
            self.plugin.journal_holders(at);
        }
    }

    /// Saves the engine and loads it again in its place; the books must be what they were.
    pub fn round_trip(&mut self) {
        let h = self.c.state_hash();
        let back = Conflict::load(self.c.tun.clone(), self.c.cultures.clone(), &self.c.save())
            .expect("loads");
        if back.state_hash() != h {
            self.failures.push(format!(
                "tick {}: a save and load changed the books",
                self.c.now().0
            ));
        }
        let (replaying, log) = (self.c.replaying, self.c.log_enabled);
        self.c = back;
        self.c.replaying = replaying;
        self.c.log_enabled = log;
    }

    /// Every way the books and the camp's world break §4.9's invariants now (empty when they
    /// hold): the engine's own audit (I1, I2, the cap), I4 (no line of an order is applied
    /// twice), and against the world: I5 always (no two bodies with one slot and generation),
    /// and, with the camp in and nothing in flight, I3 (the tagged bodies are exactly the slots
    /// with a body) and that a body is left behind only while its party musters. I6 (the engine
    /// books only what Java reports) is by construction: the engine never sees the world.
    pub fn check(&mut self) -> Vec<String> {
        let now = self.c.now();
        let mut bad = self.c.camp_audit(self.f, now);
        // I4: no order is applied twice, counted by each line of each reconcile (the op and the
        // line's index) the plugin applied.
        for a in &self.plugin.applied[self.applied_checked..] {
            if !self.applied_keys.insert((a.op, a.index)) {
                bad.push(format!(
                    "I4: line {} of order {:#x} applied twice",
                    a.index, a.op
                ));
            }
        }
        self.applied_checked = self.plugin.applied.len();
        let Some(camp) = self.c.camp(self.f) else {
            return bad;
        };
        let world = self
            .plugin
            .camps
            .get(&self.key)
            .cloned()
            .unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        for b in world.bodies.iter().filter(|b| b.generation != 0) {
            if !seen.insert((b.slot, b.generation)) {
                bad.push(format!(
                    "I5: two bodies carry slot {} generation {:#x}",
                    b.slot, b.generation
                ));
            }
        }
        let settled = camp.is_in()
            && camp.epoch == world.epoch
            && camp.in_flight() == 0
            && !self.plugin.camp_pending(self.key);
        if settled {
            // A home body a census missed (`misses` above 0) may or may not be in the world: it
            // is the crash table's lost body, written off at the third miss.
            let mut want: std::collections::BTreeSet<(u32, u32)> =
                std::collections::BTreeSet::new();
            let mut maybe: std::collections::BTreeSet<(u32, u32)> =
                std::collections::BTreeSet::new();
            for s in &camp.slots {
                use crate::engine::SlotState::{Body, Dead, Left};
                if matches!(s.state, Body | Left | Dead) && s.misses > 0 {
                    maybe.insert((s.id, s.generation));
                } else if matches!(s.state, Body | Left | Dead) {
                    want.insert((s.id, s.generation));
                }
            }
            let held: std::collections::BTreeSet<(u32, u32)> =
                seen.difference(&maybe).copied().collect();
            if want != held {
                bad.push(format!(
                    "I3: slots with a body {want:?}, tagged bodies in the world {seen:?}"
                ));
            }
            for s in &camp.slots {
                if s.state == crate::engine::SlotState::Left
                    && self
                        .c
                        .parties
                        .get(s.party)
                        .is_some_and(|p| p.phase != crate::engine::Phase::Mustering)
                {
                    bad.push(format!(
                        "slot {} is left behind by a party that has set out",
                        s.id
                    ));
                }
            }
        }
        bad
    }

    /// [`CampRun::check`], its findings kept with the tick.
    pub fn audit(&mut self) {
        let t = self.c.now().0;
        for b in self.check() {
            self.failures.push(format!("tick {t}: {b}"));
        }
    }

    /// Random events at random times, `events` of them, each followed by a check: the camp
    /// coming in and out, raids sent, bodies killed, strays put down, answers lost, late, replayed
    /// and stale, session restarts and saves. The books are checked after each (§7 run 10).
    pub fn chaos(&mut self, seed: u64, events: u32) {
        // The plugin's variants follow the seed: a record lost is lost (the ghosts of the crash
        // table's gaps), or holders journal it again, whole or one line to a record.
        self.plugin.holders_again = !seed.is_multiple_of(3);
        self.plugin.split_answers = [0, 0, 1][(seed % 3) as usize];
        let mut rng = StreamKey::new(seed, SYS_CHAOS, self.key).slot(0);
        for _ in 0..events {
            self.chaos_step(&mut rng);
        }
    }

    /// One random event of [`CampRun::chaos`], then the check.
    pub fn chaos_step(&mut self, rng: &mut rtsim_kernel::rng::SlotRng) {
        let now = self.c.now();
        let what: String = match rng.below(16) {
            0..=3 => {
                let span = 1 + rng.below(14 * (TICKS_PER_DAY / 24));
                self.run(now + span);
                format!("run {span}")
            }
            4 | 5 => {
                if self.plugin.camp_is_in(self.key) {
                    "in (already)".into()
                } else {
                    format!("camp in, epoch {:#x}", self.camp_in())
                }
            }
            6 => {
                if self.plugin.camp_is_in(self.key) {
                    self.camp_out();
                    "camp out".into()
                } else {
                    "out (already)".into()
                }
            }
            7 | 8 => {
                // The draw's bound (ledger 6): no more than the smaller of the pool's whole
                // members and the slots it may take, as they stand when the order is given.
                let whole = self.c.pool_at(self.f, now).floor() as u32;
                // Slots the pool gained and the books have not added yet are drawable too.
                let room = self
                    .c
                    .camp(self.f)
                    .map_or(0, |c| c.drawable() + whole.saturating_sub(c.home_count()));
                let p = self.raid();
                if let (Some(p), Some(c)) = (p, self.c.camp(self.f)) {
                    let drawn = c.slots.iter().filter(|s| s.party == p).count() as u32;
                    if drawn > whole.min(room) {
                        self.failures.push(format!(
                            "tick {}: a draw of {drawn} from a pool of {whole} with {room} slots to take",
                            now.0
                        ));
                    }
                }
                format!("raid {}", p.is_some())
            }
            9 | 10 => {
                let bodies: Vec<_> = self
                    .plugin
                    .camps
                    .get(&self.key)
                    .map(|c| c.bodies.clone())
                    .unwrap_or_default();
                if bodies.is_empty() {
                    "kill (none)".into()
                } else {
                    let b = bodies[rng.below(bodies.len() as u64) as usize];
                    self.plugin
                        .camp_kill(self.key, b.uuid, boundary::BY_PLAYER, now + 1);
                    format!("kill slot {} gen {:#x}", b.slot, b.generation)
                }
            }
            11 => {
                let body = self.body;
                self.plugin.camp_stray(self.key, body);
                "stray".into()
            }
            12 => {
                self.plugin.lose_answers += 1;
                "lose the next answer".into()
            }
            13 => {
                self.plugin.replay_answers = !self.plugin.replay_answers;
                format!("replay answers {}", self.plugin.replay_answers)
            }
            14 => {
                self.restart();
                "restart".into()
            }
            _ if self.round_trips => {
                self.round_trip();
                "save and load".into()
            }
            _ => "save (skipped)".into(),
        };
        self.events.push((now, what));
        // Half the time the answers come before the check, half not.
        if rng.below(2) == 0 {
            let to = self.c.now() + 3;
            self.run(to);
        }
        if self.audits {
            self.audit();
        }
    }
}
