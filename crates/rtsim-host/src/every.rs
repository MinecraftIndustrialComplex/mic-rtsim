// SPDX-License-Identifier: AGPL-3.0-or-later
//! The every-kind world (EQUILIBRIUM.md §4's S11 row; docs/overnight-choices-equilibrium.md EQC401
//! to EQC404, EQC470 onward): one overworld map with sites of every census kind, founded through
//! the fake Java's own records and run in the shipped engine, where every module shares one clock
//! (`shared::run_rounds`). The long run, the together kill-alls and the who-is-alive soak all run
//! on it.
//!
//! - [`populate`] founds it: the 56 herd ranges of S4's grid (HABITAT painted per region, so the
//!   world model founds a herd a range), the five lairs of `rtsim_ecology::synth::lairs` each with
//!   one roosting body, nine villages on the host's grid (three of them Ribbit) counted alive by a
//!   complete census, six sea ports and two havens, three pool camps and two outposts, three
//!   Frostmaw dens and three Umvuthana groves of K 5, 7 and 9, and two oil nests with an endless
//!   well beside each.
//! - [`world`] makes it on disk as a server would leave it; [`Every::open`] forks it and runs it in
//!   the headless host at max on [`Policy::Every`]: nobody online, villages refilled, nests
//!   placed, the conflict's commands played to its fake plugin.
//! - [`Every`] keeps each village's real residents, as the bridge's census counts them, and sends
//!   the engine a complete census after every change (twice after a fall: the engine lowers a
//!   village's count only on two complete readings in a row).
//!
//! Positions are fixed, so a seed's world is the same map in every build; the seed is the engine's
//! (`Options::seed`, EQC475).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rtsim_conflict::boundary as cb;
use rtsim_ecology::boundary as db;
use rtsim_ecology::species::Gender;
use rtsim_industry::ids::{ChunkPos, DistrictId, Good};
use rtsim_industry::obs::{DepositPluginId, DepositSample, DistrictDecl, Obs as IObs, SweepEnd, Timed};
use rtsim_jni::engine::Engine;
use rtsim_jni::orders::Order;
use rtsim_jni::wire;
use rtsim_kernel::census::KindCensus;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::Durability;
use rtsim_kernel::renewal::Pressure;
use rtsim_kernel::rng::{SlotRng, StreamKey, SystemId};
use rtsim_ships::boundary as sb;

use crate::fork;
use crate::java::{self, DICT_GOOD, DICT_NAME, Java, OVERWORLD, Policy, WorldSave};
use crate::nbt::{self, Tag};
use crate::run::{Control, DAY, Host, Options, Speed, Until};
use crate::save::{self, Journal};

/// The player whose kills the suite books (`every::PLAYER`, EQC403).
pub const PLAYER: (u64, u64) = (0x5E11_0000_0000_0001, 0x5E11_0000_0000_0011);

/// The census kinds the every-kind world holds, in the engine's order (EQC436).
pub const KINDS: [&str; 11] = [
    "village.human",
    "village.ribbit",
    "dragon.lair",
    "oil.nest",
    "herd.range",
    "ship.haven",
    "ship.port",
    "conflict.camp",
    "conflict.den",
    "conflict.grove",
    "conflict.umvuthi",
];

/// The herd ranges: S4's grid, regions x -3 to 4 and z -3 to 3 (EQC18).
pub const RANGES_X: std::ops::RangeInclusive<i32> = -3..=4;
pub const RANGES_Z: std::ops::RangeInclusive<i32> = -3..=3;

/// The first village's key; village `i` is `VILLAGE + i`.
pub const VILLAGE: u64 = 0x0001_0000_0000_0100;
/// The villages of Ribbit culture (numbers 2, 5 and 8).
pub const RIBBITS: [u64; 3] = [2, 5, 8];
/// The human villages' beds and the Ribbit villages' residents (their K).
pub const HUMAN_BEDS: u32 = 15;
pub const RIBBIT_RESIDENTS: u32 = 12;

const STRUCTURES: [&str; 5] = [
    "minecraft:village_plains",
    "minecraft:village_taiga",
    "minecraft:village_desert",
    "minecraft:village_savanna",
    "minecraft:village_snowy",
];

/// The oil chunks: two nest cells, (12, 12) and (-19, 7), as the CLI's ecology scenario.
pub const OIL_CHUNKS: [(i32, i32); 4] = [(100, 100), (101, 100), (100, 101), (-150, 60)];
/// The wells, one beside each nest cell (chunk coordinates).
pub const WELLS: [(i32, i32); 2] = [(100, 102), (-150, 62)];

/// One village of the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Village {
    pub key: u64,
    pub ribbit: bool,
    pub at: (i32, i32),
    /// The census's beds (a human village's K; a Ribbit village's K is its largest complete
    /// reading).
    pub beds: u32,
}

/// One camp of the world: its faction key, culture and block position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camp {
    pub key: u64,
    pub culture: &'static str,
    pub at: (i32, i32),
    /// The home bodies it was declared with (a grove's K; a den's Frostmaw).
    pub bodies: u16,
}

/// What [`populate`] founded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Founded {
    pub ranges: Vec<(i32, i32)>,
    /// Lair keys and their homes.
    pub lairs: Vec<(u64, (i32, i32, i32))>,
    pub villages: Vec<Village>,
    /// The ports (their villages' keys) and havens (keys and docks).
    pub ports: Vec<u64>,
    pub havens: Vec<(u64, (i32, i32, i32))>,
    /// Pool camps (goblin, bandit, raider), outposts (raider camps a vanilla outpost reports),
    /// dens and groves.
    pub camps: Vec<Camp>,
    pub outposts: Vec<Camp>,
    pub dens: Vec<Camp>,
    pub groves: Vec<Camp>,
    /// The oil chunks reported, and the wells' chunks.
    pub oil: Vec<(i32, i32)>,
    pub wells: Vec<(i32, i32)>,
    /// The fake plugin's nests when the world was saved (worms and last serial): Minecraft's
    /// state, which the fork's Java takes over.
    pub nest_worms: BTreeMap<u64, (u32, u32)>,
}

impl Founded {
    /// The world's site count of a census kind (EQC436): what every sample must hold exactly.
    pub fn sites(&self, kind: &str) -> u32 {
        let n = match kind {
            "village.human" => self.villages.iter().filter(|v| !v.ribbit).count(),
            "village.ribbit" => self.villages.iter().filter(|v| v.ribbit).count(),
            "dragon.lair" => self.lairs.len(),
            "oil.nest" => 2,
            "herd.range" => self.ranges.len(),
            "ship.haven" => self.havens.len(),
            "ship.port" => self.ports.len(),
            "conflict.camp" => self.camps.len() + self.outposts.len(),
            "conflict.den" => self.dens.len(),
            "conflict.grove" | "conflict.umvuthi" => self.groves.len(),
            _ => 0,
        };
        n as u32
    }

    pub fn village(&self, key: u64) -> Option<&Village> {
        self.villages.iter().find(|v| v.key == key)
    }
}

/// The sea the ports and havens need (blocks, EQC476): a ladder of channels one 32-block water
/// cell wide (the ships' mask cell: a cell is water only when all its points are) that passes
/// within 2 cells of every port's dock: east-west at z 64, 544 and 1,056, north-south at x 64 and
/// 544, between the village grid's rows and columns. The rest of every region is land.
fn is_channel(x: i32, z: i32) -> bool {
    let band = |v: i32, lo: i32| (lo..lo + 32).contains(&v);
    let across = (64..576).contains(&x) && [64, 544, 1_056].iter().any(|&lo| band(z, lo));
    let down = (64..1_088).contains(&z) && [64, 544].iter().any(|&lo| band(x, lo));
    across || down
}

/// Paints region (`rx`, `rz`) as habitat: 64 cells of one biome (land but for the channels of
/// [`is_channel`]), sent as the core's sampler sends them (latest wins, as the buffers have room).
fn paint_region(j: &mut Java, p: &mut rtsim_space::synth::Painter, rx: i32, rz: i32) {
    use rtsim_space::synth::{land, sea};
    let biomes = ["minecraft:plains", "minecraft:savanna", "minecraft:forest"];
    let biome = biomes[(rx + 3 + 2 * (rz + 3)).rem_euclid(biomes.len() as i32) as usize];
    let per = (rtsim_travel::legs::REGION_BLOCKS / 64) as i32;
    for cz in rz * per..(rz + 1) * per {
        for cx in rx * per..(rx + 1) * per {
            let r = p.record(
                0,
                cx,
                cz,
                &|x, z| if is_channel(x, z) { sea() } else { land(biome) },
                &[],
            );
            while !j.habitat(&r, &|i| p.biome_name(i).to_string(), &|i| {
                p.body_name(i).to_string()
            }) {
                j.tick();
            }
        }
    }
}

/// Founds every kind in `j`'s world (EQC402) and returns what it founded. The camps' bodies stand
/// in the fake conflict plugin's camp worlds, as the jars place them.
pub fn populate(j: &mut Java) -> Founded {
    let mut f = Founded::default();
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    // (1) Herd ranges.
    let mut painter = rtsim_space::synth::Painter::default();
    for rz in RANGES_Z {
        for rx in RANGES_X {
            paint_region(j, &mut painter, rx, rz);
            f.ranges.push((rx, rz));
        }
    }
    j.idle();
    // (2) Lairs, each with one roosting body.
    for (i, l) in rtsim_ecology::synth::lairs().iter().enumerate() {
        let home = (l.x, l.y, l.z);
        j.dragon_obs(&db::Obs::LairFound(db::LairFound {
            lair: l.key,
            late: false,
            placed: false,
            dimension: dim,
            species: l.species,
            kind: l.kind,
            min: (home.0 - 30, home.1 - 40, home.2 - 30),
            max: (home.0 + 30, home.1 + 40, home.2 + 30),
            home,
            row: Default::default(),
        }));
        j.dragon_obs(&db::Obs::LairCensus(db::LairCensus {
            lair: l.key,
            epoch: 1,
            dimension: dim,
            flags: 0,
            bodies: vec![db::CensusBody {
                entity: (0xD1, 0x100 + i as u64),
                agent: 0,
                species: l.species,
                flags: db::BODY_HAS_HOME,
                age_ticks: 110 * 24_000,
                hunger: 60,
                health: 0.0,
                gender: Gender::Female,
                variant: 1,
                pos: (
                    f64::from(home.0),
                    f64::from(home.1),
                    f64::from(home.2),
                ),
            }],
        }));
        f.lairs.push((l.key, home));
    }
    // (3) Villages: nine on a 500-block grid, numbers 2, 5 and 8 Ribbit.
    for i in 0..9u64 {
        let at = ((i % 3) as i32 * 500, (i / 3) as i32 * 500);
        let ribbit = RIBBITS.contains(&i);
        let key = VILLAGE + i;
        if ribbit {
            j.adopt(key, "ribbits:village", "ribbit", 40, at.0, at.1);
        } else {
            j.adopt(
                key,
                STRUCTURES[i as usize % STRUCTURES.len()],
                "human",
                90,
                at.0,
                at.1,
            );
        }
        f.villages.push(Village {
            key,
            ribbit,
            at,
            beds: if ribbit { RIBBIT_RESIDENTS } else { HUMAN_BEDS },
        });
    }
    j.idle();
    // (4) The human villages are sea ports; two havens.
    for v in f.villages.iter().filter(|v| !v.ribbit) {
        j.ship_obs(&sb::Obs::PortFound(sb::PortFound {
            village: v.key,
            medium: rtsim_travel::Medium::Sea,
            dimension: dim,
            structure: 0,
            dock: (v.at.0 + 8, 63, v.at.1 + 8),
            berths: 4,
        }));
        f.ports.push(v.key);
    }
    for (haven, dock) in [(0x42u64, (250, 63, 250)), (0x43, (750, 63, 750))] {
        j.ship_obs(&sb::Obs::HavenFound(sb::HavenFound {
            haven,
            media: 1,
            dimension: dim,
            structure: 0,
            dock,
        }));
        f.havens.push((haven, dock));
    }
    // (5) Pool camps and (6) outposts (raider camps, as core's `Outposts` reports a vanilla one:
    // the outpost's structure, no bodies, not late).
    let pool = [
        (0xCA_0001u64, "goblin", "goblinstyranny:goblin_camp", (750, 250)),
        (0xCA_0002, "bandit", "mic_rtsim:conflict/bandit_camp", (-750, 250)),
        (0xCA_0003, "raider", "minecraft:pillager_outpost", (250, -750)),
    ];
    for (key, culture, structure, at) in pool {
        found_camp(j, key, structure, at, 0, 0);
        f.camps.push(Camp {
            key,
            culture,
            at,
            bodies: 0,
        });
    }
    for (key, at) in [(0xCA_0011u64, (1750, 750)), (0xCA_0012, (-1250, -1250))] {
        found_camp(j, key, "minecraft:pillager_outpost", at, 0, 0);
        f.outposts.push(Camp {
            key,
            culture: "raider",
            at,
            bodies: 0,
        });
    }
    // (7) Frostmaw dens (one Frostmaw, its crystal on it) and Umvuthana groves of K 5, 7 and 9,
    // their bodies standing in the plugin's camp worlds as the jar places them.
    for (k, at) in [(-1250, 1250), (1750, -1250), (2250, 1250)]
        .into_iter()
        .enumerate()
    {
        let key = 0xDE_0001 + k as u64;
        found_camp(
            j,
            key,
            "mowziesmobs:frostmaw_spawn",
            at,
            1,
            (cb::CENSUS_CHIEF | cb::CENSUS_CRYSTAL) as u8,
        );
        f.dens.push(Camp {
            key,
            culture: "frostmaw",
            at,
            bodies: 1,
        });
    }
    for (k, (warriors, at)) in [(5u16, (-500, 500)), (7, (1500, 500)), (9, (500, 1500))]
        .into_iter()
        .enumerate()
    {
        let key = 0x6E_0001 + k as u64;
        found_camp(j, key, "mowziesmobs:umvuthana_grove", at, warriors, 0);
        f.groves.push(Camp {
            key,
            culture: "umvuthana",
            at,
            bodies: warriors,
        });
    }
    // (8) Oil nests: the chunks reported, and an endless well beside each nest cell.
    j.dragon_obs(&db::Obs::OilChunks(db::OilChunks {
        dimension: dim,
        source: db::OIL_SOURCE_CDG,
        chunks: OIL_CHUNKS
            .iter()
            .map(|&(cx, cz)| db::OilChunkRec {
                cx,
                cz,
                amount: 1,
                flags: 0,
            })
            .collect(),
    }));
    camp_bodies(j, &f);
    f.oil = OIL_CHUNKS.to_vec();
    let oil = Good(j.dict_id(DICT_GOOD, "createdieselgenerators:crude_oil"));
    for (i, &(cx, cz)) in WELLS.iter().enumerate() {
        well(j, DistrictId(0x0E11_0000 + i as u64), dim, oil, (cx, cz));
        f.wells.push((cx, cz));
    }
    j.idle();
    // Every village counted alive (S6's FB-1 (b)): a complete census of its residents.
    for v in &f.villages {
        j.village_census(v.key, v.beds, v.beds, true);
    }
    j.idle();
    j.idle();
    f
}

/// The dens' and groves' bodies standing in the fake conflict plugin's camp worlds, as the jars
/// place them: a den's Frostmaw (its chief, crystal on it), a grove's warriors. Minecraft's state:
/// the fork's Java stands them there again.
pub fn camp_bodies(j: &mut Java, f: &Founded) {
    let frostmaw = j.dict_id(wire::DICT_BODY, "mic_rtsim:conflict/frostmaw/frostmaw");
    for d in &f.dens {
        j.conflict_plugin.camp_world(d.key).bodies.clear();
        j.conflict_plugin.camp_stray(d.key, frostmaw);
        if let Some(b) = j.conflict_plugin.camp_world(d.key).bodies.first_mut() {
            b.chief = true;
            b.crystal = true;
        }
    }
    let warrior = j.dict_id(wire::DICT_BODY, "mic_rtsim:conflict/umvuthana/warrior");
    for g in &f.groves {
        j.conflict_plugin.camp_world(g.key).bodies.clear();
        for _ in 0..g.bodies {
            j.conflict_plugin.camp_stray(g.key, warrior);
        }
    }
}

fn found_camp(j: &mut Java, key: u64, structure: &str, at: (i32, i32), bodies: u16, census: u8) {
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let structure = j.dict_id(wire::DICT_STRUCTURE, structure);
    j.conflict_obs(&cb::Obs::FactionFound(cb::FactionFound {
        faction: key,
        flags: 0,
        dimension: dim,
        structure,
        pos: (at.0, 64, at.1),
        home_bodies: bodies,
        census,
        row: Default::default(),
    }));
}

/// An industry district with one chunk, `chunk`, whose last sweep holds an endless crude oil
/// deposit: a well (`rtsim_ecology::infra::IndustryLink::wells`, R19).
fn well(j: &mut Java, district: DistrictId, dim: u32, oil: Good, chunk: (i32, i32)) {
    let at = Tick(j.game_time.max(0) as u64);
    let name = j.dict_id(DICT_NAME, &format!("well {}", district.0));
    for obs in [
        IObs::District(DistrictDecl {
            district,
            revision: 1,
            dim,
            name,
            not_learnable: None,
            inventory_count: 0,
        }),
        IObs::DistrictChunks {
            district,
            revision: 1,
            part: 0,
            chunks: vec![chunk],
        },
        IObs::DepositSample(DepositSample {
            district,
            good: oil,
            plugin: DepositPluginId::DieselOil,
            chunk: ChunkPos {
                dim,
                x: chunk.0,
                z: chunk.1,
            },
            remaining: 0,
            infinite: true,
            sweep: 1,
        }),
        IObs::SweepEnd(SweepEnd {
            district,
            sweep: 1,
            records: 1,
            inventories: 0,
            live: false,
            creative: false,
            overstressed: false,
        }),
    ] {
        j.industry_obs(&Timed { at, obs });
    }
}

/// A scratch folder for one world.
pub fn scratch(tag: &str) -> PathBuf {
    let d = java::tmpdir(&format!("every-{tag}"));
    fs::create_dir_all(&d).expect("a scratch folder");
    d
}

fn write_level(world: &Path, time: i64) {
    let root = Tag::Compound(vec![(
        b"Data".to_vec(),
        Tag::Compound(vec![
            (b"Time".to_vec(), Tag::Long(time)),
            (b"DayTime".to_vec(), Tag::Long(time)),
            (b"LevelName".to_vec(), Tag::string("every-kind")),
        ]),
    )]);
    save::write_atomic(&world.join(save::LEVEL), &nbt::write_gz(b"", &root))
        .expect("level.dat written");
}

/// The every-kind world made in folder `world` on seed `seed` as a server would leave it: founded,
/// `player_day` with a player in village 0 for the first game day (the soak's, EQC404), saved, and
/// the engine stopped. Returns what was founded.
pub fn make(world: &Path, seed: u64, player_day: bool) -> Founded {
    let e = java::engine_with(&world.join(save::PAGES), 1 << 20, |c| {
        c.seed = Some(seed);
        c.industry_seed = Some(seed);
        c.test = false;
    });
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = save::ALL_MODULES;
    j.policy = Policy::Every;
    j.conflict_plugin = rtsim_conflict::synth::FakeConflict::new(PLAYER);
    let mut f = populate(&mut j);
    if player_day {
        let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
        let v = f.villages[0].at;
        j.players = vec![(PLAYER, dim, [f64::from(v.0) + 10.0, 64.0, f64::from(v.1)])];
        let end = j.game_time + DAY;
        while j.game_time < end {
            j.game_time += 998;
            j.idle();
        }
        j.players.clear();
    }
    j.idle();
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
    let through = j.next_obs - 1;
    j.e.request_save(through);
    j.written_through = Some(through);
    j.world_saved(through);
    j.run_until("the world's save", |j| j.e.durable_obs_seq() >= through);
    Journal {
        save: j.world_save(),
        data_version: Some(3955),
    }
    .write(world)
    .expect("the journal written");
    write_level(world, j.game_time);
    f.nest_worms = j.nest_worms.clone();
    j.crash();
    f
}

/// The every-kind world forked and running in the headless host, with the villages' residents the
/// suite keeps.
pub struct Every {
    pub host: Host,
    pub founded: Founded,
    pub seed: u64,
    pub opts: Options,
    ctl: &'static Control,
    /// Each village's residents as the bridge's census would count them.
    pub residents: BTreeMap<u64, u32>,
    /// Villages whose count fell at the last census: read again at the next hand-over.
    second: Vec<u64>,
    /// What the kills since the last day's end should have booked, and what was ([`Teeth`]).
    owed: Vec<Owed>,
    pub teeth: Teeth,
    /// The last op id a SUNK record took.
    op: u64,
    /// The folders to delete at the end.
    scratch: Vec<PathBuf>,
}

impl Every {
    /// Makes the every-kind world on `seed` (`player_day` as [`make`]), forks it and opens it in
    /// the host at max with `durability`, on [`Policy::Every`].
    pub fn open(tag: &str, seed: u64, player_day: bool, durability: Durability) -> Every {
        Every::open_with(tag, seed, player_day, durability, Policy::Every, false)
    }

    /// [`Every::open`] in TEST mode, so the kills' orders (41, 43, 44) are taken: the long run's
    /// and the together kill-alls' world.
    pub fn open_test(tag: &str, seed: u64) -> Every {
        Every::open_with(tag, seed, false, Durability::NoSync, Policy::Every, true)
    }

    /// [`Every::open`] on another policy (the soak's is [`Policy::Refuse`], EQC404), and in TEST
    /// mode or not.
    pub fn open_with(
        tag: &str,
        seed: u64,
        player_day: bool,
        durability: Durability,
        policy: Policy,
        test: bool,
    ) -> Every {
        let world = scratch(&format!("{tag}-{seed}"));
        let founded = make(&world, seed, player_day);
        let parent = scratch(&format!("{tag}-{seed}-fork"));
        let dir = parent.join("fork");
        fork::export(&world, &dir).expect("the fork");
        let opts = Options {
            speed: Speed::Max,
            durability,
            seed: Some(seed),
            test,
            ..Options::default()
        };
        let mut host = Host::open(&dir, &opts).expect("the host opens the fork");
        host.j.policy = policy;
        host.j.conflict_plugin = rtsim_conflict::synth::FakeConflict::new(PLAYER);
        host.j.nest_worms = founded.nest_worms.clone();
        camp_bodies(&mut host.j, &founded);
        let residents = founded.villages.iter().map(|v| (v.key, v.beds)).collect();
        Every {
            host,
            founded,
            seed,
            opts,
            ctl: Box::leak(Box::new(Control::new())),
            residents,
            second: Vec::new(),
            owed: Vec::new(),
            teeth: Teeth::default(),
            op: 0,
            scratch: vec![world, parent],
        }
    }

    /// The fork's pages folder.
    pub fn pages(&self) -> PathBuf {
        self.host.dir.join(save::PAGES)
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.host.e
    }

    pub fn java(&mut self) -> &mut Java {
        &mut self.host.j
    }

    /// The game day the host has handed over through, counted from the fork's start.
    pub fn day(&self) -> u64 {
        ((self.host.last_handed - self.host.start_time).max(0) / DAY) as u64
    }

    /// Runs the host until it has handed over game day `day` (from the fork's start), answering
    /// the villages' residents as it goes: each day's new residents are counted, and a village
    /// whose count changed gets a complete census.
    pub fn run_to(&mut self, day: u64) {
        while self.day() < day {
            let next = self.day() + 1;
            let o = Options {
                until: Some(Until::At(self.host.start_time + next as i64 * DAY)),
                ..self.opts.clone()
            };
            self.host
                .run(&o, self.ctl, Duration::from_secs(3600), |_| {})
                .unwrap_or_else(|e| panic!("seed {}: day {next}: {e}", self.seed));
            self.villages();
            self.check_owed();
        }
    }

    /// Runs the host to game time `t` (from the fork's start), within a day: a camp visit's ticks.
    fn run_ticks_to(&mut self, t: i64) {
        let o = Options {
            until: Some(Until::At(self.host.start_time + t)),
            ..self.opts.clone()
        };
        self.host
            .run(&o, self.ctl, Duration::from_secs(3600), |_| {})
            .unwrap_or_else(|e| panic!("seed {}: tick {t}: {e}", self.seed));
    }

    /// The day's village censuses: residents the engine settled join their villages; every
    /// village that changed (or fell at the last census) is counted complete.
    fn villages(&mut self) {
        let mut changed: Vec<u64> = std::mem::take(&mut self.second);
        for v in std::mem::take(&mut self.host.j.settled) {
            if let Some(n) = self.residents.get_mut(&v) {
                *n += 1;
                changed.push(v);
            }
        }
        changed.sort_unstable();
        changed.dedup();
        for v in changed {
            self.census_village(v);
        }
    }

    /// A complete census of village `v` as it stands now.
    pub fn census_village(&mut self, v: u64) {
        let n = self.residents[&v];
        let beds = self.founded.village(v).map_or(n, |x| x.beds.max(n));
        self.host.j.village_census(v, n, beds, true);
    }

    /// The player kills `n` of village `v`'s residents (kind-8 reports, as the bridge's kill
    /// credit sends them), and the village is counted complete now and again at the next
    /// hand-over. How many died. Booked: the village's pressure, 1/K a resident.
    pub fn kill_residents(&mut self, v: u64, n: u32) -> u32 {
        let have = self.residents.get(&v).copied().unwrap_or(0);
        let n = n.min(have);
        let kind = if self.founded.village(v).is_some_and(|x| x.ribbit) {
            "village.ribbit"
        } else {
            "village.human"
        };
        if n > 0 {
            let day = self.obs_day();
            if let Some((p, h, k)) = self.pressure(Site::Village(v)) {
                self.owe_pressure(kind, Site::Village(v), day, p.now(day, h), n as f32 / k);
            }
        }
        for _ in 0..n {
            self.host.j.resident_killed(v, true);
        }
        if n > 0 {
            self.residents.insert(v, have - n);
            self.census_village(v);
            self.second.push(v);
        }
        self.teeth.killed(kind, u64::from(n));
        n
    }

    // ---- the kills (EQC403, EQC405) ------------------------------------------------------------

    /// The engine day a record journaled now is applied on (its hand-over's).
    fn obs_day(&self) -> u32 {
        (self.host.j.game_time.max(0) as u64 / TICKS_PER_DAY) as u32
    }

    fn dim(&mut self) -> u32 {
        self.host.j.dict_id(wire::DICT_DIMENSION, OVERWORLD)
    }

    fn order(&self, o: Order) {
        self.host
            .e
            .order(vec![o])
            .unwrap_or_else(|e| panic!("an order: {e} (the world needs TEST mode)"));
    }

    /// Order 43: every dragon at lair `key` killed by [`PLAYER`]. How many it held. Booked: the
    /// dragons' player losses.
    pub fn slay_lair(&mut self, key: u64) -> u64 {
        let (n, before) = self.host.e.with_dragons(|d, _| {
            let n = d
                .lair_of_key(key)
                .map_or(0, |l| d.lairs[l as usize].occupants.len() as u64);
            (n, d.stats.player_losses)
        });
        if n > 0 {
            self.order(Order::DragonSlay {
                lair: key,
                player: PLAYER,
            });
            self.owe_count("dragon.lair", Counter::DragonLosses, before, n);
        }
        self.teeth.killed("dragon.lair", n);
        n
    }

    /// The latent animals of the herds of range `region`, and the player kills they booked.
    fn range(&mut self, region: (i32, i32)) -> (u64, u64) {
        let dim = self.dim() as u16;
        self.host.e.with_herds(|h, _| {
            let now = h.now();
            h.range_herds((dim, region.0, region.1))
                .into_iter()
                .fold((0, 0), |(n, k), id| {
                    (
                        n + h.count(id, now),
                        k + h.herd(id).map_or(0, |x| x.ledger.killed),
                    )
                })
        })
    }

    /// Order 44: up to `n` latent animals of range `region` killed by [`PLAYER`]. How many.
    /// Booked: the range's herds' player kills.
    pub fn cull_range(&mut self, region: (i32, i32), n: u64) -> u64 {
        let (have, before) = self.range(region);
        let n = n.min(have).min(u64::from(u32::MAX));
        if n > 0 {
            let dimension = self.dim();
            self.order(Order::HerdCull {
                dimension,
                region,
                n: n as u32,
                player: PLAYER,
            });
            self.owe_count("herd.range", Counter::RangeKilled(region), before, n);
        }
        self.teeth.killed("herd.range", n);
        n
    }

    /// A quarter of range `region`'s latent animals, rounded down (EQC412).
    pub fn quarter_of_range(&mut self, region: (i32, i32)) -> u64 {
        self.range(region).0 / 4
    }

    /// The engine's oil nests: (id, chunk, worms booked lost to a player).
    pub fn nests(&self) -> Vec<(u64, (i32, i32), u32)> {
        self.host.e.with_dragons(|d, _| {
            d.nests
                .ids()
                .into_iter()
                .filter_map(|id| d.nests.get(id).map(|n| (id, n.chunk, n.lost_player)))
                .collect()
        })
    }

    /// NEST_LOSS, cause Player: every worm of nest `id` killed by [`PLAYER`]. How many. Booked:
    /// the nest's worms lost to a player.
    pub fn kill_nest(&mut self, id: u64) -> u64 {
        let before = self.nests().into_iter().find(|n| n.0 == id).map_or(0, |n| n.2);
        let n = u64::from(self.host.j.kill_worms(id, u32::MAX, PLAYER));
        if n > 0 {
            self.owe_count("oil.nest", Counter::NestLost(id), u64::from(before), n);
        }
        self.teeth.killed("oil.nest", n);
        n
    }

    /// A camp's warriors at home (the pool's whole ones) and away, its pressure on `day` and its
    /// cap.
    fn camp(&self, key: u64, day: u32) -> Option<CampNow> {
        self.host.e.with_conflict(|c, _| {
            let f = c.faction_of_key(key)?;
            let now = c.now();
            Some(CampNow {
                home: c.pool_at(f, now).max(0.0).floor() as u32,
                away: c.factions[f as usize].away,
                pressure: c.pressure_now(f, Tick::from_days(u64::from(day))),
                cap: c.factions[f as usize].cap,
            })
        })
    }

    /// Order 41: pool camp `key`'s warriors killed by a player: every one at home, and with `all`
    /// the war parties away too (a kill-all, as `camp_run`'s). How many. Booked: the camp's
    /// strain, 1/cap a warrior.
    pub fn slay_camp(&mut self, key: u64, all: bool) -> u64 {
        let day = self
            .host
            .e
            .with_conflict(|c, _| (c.now().0 / TICKS_PER_DAY) as u32);
        let Some(c) = self.camp(key, day) else {
            return 0;
        };
        let n = c.home + if all { c.away } else { 0 };
        if n == 0 {
            return 0;
        }
        self.order(Order::ConflictSlay {
            faction: key,
            n: if all { u32::MAX } else { n },
            pressure: 0.0,
        });
        self.owe_pressure(
            "conflict.camp",
            Site::Camp(key),
            day,
            c.pressure,
            n as f32 / c.cap.max(1) as f32,
        );
        self.teeth.killed("conflict.camp", u64::from(n));
        u64::from(n)
    }

    /// A player's visit to den or grove `key` through the fake conflict plugin, as S9's
    /// `mowzie_visit`: the camp comes in, every member at home (`chief`: the chief's body alone)
    /// is killed by the player, and the camp goes out. How many died. Booked: the camp's pressure,
    /// 1/K a member (the chief's own pressure for the chief, which the census reads).
    pub fn visit_camp(&mut self, key: u64, kind: &'static str, chief: bool) -> u64 {
        use rtsim_conflict::boundary as cb;
        use rtsim_conflict::engine::CHIEF_SLOT;
        let t0 = self.host.j.game_time - self.host.start_time;
        let at = Tick(self.host.j.game_time.max(0) as u64);
        self.host.j.conflict_plugin.camp_in(key, at);
        self.run_ticks_to(t0 + 10);
        let ids: Vec<_> = self
            .host
            .j
            .conflict_plugin
            .camp_world(key)
            .bodies
            .iter()
            .filter(|b| b.generation != 0 && (b.slot == CHIEF_SLOT) == chief)
            .map(|b| b.uuid)
            .collect();
        let day = self.obs_day();
        let before = self.camp(key, day);
        let at = Tick(self.host.j.game_time.max(0) as u64);
        // TODO(S9): the chief's own pressure (`Camp::chief_pressure`) for the Umvuthi's teeth.
        let mut killed = 0u64;
        for u in ids {
            killed += u64::from(
                self.host
                    .j
                    .conflict_plugin
                    .camp_kill(key, u, cb::BY_PLAYER, at),
            );
        }
        if killed > 0
            && !chief
            && let Some(c) = before
        {
            self.owe_pressure(
                kind,
                Site::Camp(key),
                day,
                c.pressure,
                killed as f32 / c.cap.max(1) as f32,
            );
        }
        let t = self.host.j.game_time - self.host.start_time;
        self.run_ticks_to(t + 5);
        let at = Tick(self.host.j.game_time.max(0) as u64);
        self.host.j.conflict_plugin.camp_out(key, at);
        let t = self.host.j.game_time - self.host.start_time;
        self.run_ticks_to(t + 3);
        self.teeth.killed(kind, killed);
        killed
    }

    /// Haven `key`'s active pirate ships: (agent, epoch).
    fn active_ships(&self, key: u64) -> Vec<(u64, u32)> {
        self.host.e.with_ships(|s, w| {
            let Some(h) = s.havens.iter().find(|h| w.nodes.get(h.node).is_some_and(|n| n.key == key))
            else {
                return Vec::new();
            };
            h.ships
                .iter()
                .filter_map(|&id| {
                    s.ship(id)
                        .filter(|x| x.is_active())
                        .map(|x| (rtsim_ships::engine::Ship::agent(id), x.epoch))
                })
                .collect()
        })
    }

    /// SUNK by a player for every active pirate of haven `key` (agent and epoch read between
    /// steps, R18). How many. Booked: the haven's refit pressure, 1/slots a ship.
    pub fn sink_haven(&mut self, key: u64) -> u64 {
        let ships = self.active_ships(key);
        if ships.is_empty() {
            return 0;
        }
        let day = self.obs_day();
        if let Some((p, h, slots)) = self.pressure(Site::Haven(key)) {
            self.owe_pressure(
                "ship.haven",
                Site::Haven(key),
                day,
                p.now(day, h),
                ships.len() as f32 / slots,
            );
        }
        let dimension = self.dim();
        let dock = self
            .founded
            .havens
            .iter()
            .find(|x| x.0 == key)
            .map_or((0, 63, 0), |x| x.1);
        for (agent, epoch) in &ships {
            self.op += 1;
            self.host.j.ship_obs(&sb::Obs::Sunk(sb::Sunk {
                agent: *agent,
                epoch: *epoch,
                by: sb::By::Player,
                dimension,
                player: PLAYER,
                ship: 0,
                op: 1 << 62 | self.op,
                pos: (f64::from(dock.0), f64::from(dock.1), f64::from(dock.2)),
            }));
        }
        self.teeth.killed("ship.haven", ships.len() as u64);
        ships.len() as u64
    }

    // ---- every site as the engine holds it -----------------------------------------------------

    /// Every site of the world now (between steps): what the together kill-alls and the long
    /// run's rule bounds read, site by site.
    pub fn sites(&self) -> Vec<SiteNow> {
        let f = &self.founded;
        let mut out = Vec::new();
        self.host.e.with_dragons(|d, w| {
            for v in &f.villages {
                let row = w
                    .nodes
                    .iter()
                    .position(|n| n.key == v.key)
                    .and_then(|i| w.village_row(i));
                out.push(SiteNow {
                    kind: if v.ribbit {
                        "village.ribbit"
                    } else {
                        "village.human"
                    },
                    key: v.key,
                    members: row.map_or(0, |r| u64::from(r.members)),
                    k: row.map_or(u64::from(v.beds), |r| u64::from(r.k)),
                    broken: false,
                });
            }
            for l in d.lairs.iter() {
                out.push(SiteNow {
                    kind: "dragon.lair",
                    key: l.key,
                    members: l.occupants.len() as u64,
                    k: 1,
                    broken: false,
                });
            }
            for id in d.nests.ids() {
                if let Some(n) = d.nests.get(id) {
                    out.push(SiteNow {
                        kind: "oil.nest",
                        key: id,
                        members: u64::from(n.worms),
                        k: u64::from(d.tun.oil.nest.k),
                        broken: n.state == rtsim_ecology::nest::NestState::Broken,
                    });
                }
            }
        });
        self.host.e.with_herds(|h, _| {
            let now = h.now();
            let mut by: BTreeMap<(i32, i32), (u64, u64)> =
                f.ranges.iter().map(|&r| (r, (0, 0))).collect();
            for (id, x) in h.herds.iter() {
                if let Some(e) = by.get_mut(&(x.range.1, x.range.2)) {
                    e.0 += h.count(id, now);
                    e.1 += x.k;
                }
            }
            for ((rx, rz), (n, k)) in by {
                out.push(SiteNow {
                    kind: "herd.range",
                    key: ((rx as u32 as u64) << 32) | u64::from(rz as u32),
                    members: n,
                    k,
                    broken: false,
                });
            }
        });
        self.host.e.with_ships(|s, w| {
            for hv in &s.havens {
                let key = w.nodes.get(hv.node).map_or(0, |n| n.key);
                let up = hv
                    .ships
                    .iter()
                    .filter(|&&p| s.ship(p).is_some_and(|x| x.is_active()))
                    .count();
                out.push(SiteNow {
                    kind: "ship.haven",
                    key,
                    members: up as u64,
                    k: hv.ships.len() as u64,
                    broken: hv.shut.is_some() || hv.broken_at.is_some(),
                });
            }
        });
        self.host.e.with_conflict(|c, _| {
            let now = c.now();
            let kinds = [
                (&f.camps, "conflict.camp"),
                (&f.outposts, "conflict.camp"),
                (&f.dens, "conflict.den"),
                (&f.groves, "conflict.grove"),
            ];
            for (camps, kind) in kinds {
                for x in camps {
                    let Some(fi) = c.faction_of_key(x.key) else {
                        continue;
                    };
                    let fa = &c.factions[fi as usize];
                    out.push(SiteNow {
                        kind,
                        key: x.key,
                        members: c.pool_at(fi, now).max(0.0).floor() as u64 + u64::from(fa.away),
                        k: u64::from(fa.cap),
                        broken: fa.strain.as_ref().is_some_and(|s| s.broken.is_some()),
                    });
                }
            }
        });
        out
    }

    // ---- the teeth (EQC413) --------------------------------------------------------------------

    /// A site's stored pressure, its half-life and the share one member is (1/K, 1/slots).
    fn pressure(&self, site: Site) -> Option<(Pressure, f32, f32)> {
        match site {
            Site::Village(key) => self.host.e.with_dragons(|_, w| {
                let i = w.nodes.iter().position(|n| n.key == key)?;
                let r = w.village_row(i)?;
                Some((
                    r.pressure,
                    w.cfg.village_rule.half_life_days,
                    f32::from(r.k.max(1)),
                ))
            }),
            Site::Haven(key) => self.host.e.with_ships(|s, w| {
                let h = s
                    .havens
                    .iter()
                    .find(|h| w.nodes.get(h.node).is_some_and(|n| n.key == key))?;
                Some((
                    h.pressure,
                    s.tun.refit.half_life_days,
                    h.ships.len().max(1) as f32,
                ))
            }),
            Site::Camp(_) => None,
        }
    }

    /// A site's pressure on `day`, as stored after the kill.
    fn pressure_on(&self, site: Site, day: u32) -> Option<f32> {
        match site {
            Site::Camp(key) => self.camp(key, day).map(|c| c.pressure),
            _ => self.pressure(site).map(|(p, h, _)| {
                let (_, at) = p.parts();
                // Read on the kill's day: a bump that day left it stored as of then.
                p.now(day.max(at), h)
            }),
        }
    }

    fn owe_count(&mut self, kind: &'static str, what: Counter, before: u64, want: u64) {
        if let Some(Owed::Count { want: w, .. }) = self
            .owed
            .iter_mut()
            .find(|o| matches!(o, Owed::Count { what: x, .. } if *x == what))
        {
            *w += want;
            return;
        }
        self.owed.push(Owed::Count {
            kind,
            what,
            before,
            want,
        });
    }

    fn owe_pressure(&mut self, kind: &'static str, site: Site, day: u32, before: f32, want: f32) {
        if let Some(Owed::Pressure { want: w, .. }) = self
            .owed
            .iter_mut()
            .find(|o| matches!(o, Owed::Pressure { site: x, day: d, .. } if *x == site && *d == day))
        {
            *w += want;
            return;
        }
        self.owed.push(Owed::Pressure {
            kind,
            site,
            day,
            before,
            want,
        });
    }

    fn count(&mut self, what: Counter) -> u64 {
        match what {
            Counter::DragonLosses => self.host.e.with_dragons(|d, _| d.stats.player_losses),
            Counter::RangeKilled(r) => self.range(r).1,
            Counter::NestLost(id) => self
                .nests()
                .into_iter()
                .find(|n| n.0 == id)
                .map_or(0, |n| u64::from(n.2)),
        }
    }

    /// Reads what each kill since the last call booked against what it should have (after the
    /// day's run, so every record and order has been taken).
    fn check_owed(&mut self) {
        for o in std::mem::take(&mut self.owed) {
            match o {
                Owed::Count {
                    kind,
                    what,
                    before,
                    want,
                } => {
                    let got = self.count(what).saturating_sub(before);
                    self.teeth.booked(kind, got == want, || {
                        format!("{kind} {what:?}: booked {got}, killed {want}")
                    });
                }
                Owed::Pressure {
                    kind,
                    site,
                    day,
                    before,
                    want,
                } => {
                    let got = self.pressure_on(site, day).map_or(0.0, |p| p - before);
                    let ok = (got - want).abs() <= 1e-3 * want.max(1.0);
                    self.teeth.booked(kind, ok, || {
                        format!("{kind} {site:?} day {day}: pressure rose {got}, killed {want}")
                    });
                }
            }
        }
    }

    /// The engine's census now (between steps).
    pub fn census(&self) -> Vec<KindCensus> {
        self.host.e.census()
    }

    /// Stops the host (a final save) and deletes the world and its fork.
    pub fn close(self) {
        let protocol = self.host.e.protocol_error();
        let _ = self.host.close();
        for d in &self.scratch {
            let _ = fs::remove_dir_all(d);
        }
        assert!(protocol.is_none(), "{protocol:?}");
    }
}

/// One site as the engine holds it ([`Every::sites`]): its census kind, its key (a herd range's
/// is its region, x in the high half), its members and K, and whether it is broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SiteNow {
    pub kind: &'static str,
    pub key: u64,
    pub members: u64,
    pub k: u64,
    pub broken: bool,
}

/// The together kill-alls' wipes (EQC429): every site of one kind, through EQC403's paths.
pub const WIPES: [&str; 11] = [
    "herds.world",
    "dragons",
    "nests",
    "villagers",
    "ribbits",
    "camps",
    "outposts",
    "dens",
    "groves",
    "umvuthi",
    "pirates",
];

/// The sites a wipe reads (its kind, and for the camps' two rows the camps it kills).
pub fn wipe_reads(f: &Founded, wipe: &str) -> (&'static str, Vec<u64>) {
    let keys = |cs: &[Camp]| cs.iter().map(|c| c.key).collect::<Vec<_>>();
    match wipe {
        "herds.world" => ("herd.range", Vec::new()),
        "dragons" => ("dragon.lair", Vec::new()),
        "nests" => ("oil.nest", Vec::new()),
        "villagers" => ("village.human", Vec::new()),
        "ribbits" => ("village.ribbit", Vec::new()),
        "camps" => ("conflict.camp", keys(&f.camps)),
        "outposts" => ("conflict.camp", keys(&f.outposts)),
        "dens" => ("conflict.den", Vec::new()),
        "groves" => ("conflict.grove", Vec::new()),
        "umvuthi" => ("conflict.umvuthi", Vec::new()),
        "pirates" => ("ship.haven", Vec::new()),
        _ => panic!("no wipe {wipe}"),
    }
}

impl Every {
    /// Wipe `wipe` (one of [`WIPES`]): every site of its kind killed now through the kill paths.
    /// How many members died.
    pub fn wipe(&mut self, wipe: &str) -> u64 {
        let f = self.founded.clone();
        match wipe {
            "herds.world" => f
                .ranges
                .iter()
                .map(|&r| self.cull_range(r, u64::MAX))
                .sum(),
            "dragons" => f.lairs.iter().map(|l| self.slay_lair(l.0)).sum(),
            "nests" => self
                .nests()
                .into_iter()
                .map(|n| self.kill_nest(n.0))
                .sum(),
            "villagers" | "ribbits" => f
                .villages
                .iter()
                .filter(|v| v.ribbit == (wipe == "ribbits"))
                .map(|v| u64::from(self.kill_residents(v.key, u32::MAX)))
                .sum(),
            "camps" => f.camps.iter().map(|c| self.slay_camp(c.key, true)).sum(),
            "outposts" => f.outposts.iter().map(|c| self.slay_camp(c.key, true)).sum(),
            "dens" => f
                .dens
                .iter()
                .map(|c| self.visit_camp(c.key, "conflict.den", false))
                .sum(),
            "groves" => f
                .groves
                .iter()
                .map(|c| self.visit_camp(c.key, "conflict.grove", false))
                .sum(),
            "umvuthi" => f
                .groves
                .iter()
                .map(|c| self.visit_camp(c.key, "conflict.umvuthi", true))
                .sum(),
            "pirates" => f.havens.iter().map(|h| self.sink_haven(h.0)).sum(),
            _ => panic!("no wipe {wipe}"),
        }
    }
}

/// One together run (EQC429): the every-kind world with the roamer to `shock_day`, then `wipe`
/// (or nothing, the control) and a peaceful player to `to`; every wipe's read sites a day.
#[derive(Clone, Debug)]
pub struct Together {
    pub seed: u64,
    pub wipe: Option<&'static str>,
    pub shock_day: u64,
    /// Members killed by the wipe.
    pub killed: u64,
    /// By wipe ([`WIPES`]), its read sites at the end of each day from day 1: (day, members, K,
    /// every site whole).
    pub reads: BTreeMap<&'static str, Vec<(u64, u64, u64, bool)>>,
    pub teeth: Teeth,
}

/// The wipe's kind's members and K over its read sites, and whether every one is whole (a herd
/// range's whole is alive: it has no ceiling).
pub fn read(f: &Founded, wipe: &str, sites: &[SiteNow]) -> (u64, u64, bool) {
    let (kind, keys) = wipe_reads(f, wipe);
    let ours: Vec<&SiteNow> = sites
        .iter()
        .filter(|s| s.kind == kind && (keys.is_empty() || keys.contains(&s.key)))
        .collect();
    let whole = if kind == "herd.range" {
        ours.iter().all(|s| s.members > 0)
    } else {
        ours.iter().all(|s| s.members >= s.k && !s.broken)
    };
    (
        ours.iter().map(|s| s.members).sum(),
        ours.iter().map(|s| s.k).sum(),
        whole,
    )
}

/// Runs one together arm on `seed` (TEST mode, so the orders are taken).
pub fn together(seed: u64, wipe: Option<&'static str>, shock_day: u64, to: u64) -> Together {
    let tag = format!("together-{}", wipe.unwrap_or("control"));
    let mut w = Every::open_test(&tag, seed);
    let mut roamer = Roamer::new(seed);
    let mut out = Together {
        seed,
        wipe,
        shock_day,
        killed: 0,
        reads: BTreeMap::new(),
        teeth: Teeth::default(),
    };
    for day in 1..=to {
        w.run_to(day);
        if day == shock_day {
            roamer.kills = false;
            if let Some(x) = wipe {
                out.killed = w.wipe(x);
            }
        }
        roamer.day(&mut w, day);
        let sites = w.sites();
        for x in WIPES {
            let (n, k, whole) = read(&w.founded, x, &sites);
            out.reads.entry(x).or_default().push((day, n, k, whole));
        }
    }
    out.teeth = w.teeth.clone();
    w.close();
    out
}

/// The census grid of the long run, days (the model's).
pub const EVERY_DAYS: u64 = 24;

/// The rule bounds (EQC418), read site by site every day of a run: a site past its rule's own
/// maximum is a broken rule whatever the seed. Each episode is reported once.
#[derive(Clone, Debug, Default)]
pub struct Bounds {
    /// By site: the day it went short (below K; a village below two; a camp broken), the day its
    /// members last changed, its members then, and whether this episode was reported.
    sites: BTreeMap<(&'static str, u64), (Option<u64>, u64, u64, bool)>,
    /// The dragons' gated arrivals when each lair emptied.
    gated_at_empty: BTreeMap<u64, u64>,
    pub broken: Vec<String>,
}

impl Bounds {
    /// Day `day`'s sites, and the dragons' gated arrivals so far.
    pub fn look(&mut self, day: u64, sites: &[SiteNow], gated: u64) {
        let slack = EVERY_DAYS;
        for s in sites {
            let short = match s.kind {
                "village.human" | "village.ribbit" => s.members < 2,
                "conflict.camp" => s.broken,
                "herd.range" => s.members == 0,
                _ => s.members < s.k,
            };
            let e = self
                .sites
                .entry((s.kind, s.key))
                .or_insert((None, day, s.members, false));
            if s.members != e.2 {
                e.1 = day;
                e.2 = s.members;
            }
            match (short, e.0) {
                (false, _) => {
                    e.0 = None;
                    e.3 = false;
                    self.gated_at_empty.remove(&s.key);
                    continue;
                }
                (true, None) => {
                    e.0 = Some(day);
                    if s.kind == "dragon.lair" {
                        self.gated_at_empty.insert(s.key, gated);
                    }
                }
                (true, Some(_)) => {}
            }
            let since = e.0.unwrap_or(day);
            let quiet = day - e.1;
            let why = match s.kind {
                // Whole within 192 days of its loss (a den's only loss is its Frostmaw's).
                "conflict.den" if day - since > 192 + slack => Some("not whole 192 days after its loss"),
                // The next warrior within 96 days of the last arrival or loss.
                "conflict.grove" if quiet > 96 + slack => Some("no warrior 96 days after the last change"),
                "village.human" | "village.ribbit" if day - since > 192 + slack => {
                    Some("no pair 192 days after falling below two")
                }
                "conflict.camp" if day - since > 384 + slack => Some("not refounded 384 days after its break"),
                "oil.nest" if quiet > 96 + slack => Some("no worm 96 days after the last change"),
                "herd.range" if day.is_multiple_of(EVERY_DAYS) => Some("empty at a census"),
                "dragon.lair"
                    if day - since > 192 + slack
                        && self.gated_at_empty.get(&s.key).is_some_and(|&g| gated == g) =>
                {
                    Some("empty past 216 days with no gated arrival")
                }
                _ => None,
            };
            if let Some(why) = why
                && !e.3
            {
                e.3 = true;
                self.broken.push(format!(
                    "day {day}: {} {:#x} ({} of {}): {why}",
                    s.kind, s.key, s.members, s.k
                ));
            }
        }
    }
}

/// One long run (EQC417): the every-kind world on `seed` for `days`, with the roamer or with
/// nobody, its census every [`EVERY_DAYS`] days, the roamer's teeth and the rule bounds.
#[derive(Clone, Debug)]
pub struct LongRun {
    pub seed: u64,
    pub roamer: bool,
    pub censuses: Vec<(u64, Vec<KindCensus>)>,
    pub teeth: Teeth,
    pub bounds: Vec<String>,
    /// Game days a second over the run.
    pub rate: f64,
}

/// Runs the long run on `seed` to `days` (TEST mode for the roamer's orders).
pub fn long_run(seed: u64, roamer: bool, days: u64) -> LongRun {
    let tag = if roamer { "long-roamer" } else { "long-nobody" };
    let mut w = Every::open_test(tag, seed);
    let mut r = Roamer::new(seed);
    r.kills = roamer;
    let mut bounds = Bounds::default();
    let mut censuses = Vec::new();
    let t0 = std::time::Instant::now();
    for day in 1..=days {
        w.run_to(day);
        if roamer {
            r.day(&mut w, day);
        }
        let gated = w.host.e.with_dragons(|d, _| d.stats.arrivals_gated);
        bounds.look(day, &w.sites(), gated);
        if day.is_multiple_of(EVERY_DAYS) {
            censuses.push((day, w.census()));
        }
    }
    let rate = days as f64 / t0.elapsed().as_secs_f64().max(1e-9);
    let teeth = w.teeth.clone();
    w.close();
    LongRun {
        seed,
        roamer,
        censuses,
        teeth,
        bounds: bounds.broken,
        rate,
    }
}

/// A census kind's level: the held share for a kind of K 1 a site (lairs, dens, the Umvuthi,
/// ports), else members over capacity (EQC435).
pub fn level(c: &KindCensus) -> f64 {
    match c.kind {
        "dragon.lair" | "conflict.den" | "conflict.umvuthi" | "ship.port" => {
            1.0 - f64::from(c.empty) / f64::from(c.sites.max(1))
        }
        _ => c.fill(),
    }
}

/// A camp as the kills read it.
#[derive(Clone, Copy, Debug)]
struct CampNow {
    home: u32,
    away: u32,
    pressure: f32,
    cap: u32,
}

/// A counter a kill raises (EQC413).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Counter {
    /// The dragons' player losses.
    DragonLosses,
    /// The player kills of a range's herds.
    RangeKilled((i32, i32)),
    /// A nest's worms lost to a player.
    NestLost(u64),
}

/// A site whose pressure a kill raises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    Village(u64),
    /// A pool camp, outpost, den or grove (its faction key).
    Camp(u64),
    Haven(u64),
}

/// What a kill should have booked.
#[derive(Clone, Debug)]
enum Owed {
    Count {
        kind: &'static str,
        what: Counter,
        before: u64,
        want: u64,
    },
    Pressure {
        kind: &'static str,
        site: Site,
        day: u32,
        before: f32,
        want: f32,
    },
}

/// The teeth (EQC413): by kind, the members the suite killed, the kills checked against what the
/// engine booked as a player's, and every mismatch.
#[derive(Clone, Debug, Default)]
pub struct Teeth {
    pub killed: BTreeMap<&'static str, u64>,
    pub checked: BTreeMap<&'static str, u64>,
    pub wrong: Vec<String>,
}

impl Teeth {
    fn killed(&mut self, kind: &'static str, n: u64) {
        *self.killed.entry(kind).or_default() += n;
    }

    fn booked(&mut self, kind: &'static str, ok: bool, why: impl FnOnce() -> String) {
        *self.checked.entry(kind).or_default() += 1;
        if !ok {
            self.wrong.push(why());
        }
    }
}

/// The region (512 blocks) of block position (`x`, `z`).
pub fn region_of(x: i32, z: i32) -> (i32, i32) {
    let r = rtsim_travel::legs::REGION_BLOCKS as i32;
    (x.div_euclid(r), z.div_euclid(r))
}

/// The roamer's chance a day of killing a site of each kind it stands by (EQC411, EQC412).
pub const ROAMER_CHANCE: [(&str, f64); 10] = [
    ("dragon.lair", 0.5),
    ("conflict.den", 0.5),
    ("conflict.grove", 0.3),
    ("conflict.umvuthi", 0.3),
    ("oil.nest", 0.5),
    ("conflict.camp", 0.2),
    ("ship.haven", 0.3),
    ("herd.range", 0.5),
    ("village.human", 0.5),
    ("village.ribbit", 0.5),
];

fn chance(kind: &str) -> f64 {
    ROAMER_CHANCE
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(0.0, |x| x.1)
}

/// S11's roamer (EQC411): the model's walk, a step every second day among the nine regions around
/// it (refused beyond `min(6, 2 + day / 48)` regions of spawn), on its own stream; each day it may
/// kill each site of the region it stands in, with its kind's chance, through the kill paths of
/// EQC403. Nothing else: no position is sent, so no interest area opens (EQC404).
pub struct Roamer {
    rng: SlotRng,
    pub at: (i32, i32),
    /// Whether it still kills (the peaceful turn, EQC414).
    pub kills: bool,
}

impl Roamer {
    pub fn new(seed: u64) -> Roamer {
        Roamer {
            rng: StreamKey::new(seed, SystemId::named("eq.every_roamer"), 0).slot(0),
            at: (0, 0),
            kills: true,
        }
    }

    /// Day `day`'s turn, after the world has run to it.
    pub fn day(&mut self, w: &mut Every, day: u64) {
        if day.is_multiple_of(2) {
            let r = (2 + day / 48).min(6) as i32;
            let dx = self.rng.below(3) as i32 - 1;
            let dz = self.rng.below(3) as i32 - 1;
            let next = (self.at.0 + dx, self.at.1 + dz);
            if next.0.abs() <= r && next.1.abs() <= r {
                self.at = next;
            }
        }
        if !self.kills {
            return;
        }
        let here = self.at;
        let f = w.founded.clone();
        // Every draw is made whatever the site holds, in a fixed order, so the walk and the
        // draws of one seed do not depend on the world's state.
        for &(key, home) in &f.lairs {
            if region_of(home.0, home.2) == here && self.rng.bernoulli(chance("dragon.lair")) {
                w.slay_lair(key);
            }
        }
        for d in &f.dens {
            if region_of(d.at.0, d.at.1) == here && self.rng.bernoulli(chance("conflict.den")) {
                w.visit_camp(d.key, "conflict.den", false);
            }
        }
        for g in &f.groves {
            if region_of(g.at.0, g.at.1) == here {
                if self.rng.bernoulli(chance("conflict.grove")) {
                    w.visit_camp(g.key, "conflict.grove", false);
                }
                if self.rng.bernoulli(chance("conflict.umvuthi")) {
                    w.visit_camp(g.key, "conflict.umvuthi", true);
                }
            }
        }
        for (id, chunk, _) in w.nests() {
            if region_of(chunk.0 * 16, chunk.1 * 16) == here
                && self.rng.bernoulli(chance("oil.nest"))
            {
                w.kill_nest(id);
            }
        }
        for c in f.camps.iter().chain(&f.outposts) {
            if region_of(c.at.0, c.at.1) == here && self.rng.bernoulli(chance("conflict.camp")) {
                w.slay_camp(c.key, false);
            }
        }
        for &(key, dock) in &f.havens {
            if region_of(dock.0, dock.2) == here && self.rng.bernoulli(chance("ship.haven")) {
                w.sink_haven(key);
            }
        }
        if f.ranges.contains(&here) && self.rng.bernoulli(chance("herd.range")) {
            let n = w.quarter_of_range(here);
            w.cull_range(here, n);
        }
        for v in &f.villages {
            let kind = if v.ribbit {
                "village.ribbit"
            } else {
                "village.human"
            };
            if region_of(v.at.0, v.at.1) == here && self.rng.bernoulli(chance(kind)) {
                w.kill_residents(v.key, 2);
            }
        }
    }
}

/// One kind's row of a census.
pub fn kind<'a>(cs: &'a [KindCensus], k: &str) -> Option<&'a KindCensus> {
    cs.iter().find(|c| c.kind == k)
}
