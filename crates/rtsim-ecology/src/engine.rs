// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragon engine: every dragon's life as events on one timing wheel (PLAN.md §9, §4.3, §5.7).
//!
//! # What happens, and when
//!
//! - **Roosting.** A dragon at its lair books one `Hungry` event for when its fullness falls to
//!   the hunting level (a closed form), and a stay leg at the lair so a player who visits finds it.
//! - **Hunting** (P12's recommendation). It weighs, from its lair: caravans on the roads in its
//!   territory (a hazard on the flow), villages' livestock (market stock), and, when it holds a
//!   grudge, the home of the player it hates (P26's recommendation), whose districts it raids
//!   through the industry threat interface (C16 to C18). Season and temperament bend the weights
//!   (C25). It flies there on a booked leg, hunts for a dwell, and flies home: three events.
//! - **Crossings.** A hungry dragon whose leg crosses a road falls on the caravans passing there
//!   (PLAN.md §9 walkthrough, step 2): one event at the crossing.
//! - **Enters view.** Every leg is booked into the shared leg index (A15). A player's interest
//!   area booked there finds the legs it shares cells with, and the other way round, and the first
//!   moment the two come within range is one `Seen` event, stamped with both legs' generations. If
//!   it is still live when it fires, the dragon materializes (a boundary command) at the edge of
//!   the area.
//! - **Threats get bored** (C24). A materialized dragon harassing a place books a departure;
//!   wounds and hunger bring it forward. When it fires, the plugin flies the dragon off and
//!   captures it.
//! - **Grudges** (C3, C24). Wounds, a kin killed and a robbed hoard are reports the dragon
//!   witnesses; its sentiment toward the player falls. When it dies, its strongest memories and
//!   feelings pass to its heir: its youngest offspring, else its mate, else its lair, whose next
//!   occupant takes them over.
//! - **Population** (P13's recommendation: the sim owns every dragon). Lairs join when their
//!   structure generates; their Ice and Fire dragons are adopted. Pairs breed (M1's
//!   recommendation); young disperse to empty roosts; a roost empty for the respawn delay draws a
//!   young dragon from afar.
//!
//! # Cost
//!
//! O(events): a dragon nobody watches costs a handful of events per hunt and nothing per tick.
//! Decisions look up cells of a static prey index; legs look up the cells they cover in the leg
//! index. Nothing scans the world or every dragon.

use std::collections::{BTreeMap, BTreeSet};

use rtsim_archive::{At, Severity, Tape, kinds};
use rtsim_industry::ids::DistrictId;
use rtsim_industry::threat::{Threat, ThreatClass, ThreatSource, ThreatTarget};
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::Writer;
use rtsim_kernel::meta_pages::{PagedMap, PagedWheel};
use rtsim_kernel::rng::{StreamKey, SystemId, fnv1a64};
use rtsim_kernel::rows::PagedArena;
use rtsim_knowledge::{
    ActorId, ActorKind, HolderKind, Place, Report, ReportKind, SEVERITY_UNIT, Trait,
};
use rtsim_settle::Pos;
use rtsim_space::topology::{Rect, SharedTopologies, Topology};
use rtsim_trade::{GoodId, GoodKind};
use rtsim_travel::legs::{Meeting, Path, Stamp};
use rtsim_world::World;
use serde::Serialize;

use crate::boundary::{self, AttackerKind, Cmd, Obs, SpawnKind, Uuid};
use crate::calendar::Season;
use crate::dragon::{
    Dragon, DragonId, FlightLeg, Lair, LairId, LairKind, Lairs, Purpose, State, Target, agent_of,
    dragon_of,
};
use crate::flight::{self, dragon_owner, owner_dragon, owner_player, player_owner};
use crate::herds::{HerdId, Herds};
use crate::homes::Homes;
use crate::index::PreyIndex;
use crate::infra::{DistrictSite, Infrastructure};
use crate::nest::Nests;
use crate::oil::{OilIndex, Roll};
use crate::prey::{Item, PreyClass, PreyLedger};
use crate::species::{Gender, Species};
use crate::tunables::{DragonTunables, PreyKind};

pub const SYS_DRAGON: SystemId = SystemId::named("ecology.dragon");
const SYS_HUNT: SystemId = SystemId::named("ecology.hunt");
const SYS_BREED: SystemId = SystemId::named("ecology.breed");
/// The stream tribute stacks' op ids are drawn from, keyed by the lair's key.
const SYS_TRIBUTE: SystemId = SystemId::named("ecology.tribute");

/// A lair's actor: a group actor whose external id is the lair key under this tag.
const LAIR_TAG: u64 = 0x4C41_4952 << 32; // "LAIR"

/// The radius a dragon's own legs are booked with: its body, roughly. PLACEHOLDER 16 blocks.
pub const BODY_RADIUS: u32 = 16;

/// How long the plugin may take to fly a departing dragon out of view before it despawns it
/// anyway, ticks. PLACEHOLDER 2,400.
pub const DEPART_TIMEOUT: u32 = 2_400;

/// A hunt over a road or a village starts this far into its range: thousandths of a road kept
/// clear of the villages at its ends. PLACEHOLDER.
const ROAD_MARGIN: u32 = 150;

/// A player's home is raided within this many blocks of its centre: one region (P26).
pub(crate) const HOME_RADIUS: i64 = 512;

/// Crossings looked at per leg at most (bounds a leg's cost). PLACEHOLDER 4.
const MAX_CROSSINGS: usize = 4;

/// The dragons' row table (A16): next to the module's own page table 0x4400, which rtsim-jni's
/// per-table deletion owns.
pub const DRAGON_TABLE: u16 = 0x4410;
/// The page tables of the resident indexes when the dragons page (docs/choices-paging-followups.md
/// PF9): the wheel's log, lairs by key, lairs by cell, and the prey index's villages and roads.
pub const WHEEL_TABLE: u16 = 0x4412;
pub const LAIR_KEY_TABLE: u16 = 0x4413;
pub const LAIR_CELL_TABLE: u16 = 0x4414;
pub const VILLAGE_CELL_TABLE: u16 = 0x4415;
pub const ROAD_CELL_TABLE: u16 = 0x4416;

/// Dragons per page. PLACEHOLDER 1, measured (docs/choices-paging-modules.md, PM-D3): most of a
/// dragon's events change it, and a changed page is pinned until the next save, so a page that
/// held several dragons was pinned when any of them hunted. At 4,096 dragons and a save every
/// 6,000 ticks the peak of resident rows was 1.56 MB at 16 a page, 794 KB at 4, 322 KB at 1 (with
/// a page a lair), under a 256 KiB budget.
pub const DRAGONS_PER_PAGE: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    Hungry {
        d: DragonId,
        plan: u32,
    },
    LegEnd {
        d: DragonId,
        plan: u32,
    },
    HuntEnd {
        d: DragonId,
        plan: u32,
    },
    Cross {
        d: DragonId,
        plan: u32,
        edge: u32,
        frac_milli: u32,
        from: Tick,
        to: Tick,
    },
    Seen {
        d: DragonId,
        player: ActorId,
        meeting: Meeting,
    },
    /// A player's interest area covers a lair where `d` roosts.
    SeenAtLair {
        d: DragonId,
        player: ActorId,
        interest: Stamp,
        lair: LairId,
    },
    /// A hungry dragon's new leg looks for booked train legs along it.
    TrainScan {
        d: DragonId,
        plan: u32,
    },
    /// It meets a train it crosses.
    TrainRaid {
        d: DragonId,
        plan: u32,
        train: rtsim_industry::ids::TrainId,
    },
    Depart {
        d: DragonId,
        plan: u32,
        /// 1 bored, 2 hungry, 3 wounded, 4 passing through.
        reason: u32,
    },
    Breed {
        d: DragonId,
    },
    Disperse {
        d: DragonId,
    },
    Respawn {
        lair: LairId,
        plan: u32,
    },
    Death {
        d: DragonId,
    },
    /// The dwell on an oil field ends (OG-5): the reserve is full.
    RefuelEnd {
        d: DragonId,
        plan: u32,
    },
    /// A dimension's field-flight roll (OG-3, §4.2).
    FieldRoll {
        frame: u16,
        plan: u32,
    },
    /// The fields' daily watch (the oil audit's F2, choices-oilgate.md OGC52): every district on
    /// a field is sensed by a dragon of the field's lair. One for the world, booked when the gate
    /// opens and again each day.
    OilWatch,
    /// The oil nests' watch (OG-9e): one for the world, booked when the first nest is founded and
    /// again each nest day; stale unless `plan` is the nests' booked one.
    NestWatch {
        plan: u32,
    },
    /// A tributary village's next season's payment to the lair that raided it (SINK.md §4.6),
    /// stale unless `stamp` is still the village's tribute row's.
    Tribute {
        node: u32,
        stamp: u32,
    },
}

/// Counters for tests, the bench and the CLI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DragonStats {
    pub events: u64,
    pub stale: u64,
    pub legs: u64,
    pub decisions: u64,
    pub hunts: u64,
    pub fruitless: u64,
    pub crossings: u64,
    pub caravans_attacked: u64,
    pub village_raids: u64,
    pub district_raids: u64,
    pub train_raids: u64,
    pub seen: u64,
    pub seen_stale: u64,
    pub materialized: u64,
    pub departures: u64,
    pub bored: u64,
    pub hurt: u64,
    pub deaths_violent: u64,
    pub deaths_age: u64,
    pub births: u64,
    pub respawns: u64,
    pub claims: u64,
    pub migrations: u64,
    pub adopted: u64,
    pub discarded: u64,
    pub hoards_robbed: u64,
    /// Hunts that took animals from a wild herd.
    pub herd_hunts: u64,
    /// Meetings not materialized because the dragon is across an edge_stop seam from the player
    /// (TP12): no body crosses one.
    pub seen_across_stop: u64,
    /// The oil gate (OILGATE.md §4): oil chunks indexed, fields (a gauge), field-flight rolls and
    /// the dragons they drew, dwells on a field that filled the reserve, raids that drank fuel and
    /// the mB drunk, breaths reported, raids with passes lacking fire, Encroachment reports made,
    /// grudge raids on a district, lairs placed near oil, visitors that left the sim, and oil
    /// surveys asked for.
    pub oil_chunks: u64,
    pub fields: u64,
    pub field_rolls: u64,
    pub field_draws: u64,
    pub refuels: u64,
    pub oil_raids: u64,
    pub oil_drunk_mb: u64,
    pub breaths: u64,
    pub dry_raids: u64,
    pub encroachments: u64,
    pub district_target_raids: u64,
    pub lairs_placed: u64,
    pub visitors_left: u64,
    pub surveys: u64,
    /// Tribute payments a village made to a lair (SINK.md §4.6), the raid's first one included.
    pub tributes: u64,
    /// Tribute rows dropped: the purse dry or the lair vacant.
    pub tribute_lapses: u64,
    /// Stacks of tribute coin the plugin put in a lair's chest.
    pub tribute_stacks: u64,
}

/// A village that pays a lair tribute (SINK.md §4.6): the lair whose dragon raided it last, the
/// row's stamp (its `Ev::Tribute` is stale unless it matches) and when it last paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TributeRow {
    pub lair: LairId,
    pub stamp: u32,
    pub paid_at: Option<Tick>,
}

/// A lair's chest the plugin reported (LAIR_CHEST): a dimension dictionary id and a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chest {
    pub dimension: u32,
    pub pos: [i32; 3],
}

/// One stack of tribute coin sent to a lair's chest and not yet confirmed (keyed by its op id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placing {
    pub lair: LairId,
    pub item: Item,
    pub count: i64,
    pub chest: Chest,
}

/// The most items one placing stack carries: a chest slot's.
pub const TRIBUTE_STACK: i64 = 64;

/// One line of what happened, for the CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LogEntry {
    pub tick: u64,
    pub day: f64,
    pub kind: &'static str,
    pub dragon: Option<String>,
    pub what: String,
}

/// One hunt's result, for the season tables.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HuntRecord {
    pub tick: u64,
    pub day: f64,
    pub season: Season,
    pub dragon: String,
    pub kind: PreyKind,
    pub target: String,
    pub food: i32,
    pub taken: Vec<(String, i64)>,
}

/// What [`Dragons::raid_sites`] did: the food and goods taken, whether any raid landed, and the
/// watched district it stopped at (raided too, OGC56).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Raided {
    pub food: i32,
    pub taken: Vec<(String, i64)>,
    pub any: bool,
    pub watched: Option<DistrictSite>,
}

/// A new dragon: from a lair's adoption, a hatching, a respawn, or a scenario.
#[derive(Clone, Debug, PartialEq)]
pub struct NewDragon {
    pub name: String,
    pub species: Species,
    pub gender: Gender,
    pub variant: u8,
    pub age_ticks: u64,
    pub lair: Option<LairId>,
    pub fullness: i32,
    /// `None`: full health for its age.
    pub health: Option<f32>,
    /// `None`: derived from the world seed and a counter.
    pub seed: Option<u64>,
    /// Where it is, when it has no lair yet.
    pub at: Option<(Pos, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerView {
    pub(crate) uuid: Uuid,
    /// The player's interest area in the leg index.
    pub(crate) interest: Option<Stamp>,
}

pub struct Dragons {
    pub tun: DragonTunables,
    pub seed: u64,
    /// Every dragon (page table 0x4410, keyed by handle index; handles are never reused). Resident
    /// until [`Dragons::attach_store`] or [`Dragons::open_paged`]; then rows page under a budget
    /// (A16, docs/choices-paging-modules.md, Dragons).
    pub dragons: PagedArena<Dragon>,
    /// Every lair (page table 0x4411, keyed by lair index), paged with the dragons.
    pub lairs: Lairs,
    /// Lairs by their structure key (LAIR_FOUND, DRAGON_UNTAGGED, HOARD_ROBBED name a lair so):
    /// an index from id to id, resident (SCALE.md §8) and saved in pages of its own (PF9).
    pub(crate) lair_by_key: PagedMap<u64, LairId>,
    /// Lairs by region, so an interest area finds roosting dragons without the leg index: a
    /// dragon at rest books nothing there. An index from place to id, resident (SCALE.md §8) and
    /// saved in pages of its own (PF9).
    pub(crate) lair_cells: PagedMap<(u16, i64, i64), Vec<LairId>>,
    /// The dragons that are bodies in the game now (embodied or leaving): bounded by what players
    /// watch, so a session start and the checks find them without a pass over every dragon.
    /// Saved in the meta page.
    pub(crate) embodied: BTreeSet<DragonId>,
    /// The lairs nobody lives in and nobody flies to claim (no occupants, `vacant_since` set): a
    /// young or starving dragon looks for a roost among these, so the search loads only vacant
    /// lairs' rows, not every lair in range. Saved in the meta page.
    pub(crate) vacant: BTreeSet<LairId>,
    pub(crate) wheel: PagedWheel<Ev>,
    pub homes: Homes,
    pub ledger: PreyLedger,
    pub stats: DragonStats,
    pub log_enabled: bool,
    pub log: Vec<LogEntry>,
    /// The event archive's recorder (VIEWER.md §10.1): off until the host turns it on; not state.
    pub tape: Tape,
    pub hunts: Vec<HuntRecord>,
    /// Hunts that found food, per [season][prey kind].
    pub prey_by_season: [[u64; PreyKind::COUNT]; 4],
    /// Fullness gained, per [season][prey kind].
    pub food_by_season: [[i64; PreyKind::COUNT]; 4],
    pub(crate) outbox: Vec<Cmd>,
    pub(crate) players: BTreeMap<ActorId, PlayerView>,
    pub(crate) index: PreyIndex,
    /// Every frame's topology (TP6): flat until the engine host hands over the TOPOLOGY records'.
    pub topo: SharedTopologies,
    pub(crate) by_entity: BTreeMap<Uuid, DragonId>,
    /// Every entity the engine adopted, for good (saved; `by_entity` is this session's bodies):
    /// an untagged dragon reported again (after a crash its DRAGON_ADOPT never reached a world
    /// save, or a chunk saved before the adoption loaded again) is the dragon it already is,
    /// never a second one (review 3).
    pub(crate) adopted: BTreeMap<Uuid, DragonId>,
    /// Set while removing a dragon whose body already died in game (no despawn to send).
    pub(crate) dying_by_body: bool,
    /// Item dictionary ids the plugin uses (HOARD_ROBBED), mapped to ledger items.
    pub(crate) items: BTreeMap<u32, Item>,
    /// Prey entity types (DRAGON_ATE's body dictionary ids) that are a village herd's good.
    pub(crate) herd_bodies: BTreeMap<u32, GoodId>,
    pub(crate) next_seed: u64,
    pub(crate) now: Tick,
    /// The herds, lent for the length of [`Dragons::run_until_with_herds`] (not saved: the
    /// caller owns and saves them).
    pub(crate) wild: Option<Herds>,
    /// The oil field index (OG-2): resident, saved in pages of its own (0x4417, 0x4418).
    pub oil: OilIndex,
    /// Players' last reported positions with their height, for field flights (OG-3). Bounded:
    /// stale entries go once it holds more than a few dozen. Saved in the meta page.
    pub(crate) presence: BTreeMap<ActorId, (Pos, i32, Tick)>,
    /// Each dimension's field-flight process. Saved in the meta page.
    pub(crate) rolls: BTreeMap<u16, Roll>,
    /// The districts sensed (OG-8), by their actor: a grudge against one is a raid on it. Saved
    /// in the meta page.
    pub(crate) districts: BTreeMap<ActorId, (u64, Pos)>,
    /// The oil gate's switches (`boundary::OIL_FIELD_FLIGHTS`, `OIL_PLACE_LAIRS`): the tunables'
    /// until the plugin's OIL_CONFIG.
    pub oil_flags: u32,
    /// Villages that pay a lair tribute, by village node (SINK.md §4.6). Saved in the meta page.
    pub(crate) tribute: BTreeMap<u32, TributeRow>,
    /// The last tribute row's stamp: never reused, so an old row's event is stale. Saved.
    pub(crate) tribute_stamp: u32,
    /// Stacks of tribute coin sent to a lair's chest, by op id, until the plugin answers. Saved.
    pub(crate) placing: BTreeMap<u64, Placing>,
    /// Lairs' chests the plugin reported this session (LAIR_CHEST): not saved, cleared at a
    /// session start, since a chest can be broken while nobody watches.
    pub(crate) chests: BTreeMap<LairId, Chest>,
    /// The oil nests (OG-9e): rows in pages of their own (0x4419), the rest in the meta page.
    pub nests: Nests,
}

fn day_of(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_DAY as f64
}

pub(crate) fn place(p: Pos, y: i32) -> Place {
    Place::new(
        p.frame,
        p.x,
        y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
        p.z,
    )
}

/// Where and when an event happened, for the event archive.
pub(crate) fn spot(at: Tick, pos: Pos) -> At {
    At::new(at.0, u32::from(pos.frame), pos.x, pos.z)
}

/// An actor's archive id: its knowledge handle, as every module records actors.
fn aid(a: ActorId) -> u128 {
    u128::from(a.to_bits())
}

/// A dragon's archive id: its handle's plain bits.
pub(crate) fn did(d: DragonId) -> u128 {
    u128::from(d.to_bits())
}

/// Goods as a payload line: `"iron ingot 12, bread 3"`.
fn items_text(items: &[(String, i64)]) -> String {
    items
        .iter()
        .map(|(n, a)| format!("{n} {a}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a hunt was on, for the event archive: the prey's place where the caller knows it (else
/// the dragon's own), and its id where it has a real one (a herd, a village's key, a player's
/// actor; 0 for a road or nothing).
pub(crate) struct Quarry {
    pub kind: PreyKind,
    pub name: String,
    pub pos: Option<Pos>,
    pub id: u128,
}

/// The same 64-bit stand-in for a player's UUID the phase 1 world uses (its `uuid_hash`), so one
/// player is one actor in both modules.
pub fn uuid_hash(most: u64, least: u64) -> u64 {
    StreamKey::new(most, SystemId::named("player.uuid"), least).block(0, 0)[0]
}

impl Dragons {
    /// Hands the engine every frame's topology (TP6): flights, hunts, lairs and the prey index
    /// measure through seams from then on. The prey index is rebuilt.
    pub fn set_topologies(&mut self, t: SharedTopologies) {
        if std::sync::Arc::ptr_eq(&t, &self.topo) {
            return;
        }
        if *t != *self.topo {
            self.topo = t;
            self.index.reset();
        }
    }

    /// The topology of `frame`.
    pub fn topo(&self, frame: u16) -> &Topology {
        self.topo.of(frame)
    }

    /// The canonical position of a block the plugin reported (TP4: every position the engine
    /// keeps is canonical, so a body that crossed a seam is where it is, not a jump).
    pub fn canon(&self, dimension: u32, x: f64, z: f64) -> Pos {
        let frame = dimension as u16;
        let (x, z) = self.topo(frame).canonical_i32(x as i32, z as i32);
        Pos::new(frame, x, z)
    }

    /// The regions a canonical rectangle widened by `r` covers on `frame`, across seams.
    pub(crate) fn regions_near(&self, frame: u16, r: Rect) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        for part in self.topo(frame).fold_rect(r) {
            let (x0, z0, x1, z1) = part.grid_cells(rtsim_travel::legs::REGION_BLOCKS);
            for rx in x0..=x1 {
                for rz in z0..=z1 {
                    out.push((rx, rz));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn new(seed: u64, tun: DragonTunables, now: Tick) -> Self {
        let homes = Homes::new(tun.home_half_life_ticks, tun.home_regions);
        Dragons {
            tun,
            seed,
            dragons: PagedArena::new(DRAGON_TABLE, DRAGONS_PER_PAGE),
            lairs: Lairs::new(),
            lair_by_key: PagedMap::new(),
            lair_cells: PagedMap::new(),
            embodied: BTreeSet::new(),
            vacant: BTreeSet::new(),
            wheel: PagedWheel::new(now),
            homes,
            ledger: PreyLedger::default(),
            stats: DragonStats::default(),
            log_enabled: false,
            log: Vec::new(),
            tape: Tape::new(),
            hunts: Vec::new(),
            prey_by_season: [[0; PreyKind::COUNT]; 4],
            food_by_season: [[0; PreyKind::COUNT]; 4],
            outbox: Vec::new(),
            players: BTreeMap::new(),
            index: PreyIndex::default(),
            topo: rtsim_space::topology::all_flat(),
            by_entity: BTreeMap::new(),
            adopted: BTreeMap::new(),
            dying_by_body: false,
            items: BTreeMap::new(),
            herd_bodies: BTreeMap::new(),
            next_seed: 0,
            now,
            wild: None,
            oil: OilIndex::default(),
            presence: BTreeMap::new(),
            rolls: BTreeMap::new(),
            districts: BTreeMap::new(),
            oil_flags: 0,
            tribute: BTreeMap::new(),
            tribute_stamp: 0,
            placing: BTreeMap::new(),
            chests: BTreeMap::new(),
            nests: Nests::default(),
        }
        .with_oil_flags()
    }

    fn with_oil_flags(mut self) -> Self {
        self.oil_flags = self.tun.oil.flags;
        self
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    pub fn pending_events(&self) -> usize {
        self.wheel.len()
    }

    /// A lower bound on the next event's time (O(levels)).
    pub fn next_due(&self) -> Option<Tick> {
        self.wheel.next_due_lower_bound()
    }

    /// Commands for the plugin, oldest first.
    pub fn take_commands(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.outbox)
    }

    /// Maps a plugin item dictionary id to a ledger item (for HOARD_ROBBED).
    pub fn define_item(&mut self, id: u32, item: Item) {
        self.items.insert(id, item);
    }

    /// Maps a prey entity type (DRAGON_ATE) to the herd good it is in a village's stock.
    pub fn define_herd_body(&mut self, body: u32, good: GoodId) {
        self.herd_bodies.insert(body, good);
    }

    pub fn dragon(&self, d: DragonId) -> Option<&Dragon> {
        self.dragons.get(d)
    }

    /// The first dragon with this name. A whole-table pass, for tools and tests only.
    pub fn by_name(&self, name: &str) -> Option<DragonId> {
        self.dragons
            .iter()
            .find(|(_, d)| d.name == name)
            .map(|(h, _)| h)
    }

    pub fn lair_of_key(&self, key: u64) -> Option<LairId> {
        self.lair_by_key.get(&key).copied()
    }

    pub(crate) fn note(&mut self, at: Tick, kind: &'static str, d: Option<DragonId>, what: String) {
        if !self.log_enabled {
            return;
        }
        let dragon = d.and_then(|d| self.dragons.get(d)).map(|x| x.name.clone());
        self.log.push(LogEntry {
            tick: at.0,
            day: day_of(at),
            kind,
            dragon,
            what,
        });
    }

    pub(crate) fn schedule(&mut self, at: Tick, ev: Ev) {
        self.wheel.schedule(at.max(self.wheel.now()), ev);
    }

    /// The player's actor in the world's knowledge, registered by UUID (idempotent).
    pub fn player(&mut self, w: &mut World, uuid: Uuid) -> ActorId {
        let id = w
            .knowledge
            .actors_mut()
            .register(ActorKind::Player, uuid_hash(uuid.0, uuid.1));
        self.players.entry(id).or_insert(PlayerView {
            uuid,
            interest: None,
        });
        id
    }

    pub(crate) fn uuid_of(&self, p: ActorId) -> Uuid {
        self.players.get(&p).map_or((0, 0), |v| v.uuid)
    }

    // ---- setup ---------------------------------------------------------------------------------

    /// A lair joins when its structure generates (LAIR_FOUND; C22's rule for villages applied to
    /// lairs). Idempotent by key. Its respawn clock starts: if no dragon is adopted into it within
    /// the delay, a young one comes.
    #[allow(clippy::too_many_arguments)]
    pub fn add_lair(
        &mut self,
        w: &mut World,
        key: u64,
        name: &str,
        species: Species,
        kind: LairKind,
        pos: Pos,
        y: i32,
        at: Tick,
    ) -> LairId {
        if let Some(&l) = self.lair_by_key.get(&key) {
            return l;
        }
        let actor = w
            .knowledge
            .actors_mut()
            .register(ActorKind::Group, LAIR_TAG ^ key);
        w.knowledge.add_holder(actor, HolderKind::Notable);
        let id = self.lairs.push(Lair {
            key,
            name: name.to_string(),
            species,
            kind,
            pos,
            y,
            actor,
            occupants: Vec::new(),
            hoard: BTreeMap::new(),
            vacant_since: Some(at),
            plan: 1,
            unplaced: BTreeMap::new(),
            placed: 0,
            stacks: 0,
        });
        debug_assert_eq!(self.lairs.get(id).map(|l| l.key), Some(key));
        self.track_lair(id);
        self.lair_by_key.insert(key, id);
        let (rx, rz) = rtsim_travel::legs::LegIndex::region_of(i64::from(pos.x), i64::from(pos.z));
        self.lair_cells.get_or_default((pos.frame, rx, rz)).push(id);
        let delay = self.tun.respawn_delay_days * TICKS_PER_DAY;
        self.schedule(at + delay, Ev::Respawn { lair: id, plan: 1 });
        id
    }

    fn fresh_seed(&mut self) -> u64 {
        self.next_seed += 1;
        StreamKey::new(self.seed, SYS_DRAGON, 0).block(self.next_seed, 0)[0]
    }

    /// A dragon joins the sim at `at`: a notable whose death of age is booked now (C24).
    pub fn add_dragon(&mut self, w: &mut World, n: NewDragon, at: Tick) -> DragonId {
        let seed = n.seed.unwrap_or_else(|| self.fresh_seed());
        let sp = self.tun.species(n.species).clone();
        let life = sp.lifespan_ticks();
        let left = life.saturating_sub(n.age_ticks).max(TICKS_PER_DAY);
        let home = n
            .lair
            .and_then(|l| self.lairs.get(l))
            .map_or(ActorId::NULL, |l| l.actor);
        let actor = w.knowledge.create_notable(seed, home, home, at, left);
        let max = sp.max_health_at(n.age_ticks);
        let row = Dragon {
            actor,
            seed,
            name: n.name,
            species: n.species,
            gender: n.gender,
            variant: n.variant,
            age_base: n.age_ticks,
            age_at: at,
            dies: at + left,
            lair: n.lair,
            fullness: n.fullness.clamp(0, self.tun.full),
            fed_at: at,
            health: n.health.unwrap_or(max).min(max),
            health_at: at,
            state: State::Roosting,
            leg: None,
            plan: 0,
            epoch: 0,
            mate: None,
            parent: None,
            children: Vec::new(),
            victims: Vec::new(),
            fruitless: 0,
            bout: 0,
            last_grudge: None,
            carried: BTreeMap::new(),
            rest: n.at,
            oil: self.tun.oil.capacity,
            oil_at: at,
            visitor: None,
            sensed: Vec::new(),
        };
        let d = self.dragons.insert(row);
        self.schedule(at + left, Ev::Death { d });
        if let Some(l) = n.lair {
            self.occupy(w, d, l, at, false);
            self.roost(w, d, at);
        }
        d
    }

    /// Two dragons of a lair become mates; the pair breeds (M1's recommendation).
    pub fn pair(&mut self, w: &mut World, a: DragonId, b: DragonId, at: Tick) {
        if a == b || !self.dragons.contains(a) || !self.dragons.contains(b) {
            return;
        }
        self.dragons.get_mut(a).expect("live").mate = Some(b);
        self.dragons.get_mut(b).expect("live").mate = Some(a);
        self.refresh_heir(w, a);
        self.refresh_heir(w, b);
        let female = if self.dragons[a].gender == Gender::Female {
            a
        } else {
            b
        };
        let t = self.next_clutch(female, at);
        self.schedule(t, Ev::Breed { d: female });
    }

    fn next_clutch(&self, d: DragonId, at: Tick) -> Tick {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species);
        // Mature at stage 4 (75 days, Ice and Fire's `isMature`); then every `breed_days`.
        let mature_age = sp.stage_days[2] * TICKS_PER_DAY;
        let until_mature = mature_age.saturating_sub(x.age(at));
        at + until_mature + sp.breed_days * TICKS_PER_DAY
    }

    /// `child` is `parent`'s young (and its mate's): it becomes their heir (C24), and it leaves
    /// to claim a roost of its own when old enough.
    pub fn set_parent(&mut self, w: &mut World, child: DragonId, parent: DragonId, at: Tick) {
        if !self.dragons.contains(child) || !self.dragons.contains(parent) {
            return;
        }
        self.dragons.get_mut(child).expect("live").parent = Some(parent);
        let mate = self.dragons[parent].mate;
        for p in [Some(parent), mate].into_iter().flatten() {
            if let Some(x) = self.dragons.get_mut(p)
                && !x.children.contains(&child)
            {
                x.children.push(child);
            }
            self.refresh_heir(w, p);
        }
        let c = &self.dragons[child];
        let sp = self.tun.species(c.species);
        let due = (sp.disperse_days * TICKS_PER_DAY).saturating_sub(c.age(at));
        self.schedule(at + due, Ev::Disperse { d: child });
    }

    /// The heir link (C24): youngest living child, else mate, else lair.
    fn refresh_heir(&mut self, w: &mut World, d: DragonId) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let youngest = x
            .children
            .iter()
            .copied()
            .filter(|c| self.dragons.contains(*c))
            .min_by_key(|c| (self.dragons[*c].age(self.now), *c));
        let heir = youngest
            .map(|c| self.dragons[c].actor)
            .or_else(|| x.mate.and_then(|m| self.dragons.get(m)).map(|m| m.actor))
            .or_else(|| x.lair.map(|l| self.lairs[l as usize].actor))
            .unwrap_or(ActorId::NULL);
        w.knowledge.set_heir(x.actor, heir);
    }

    // ---- players and interest areas (A15) -------------------------------------------------------

    /// A PLAYER record: where a player is. Feeds the home counter (P26) and books the player's
    /// interest area for the next window, replacing the last (the bridge reports every 20 ticks;
    /// a new area is booked only when the player moved a quarter of the view radius or the old one
    /// ends within a window).
    pub fn observe_player(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) -> ActorId {
        let p = self.player(w, uuid);
        self.homes.observe(p, pos, at);
        let window = w.legs.window_ticks();
        let keep = self.players.get(&p).and_then(|v| v.interest).and_then(|s| {
            let row = w.legs.get(s.leg).filter(|r| r.generation == s.generation)?;
            let moved = self.topo(pos.frame).dist_sq(
                (i64::from(row.path.x0), i64::from(row.path.z0)),
                (i64::from(pos.x), i64::from(pos.z)),
            ) as i64;
            let quarter = i64::from(self.tun.view_radius / 4).pow(2);
            (row.path.frame == pos.frame && moved <= quarter && row.path.t1.0 >= at.0 + window)
                .then_some(s)
        });
        if keep.is_none() {
            let path = flight::stay(pos, at, at + 2 * window, self.tun.view_radius);
            self.book_interest(w, p, path);
        }
        p
    }

    /// A stay the caller knows in one piece (a scripted player): one interest area from `from` to
    /// `until`. The home counter still learns from [`Dragons::observe_player`] as time passes.
    pub fn player_stays(
        &mut self,
        w: &mut World,
        uuid: Uuid,
        pos: Pos,
        from: Tick,
        until: Tick,
    ) -> ActorId {
        let p = self.player(w, uuid);
        let path = flight::stay(pos, from, until, self.tun.view_radius);
        self.book_interest(w, p, path);
        p
    }

    /// The player left (logged out, changed dimension): its interest area goes, and with it every
    /// meeting booked against it.
    pub fn player_left(&mut self, w: &mut World, p: ActorId) {
        if let Some(v) = self.players.get_mut(&p)
            && let Some(s) = v.interest.take()
        {
            w.legs.cancel(s.leg);
        }
    }

    fn book_interest(&mut self, w: &mut World, p: ActorId, path: Path) {
        let old = self.players.get(&p).and_then(|v| v.interest);
        let stamp = match old.and_then(|s| w.legs.replan(s.leg, path)) {
            Some(s) => s,
            None => w.legs.book(path, player_owner(p)),
        };
        if let Some(v) = self.players.get_mut(&p) {
            v.interest = Some(stamp);
        }
        let mut found = Vec::new();
        for other in w.legs.sharing(stamp) {
            if w.legs
                .get(other.leg)
                .and_then(|r| owner_dragon(r.owner))
                .is_none()
            {
                continue;
            }
            if let Some(m) = w.legs.meeting(other, stamp) {
                found.push(m);
            }
        }
        self.seen_legs(w, p, &found);
        self.seen_at_lairs(p, stamp, path);
    }

    /// Dragon legs crossing a player's area: one `Seen` per meeting (`a` the dragon's leg).
    fn seen_legs(&mut self, w: &World, p: ActorId, found: &[Meeting]) {
        for m in found {
            let Some(d) = w.legs.get(m.a.leg).and_then(|r| owner_dragon(r.owner)) else {
                continue;
            };
            self.schedule(
                m.at,
                Ev::Seen {
                    d,
                    player: p,
                    meeting: *m,
                },
            );
        }
    }

    /// Dragons at rest in a lair the area covers at its start: they book no leg.
    fn seen_at_lairs(&mut self, p: ActorId, stamp: Stamp, path: Path) {
        let r = i64::from(path.radius);
        let bbox = Rect::spanning(
            (i64::from(path.x0), i64::from(path.z0)),
            (i64::from(path.x1), i64::from(path.z1)),
            r,
        );
        let mut found = Vec::new();
        for (rx, rz) in self.regions_near(path.frame, bbox) {
            found.extend(
                self.lair_cells
                    .get(&(path.frame, rx, rz))
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        found.sort_unstable();
        found.dedup();
        let topo = self.topo.clone();
        let t = topo.of(path.frame);
        for l in found {
            let lp = self.lairs[l as usize].pos;
            let (px, pz) = path.at(path.t0);
            if flight::dist(t, (px, pz), (i64::from(lp.x), i64::from(lp.z)))
                > f64::from(path.radius)
            {
                continue;
            }
            for d in self.lairs[l as usize].occupants.clone() {
                let at = path.t0.max(self.now);
                self.schedule(
                    at,
                    Ev::SeenAtLair {
                        d,
                        player: p,
                        interest: stamp,
                        lair: l,
                    },
                );
            }
        }
    }

    /// A player's position for the home counter (P26) only, when the player's interest area is
    /// the engine's shared one (`rtsim_world::interest::Viewer`, routed by
    /// `World::route_area`) instead of this engine's own ([`Dragons::observe_player`]).
    pub fn observe_home(&mut self, w: &mut World, uuid: Uuid, pos: Pos, at: Tick) -> ActorId {
        let p = self.player(w, uuid);
        self.homes.observe(p, pos, at);
        p
    }

    /// A dragon settling at its lair: any player whose interest area covers the lair now sees it.
    fn visitors_at_lair(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(l) = self.dragons[d].lair else {
            return;
        };
        let lp = self.lairs[l as usize].pos;
        let (rx, rz) = rtsim_travel::legs::LegIndex::region_of(i64::from(lp.x), i64::from(lp.z));
        for s in w.legs.in_region(lp.frame, rx, rz, at) {
            let Some(row) = w.legs.get(s.leg) else {
                continue;
            };
            let Some(p) = owner_player(row.owner) else {
                continue;
            };
            if row.path.t1 < at {
                continue;
            }
            let (px, pz) = row.path.at(at);
            if flight::dist(
                self.topo(lp.frame),
                (px, pz),
                (i64::from(lp.x), i64::from(lp.z)),
            ) <= f64::from(row.path.radius)
            {
                self.schedule(
                    at,
                    Ev::SeenAtLair {
                        d,
                        player: p,
                        interest: s,
                        lair: l,
                    },
                );
            }
        }
    }

    // ---- the step loop -------------------------------------------------------------------------

    /// Runs every dragon event up to and including `to`, in time order, with the world caught up
    /// to each event's time first. Nothing is ticked.
    ///
    /// **Paged** ([`Dragons::attach_store`], [`Dragons::open_paged`]): rows load as events read
    /// them and nothing is evicted here; the host calls [`Dragons::enforce_budget`] at its step
    /// boundaries. Resident, the rows changed are measured here (no eviction).
    pub fn run_until(&mut self, w: &mut World, infra: &mut dyn Infrastructure, to: Tick) {
        self.index.refresh(w, &self.topo);
        while let Some((at, ev)) = self.wheel.pop_until(to) {
            w.run_until(at);
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        w.run_until(to);
        self.now = self.now.max(to);
        self.measure_if_resident();
    }

    /// Resident tables evict nothing, so their only budget work is measuring the rows changed:
    /// done at the end of every run, so a host that never pages needs no call of its own.
    fn measure_if_resident(&mut self) {
        if !self.is_paged() {
            self.enforce_budget();
        }
    }

    /// As [`Dragons::run_until`], with wild herds as prey (PLAN.md §10.2): the herds' events and
    /// the dragons' run in one time order (a herd event at the same tick as a dragon's runs first),
    /// and hungry dragons weigh and hunt the herds through the herds' prey interface
    /// (`Herds::prey_near`, `Herds::predate`). The herds share the world's leg index.
    pub fn run_until_with_herds(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        herds: &mut Herds,
        to: Tick,
    ) {
        self.wild = Some(std::mem::replace(herds, Herds::empty()));
        self.index.refresh(w, &self.topo);
        loop {
            let next = self.wheel.next_due().filter(|t| *t <= to).unwrap_or(to);
            if let Some(h) = self.wild.as_mut() {
                h.run_until(&mut w.legs, next);
            }
            let Some((at, ev)) = self.wheel.pop_until(next) else {
                break;
            };
            w.run_until(at);
            self.now = self.now.max(at);
            self.stats.events += 1;
            self.handle(w, infra, at, ev);
        }
        if let Some(h) = self.wild.as_mut() {
            h.run_until(&mut w.legs, to);
        }
        w.run_until(to);
        self.now = self.now.max(to);
        *herds = self.wild.take().expect("lent");
        self.measure_if_resident();
    }

    fn handle(&mut self, w: &mut World, infra: &mut dyn Infrastructure, at: Tick, ev: Ev) {
        match ev {
            Ev::Hungry { d, plan } => {
                if !self.live(d, plan) {
                    return;
                }
                if matches!(self.dragons[d].state, State::Roosting) {
                    self.decide(w, d, at, 0);
                }
            }
            Ev::LegEnd { d, plan } => {
                if self.live(d, plan) {
                    self.leg_end(w, infra, d, at);
                }
            }
            Ev::HuntEnd { d, plan } => {
                if self.live(d, plan) {
                    self.hunt_end(w, d, at);
                }
            }
            Ev::Cross {
                d,
                plan,
                edge,
                frac_milli,
                from,
                to,
            } => {
                if !self.live(d, plan) {
                    return;
                }
                let x = &self.dragons[d];
                if x.fullness_at(&self.tun, at) >= self.tun.hunt_below {
                    return;
                }
                self.stats.crossings += 1;
                let (food, taken, victims) = self.hunt_road(w, d, edge, frac_milli, from, to, at);
                self.feed(d, at, food, &victims);
                if food > 0 || !taken.is_empty() {
                    let name = self.road_name(w, edge);
                    let quarry = Quarry {
                        kind: PreyKind::Road,
                        name: format!("{name} (crossing)"),
                        pos: None,
                        id: 0,
                    };
                    self.record_hunt(at, d, quarry, food, taken);
                }
            }
            Ev::Seen { d, player, meeting } => self.seen(w, d, player, meeting, at),
            Ev::TrainScan { d, plan } => {
                if self.live(d, plan) {
                    self.train_scan(infra, d);
                }
            }
            Ev::TrainRaid { d, plan, train } => {
                if self.live(d, plan) {
                    self.train_raid(infra, d, train, at);
                }
            }
            Ev::SeenAtLair {
                d,
                player,
                interest,
                lair,
            } => {
                let home = self.dragons.get(d).is_some_and(|x| {
                    x.lair == Some(lair) && matches!(x.state, State::Roosting) && x.rest.is_none()
                });
                if !home || !w.legs.is_live(interest) {
                    self.stats.seen_stale += 1;
                    return;
                }
                self.stats.seen += 1;
                self.materialize(w, d, player, false, false, None, at);
            }
            Ev::Depart { d, plan, reason } => {
                if self.live(d, plan) {
                    self.depart(d, reason, at);
                }
            }
            Ev::Breed { d } => self.breed(w, d, at),
            Ev::Disperse { d } => self.disperse(w, d, at),
            Ev::Respawn { lair, plan } => self.respawn(w, lair, plan, at),
            Ev::Death { d } => {
                let Some(x) = self.dragons.get(d) else {
                    return;
                };
                if x.dies > at {
                    return;
                }
                // The world's loop has already passed its memories on (Knowledge::die) when it
                // caught up to `at`; if not, do it now.
                if w.knowledge.notable(x.actor).is_some() {
                    w.knowledge.die(x.actor, at);
                }
                self.stats.deaths_age += 1;
                let here = x.position(&self.topo, &self.lairs, at).0;
                self.tape_death(d, here, None, at);
                self.note(
                    at,
                    "death",
                    Some(d),
                    "dies of age; its heir inherits".into(),
                );
                self.remove(w, d, at);
            }
            Ev::RefuelEnd { d, plan } => {
                if self.live(d, plan) {
                    self.refuel_end(w, d, at);
                }
            }
            Ev::FieldRoll { frame, plan } => self.field_roll(w, frame, plan, at),
            Ev::OilWatch => self.oil_watch(w, infra, at),
            Ev::NestWatch { plan } => self.nest_watch(w, infra, plan, at),
            Ev::Tribute { node, stamp } => {
                let Some(row) = self.tribute.get(&node).copied() else {
                    self.stats.stale += 1;
                    return;
                };
                if row.stamp != stamp {
                    self.stats.stale += 1;
                    return;
                }
                if self.vacant.contains(&row.lair) {
                    self.tribute_lapse(w, node, "its lair stands empty", at);
                } else {
                    self.tribute_pay(w, node, at);
                }
            }
        }
    }

    /// Keeps [`Dragons::embodied`] in step with `d`'s state: called after every change of state.
    pub(crate) fn track(&mut self, d: DragonId) {
        if self.dragons.get(d).is_some_and(Dragon::is_embodied) {
            self.embodied.insert(d);
        } else {
            self.embodied.remove(&d);
        }
    }

    /// Keeps [`Dragons::vacant`] in step with lair `l`: called after every change of its
    /// occupants or its vacancy.
    pub(crate) fn track_lair(&mut self, l: LairId) {
        if self
            .lairs
            .get(l)
            .is_some_and(|x| x.occupants.is_empty() && x.vacant_since.is_some())
        {
            self.vacant.insert(l);
        } else {
            self.vacant.remove(&l);
        }
    }

    pub(crate) fn live(&mut self, d: DragonId, plan: u32) -> bool {
        let ok = self.dragons.get(d).is_some_and(|x| x.plan == plan);
        if !ok {
            self.stats.stale += 1;
        }
        ok
    }

    // ---- legs ------------------------------------------------------------------------------------

    pub(crate) fn book_leg(&mut self, w: &mut World, d: DragonId, path: Path, y: i32) {
        self.stats.legs += 1;
        // Every dragon is named: a flight or a stay at prey is a named mover's leg.
        self.tape.leg(
            kinds::DRAGONS_FLIGHT,
            u32::from(path.frame),
            (path.t0.0, path.x0, path.z0),
            (path.t1.0, path.x1, path.z1),
            did(d),
        );
        let old = self.dragons[d].leg.map(|l| l.stamp.leg);
        let stamp = match old.and_then(|l| w.legs.replan(l, path)) {
            Some(s) => s,
            None => w.legs.book(path, dragon_owner(d)),
        };
        self.dragons.get_mut(d).expect("live").leg = Some(FlightLeg { path, y, stamp });
        for other in w.legs.sharing(stamp) {
            let Some(p) = w.legs.get(other.leg).and_then(|r| owner_player(r.owner)) else {
                continue;
            };
            if let Some(m) = w.legs.meeting(stamp, other) {
                self.schedule(
                    m.at,
                    Ev::Seen {
                        d,
                        player: p,
                        meeting: m,
                    },
                );
            }
        }
    }

    fn cancel_leg(&mut self, w: &mut World, d: DragonId) {
        // Read first: a dragon with no leg keeps its row (and its page) clean.
        if self.dragons.get(d).is_none_or(|x| x.leg.is_none()) {
            return;
        }
        if let Some(l) = self.dragons.get_mut(d).and_then(|x| x.leg.take()) {
            w.legs.cancel(l.stamp.leg);
        }
    }

    pub(crate) fn bump(&mut self, d: DragonId) -> u32 {
        let x = self.dragons.get_mut(d).expect("live");
        x.plan = x.plan.wrapping_add(1);
        x.plan
    }

    pub(crate) fn target_pos(&self, w: &World, t: Target) -> Pos {
        match t {
            Target::Road { x, z, edge, .. } => {
                let frame = w.nodes[w.edges[edge as usize].a].pos.frame;
                Pos::new(frame, x, z)
            }
            Target::Village { node } => w.nodes[node as usize].pos,
            Target::Home { frame, x, z, .. } => Pos::new(frame, x, z),
            Target::Herd { frame, x, z, .. } => Pos::new(frame, x, z),
            Target::District { frame, x, z, .. } => Pos::new(frame, x, z),
        }
    }

    fn purpose_pos(&self, w: &World, d: DragonId, p: Purpose) -> (Pos, i32) {
        let x = &self.dragons[d];
        let cruise = self.tun.species(x.species).cruise_y;
        match p {
            Purpose::Hunt(t) => (self.target_pos(w, t), cruise),
            Purpose::Home => match x.lair {
                Some(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
                None => x.position(&self.topo, &self.lairs, self.now),
            },
            Purpose::Claim(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
            Purpose::Refuel { frame, x, z, .. } => (Pos::new(frame, x, z), cruise),
        }
    }

    /// Flies from where it is now to `to` for `purpose`: one leg, one event at its end, and a
    /// crossing event for each road a hungry dragon passes over.
    pub(crate) fn fly(&mut self, w: &mut World, d: DragonId, purpose: Purpose, at: Tick) {
        let (to, _) = self.purpose_pos(w, d, purpose);
        let (from, _) = self.dragons[d].position(&self.topo, &self.lairs, at);
        let sp = self.tun.species(self.dragons[d].species).clone();
        let path = flight::leg(self.topo(from.frame), from, to, at, sp.speed, BODY_RADIUS);
        let plan = self.bump(d);
        {
            let x = self.dragons.get_mut(d).expect("live");
            x.state = State::Flying(purpose);
            x.rest = None;
        }
        self.track(d);
        self.book_leg(w, d, path, sp.cruise_y);
        self.schedule(path.t1, Ev::LegEnd { d, plan });
        self.book_crossings(w, d, path, purpose, plan);
        if self.dragons[d].fullness_at(&self.tun, path.t1) < self.tun.hunt_below {
            self.schedule(at, Ev::TrainScan { d, plan });
        }
    }

    /// The booked train legs a hungry dragon's leg crosses: one lookup per region it passes; it
    /// meets the first in the middle of the window both are under way in.
    fn train_scan(&mut self, infra: &mut dyn Infrastructure, d: DragonId) {
        let Some(l) = self.dragons[d].leg else {
            return;
        };
        let p = l.path;
        let (a, b) = (
            (i64::from(p.x0), i64::from(p.z0)),
            (i64::from(p.x1), i64::from(p.z1)),
        );
        let t = self.topo(p.frame);
        let steps = (flight::dist(t, a, b) / 256.0).ceil().max(1.0) as u32;
        let mut regions: Vec<(i32, i32)> = (0..=steps)
            .map(|k| {
                let (x, z) = flight::lerp(t, a, b, k * 1000 / steps);
                let r = rtsim_travel::legs::LegIndex::region_of(x, z);
                (r.0 as i32, r.1 as i32)
            })
            .collect();
        regions.dedup();
        let plan = self.dragons[d].plan;
        if let Some(s) = infra
            .trains_crossed(p.frame, &regions, p.t0, p.t1)
            .into_iter()
            .find(|s| s.to >= s.from)
        {
            let at = Tick(s.from.0 + (s.to.0 - s.from.0) / 2);
            self.schedule(
                at,
                Ev::TrainRaid {
                    d,
                    plan,
                    train: s.train,
                },
            );
        }
    }

    /// A dragon falls on a train under way (I24): the industry engine takes the loot from its
    /// carriages, bounded per attack; the ledger splits it.
    fn train_raid(
        &mut self,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        train: rtsim_industry::ids::TrainId,
        at: Tick,
    ) {
        let x = &self.dragons[d];
        if !matches!(x.state, State::Flying(_))
            || x.fullness_at(&self.tun, at) >= self.tun.hunt_below
        {
            return;
        }
        let sp = self.tun.species(x.species).clone();
        let stage = x.stage(&sp, at);
        let passes = sp.passes[stage.index()].max(1);
        let seed = x.seed;
        let op = self.oil_passes(d, passes, at);
        let threat = Threat {
            id: fnv1a64(
                &[
                    seed.to_le_bytes(),
                    at.0.to_le_bytes(),
                    train.0.to_le_bytes(),
                ]
                .concat(),
            ),
            source: ThreatSource::Dragon,
            class: ThreatClass::Flying,
            count: passes,
            strength: f64::from(passes) * sp.strength_per_pass[stage.index()] * op.factor,
            target: ThreatTarget::Train(train),
            loads: None,
            at,
        };
        let Some(loot) = infra.raid_train(&threat) else {
            return;
        };
        self.stats.train_raids += 1;
        if op.burn > 0 {
            self.dragons
                .get_mut(d)
                .expect("live")
                .burn_oil(&self.tun.oil, op.burn, at);
        }
        if op.dry {
            self.stats.dry_raids += 1;
        }
        let mut food = 0;
        let mut taken = Vec::new();
        for (g, n) in loot.iter() {
            let name = infra
                .good_name(g)
                .unwrap_or_else(|| format!("good {}", g.0));
            let class = self.tun.prey.class(&name, false);
            food += self.ledger.take(Item::Industry(g), n, class);
            if matches!(class, PreyClass::Valuable) {
                self.hoard(d, Item::Industry(g), n);
            }
            taken.push((name, n));
        }
        if self.tape.is_on() {
            let here = self.dragons[d].position(&self.topo, &self.lairs, at).0;
            let stolen: i64 = taken.iter().map(|(_, n)| *n).sum();
            let severity = if stolen > 0 {
                Severity::Major
            } else {
                Severity::Minor
            };
            self.tape
                .event_as(
                    kinds::DRAGONS_RAID,
                    severity,
                    spot(at, here),
                    did(d),
                    u128::from(train.0),
                    stolen,
                )
                .text(|| format!("falls on train {}, took {}", train.0, items_text(&taken)));
        }
        self.feed(d, at, food, &[]);
        let quarry = Quarry {
            kind: PreyKind::Train,
            name: format!("train {}", train.0),
            pos: None,
            id: u128::from(train.0),
        };
        self.record_hunt(at, d, quarry, food, taken);
    }

    fn book_crossings(
        &mut self,
        w: &mut World,
        d: DragonId,
        path: Path,
        purpose: Purpose,
        plan: u32,
    ) {
        let skip = match purpose {
            Purpose::Hunt(Target::Road { edge, .. }) => Some(edge),
            _ => None,
        };
        let x = &self.dragons[d];
        // Only a dragon still hungry when it reaches a road hunts there.
        if x.fullness_at(&self.tun, path.t1) >= self.tun.hunt_below {
            return;
        }
        let sp = self.tun.species(x.species);
        let half = flight::flight_ticks(f64::from(sp.reach), sp.speed);
        let (p0, p1) = (
            (i64::from(path.x0), i64::from(path.z0)),
            (i64::from(path.x1), i64::from(path.z1)),
        );
        let t = self.topo(path.frame);
        let (cx, cz) = ((p0.0 + p1.0) / 2, (p0.1 + p1.1) / 2);
        let r = (flight::dist(t, p0, p1) / 2.0).ceil() as i64 + 1;
        let mut found = Vec::new();
        for e in self.index.roads_near(t, path.frame, cx, cz, r) {
            if Some(e) == skip {
                continue;
            }
            let edge = &w.edges[e as usize];
            let (a, b) = (w.nodes[edge.a].pos, w.nodes[edge.b].pos);
            let (q0, q1) = (
                (i64::from(a.x), i64::from(a.z)),
                (i64::from(b.x), i64::from(b.z)),
            );
            if let Some((fd, fr)) = flight::crossing(t, p0, p1, q0, q1)
                && (ROAD_MARGIN..=1000 - ROAD_MARGIN).contains(&fr)
            {
                let tc = path.t0 + (path.t1.0 - path.t0.0) * u64::from(fd) / 1000;
                found.push((tc, e, fr));
            }
        }
        found.sort_unstable();
        for (tc, edge, frac_milli) in found.into_iter().take(MAX_CROSSINGS) {
            let (from, to) = (Tick(tc.0.saturating_sub(half)), tc + half);
            self.schedule(
                to,
                Ev::Cross {
                    d,
                    plan,
                    edge,
                    frac_milli,
                    from,
                    to,
                },
            );
        }
    }

    /// Settles at its lair: one `Hungry` event, and no leg (a visiting player finds it through
    /// the lair index).
    pub(crate) fn roost(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let plan = self.bump(d);
        let hunt_below = self.tun.hunt_below;
        let rest = self.tun.rest_ticks.max(1);
        let here = self.dragons[d].position(&self.topo, &self.lairs, at);
        let gated = self.oil_gated(d);
        let x = self.dragons.get_mut(d).expect("live");
        x.state = State::Roosting;
        // A dragon with a lair rests at it; one without rests where it landed (with no resting
        // place it would stand at the origin).
        x.rest = if x.lair.is_none() { Some(here) } else { None };
        x.bout = 0;
        let mut t = x.hungry_at(&self.tun, hunt_below, at);
        // A fire dragon also wakes when its reserve runs low (OG-4); one already low waits for
        // hunger, which weighs refuelling with food.
        let o = &self.tun.oil;
        if gated && x.oil_level(o, at) > o.refuel_below {
            t = t.min(x.oil_low_at(o, o.refuel_below, at));
        }
        let t = t.max(at + rest);
        // At rest it books nothing in the leg index: players find it through the lair.
        self.cancel_leg(w, d);
        self.track(d);
        self.schedule(t, Ev::Hungry { d, plan });
        self.visitors_at_lair(w, d, at);
    }

    /// Sends `d` to lair `l` to claim it (a migration ordered from outside: an admin command, a
    /// test), from `at`, which the caller has run the engine to. Ignored for an embodied dragon,
    /// which the plugin is flying.
    pub fn relocate(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick) {
        if !self.dragons.contains(d)
            || self.dragons[d].is_embodied()
            || l as usize >= self.lairs.len()
        {
            return;
        }
        self.release_claim(d, at);
        self.leave_lair(w, d, at);
        self.lairs[l as usize].plan += 1;
        self.lairs[l as usize].vacant_since = None;
        self.track_lair(l);
        self.fly(w, d, Purpose::Claim(l), at);
    }

    /// `d` no longer flies to claim a roost (it died on the way, or was sent elsewhere): a roost
    /// nobody lives in stands empty again and its respawn clock starts over, or it would wait for
    /// a claimant that never comes.
    fn release_claim(&mut self, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let (State::Flying(Purpose::Claim(l))
        | State::Embodied {
            purpose: Purpose::Claim(l),
            ..
        }
        | State::Leaving {
            purpose: Purpose::Claim(l),
            ..
        }) = x.state
        else {
            return;
        };
        let delay = self.tun.respawn_delay_days * TICKS_PER_DAY;
        // Read before changing: a change pins the lair's page until the next save.
        let lair = &self.lairs[l as usize];
        if !lair.occupants.is_empty() || lair.vacant_since.is_some() {
            return;
        }
        let lair = &mut self.lairs[l as usize];
        lair.vacant_since = Some(at);
        lair.plan += 1;
        let plan = lair.plan;
        self.track_lair(l);
        self.schedule(at + delay, Ev::Respawn { lair: l, plan });
    }

    // ---- choosing prey -----------------------------------------------------------------------------

    /// Picks the best prey and flies to it. `chain` counts hunts already made in this bout.
    pub(crate) fn decide(&mut self, w: &mut World, d: DragonId, at: Tick, chain: u8) {
        self.dragons.get_mut(d).expect("live").bout = chain;
        self.stats.decisions += 1;
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let stage = x.stage(&sp, at);
        if !stage.hunts() {
            // A hatchling is fed by its parents (abstractly): it stays full.
            let full = self.tun.full;
            self.dragons
                .get_mut(d)
                .expect("live")
                .set_fullness(full, at);
            self.dragons.get_mut(d).expect("live").set_oil(
                &self.tun.oil,
                self.tun.oil.capacity,
                at,
            );
            self.roost(w, d, at);
            return;
        }
        // Low on oil (OG-4, OG-5): fed, it flies to drink (with no oil in reach it waits for
        // hunger); hungry, refuelling is weighed with the best prey.
        let low = self.oil_low(d, at);
        let refuel = if low { self.refuel_choice(d, at) } else { None };
        let best = if low && self.dragons[d].fullness_at(&self.tun, at) >= self.tun.hunt_below {
            match refuel {
                Some((p, u)) => {
                    self.refuel_now(w, d, p, u, at);
                    return;
                }
                None => {
                    let plan = self.bump(d);
                    let t = self.dragons[d]
                        .hungry_at(&self.tun, self.tun.hunt_below, at)
                        .max(at + 1);
                    self.schedule(t, Ev::Hungry { d, plan });
                    return;
                }
            }
        } else {
            self.choose(w, d, at)
        };
        if let Some((p, u)) = refuel
            && best.is_none_or(|b| u >= b.2)
        {
            self.refuel_now(w, d, p, u, at);
            return;
        }
        match best {
            Some((t, kind, u)) => {
                let x = self.dragons.get_mut(d).expect("live");
                x.bout = chain + 1;
                if kind == PreyKind::Grudge {
                    x.last_grudge = Some(at);
                }
                let name = self.target_name(w, t);
                self.note(
                    at,
                    "hunt",
                    Some(d),
                    format!(
                        "hungry ({}), hunts {} at {name} (utility {u:.1})",
                        self.dragons[d].fullness_at(&self.tun, at),
                        kind.name()
                    ),
                );
                self.fly(w, d, Purpose::Hunt(t), at);
            }
            None => {
                // Once a day with nothing in range: a u8 that would overflow in 256 days.
                let x = self.dragons.get_mut(d).expect("live");
                x.fruitless = x.fruitless.saturating_add(1);
                self.stats.fruitless += 1;
                if chain > 0 {
                    self.fly(w, d, Purpose::Home, at);
                } else if !self.migrate(w, d, at) {
                    // Nothing worth it within range: rest and try again in a day.
                    let plan = self.bump(d);
                    self.schedule(at + TICKS_PER_DAY, Ev::Hungry { d, plan });
                }
            }
        }
    }

    /// The best target by utility: expected fullness × season weight, less flight cost and risk,
    /// bent by temperament (C25); a grudge adds its pull. Ties go to the lower kind, then index.
    fn choose(&self, w: &World, d: DragonId, at: Tick) -> Option<(Target, PreyKind, f64)> {
        let mut best: Option<(Target, PreyKind, f64)> = None;
        for (t, k, u) in self.weigh(w, d, at) {
            if u > 0.0 && best.is_none_or(|b| u > b.2) {
                best = Some((t, k, u));
            }
        }
        best
    }

    /// Every target `d` would weigh at `at`, with its utility, in the order weighed (grudges,
    /// villages, roads). For diagnostics; [`Dragons::choose`] takes the best positive one.
    pub fn weigh(&self, w: &World, d: DragonId, at: Tick) -> Vec<(Target, PreyKind, f64)> {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species);
        let (pos, _) = x.position(&self.topo, &self.lairs, at);
        let origin = match x.lair {
            Some(l) => self.lairs[l as usize].pos,
            None => pos,
        };
        let (ox, oz) = (i64::from(origin.x), i64::from(origin.z));
        let stage = x.stage(sp, at);
        let range = i64::from(sp.territory[stage.index()]) * (1 + i64::from(x.fruitless.min(3)));
        let season = self.tun.calendar.season_at(at);
        let temper = x.temperament();
        let need = f64::from((self.tun.full - x.fullness_at(&self.tun, at)).min(sp.appetite));
        let t = self.topo(origin.frame);
        let cost = |p: Pos| {
            flight::dist(t, (ox, oz), (i64::from(p.x), i64::from(p.z))) / 1000.0
                * f64::from(self.tun.distance_cost_per_kblock)
        };
        let mut all: Vec<(Target, PreyKind, f64)> = Vec::new();
        let mut offer = |t: Target, k: PreyKind, u: f64| all.push((t, k, u));
        // Grudges: the player it hates, at their home (P26). A starving dragon eats first, and
        // one that just went after a grudge hunts for food for a while.
        let starving = x.fullness_at(&self.tun, at) <= self.tun.starving;
        let cooling = x
            .last_grudge
            .is_some_and(|t| at.0 < t.0 + self.tun.grudge_cooldown_ticks);
        let grudges = if starving || cooling {
            Vec::new()
        } else {
            w.knowledge.sentiment_snapshot(x.actor, at)
        };
        // A district it sensed near oil (OG-8) it raids where it is.
        for (target, v) in grudges {
            if v > self.tun.grudge_below {
                continue;
            }
            let (goal, gpos) = if w.knowledge.actors().is_player(target) {
                let Some(home) = self.homes.home(target, at) else {
                    continue;
                };
                (
                    Target::Home {
                        player: target,
                        frame: home.pos.frame,
                        x: home.pos.x,
                        z: home.pos.z,
                    },
                    home.pos,
                )
            } else if let Some(&(id, p)) = self.districts.get(&target) {
                (
                    Target::District {
                        district: id,
                        frame: p.frame,
                        x: p.x,
                        z: p.z,
                    },
                    p,
                )
            } else {
                continue;
            };
            // A home in another dimension is out of reach, whatever its coordinates.
            if gpos.frame != origin.frame {
                continue;
            }
            let dist = flight::dist(t, (ox, oz), (i64::from(gpos.x), i64::from(gpos.z)));
            if dist > f64::from(sp.grudge_range) {
                continue;
            }
            let pull = f64::from(temper.weight(Trait::Aggressive, self.tun.grudge_weight * 100))
                / 100.0
                * f64::from(-i32::from(v))
                / 127.0;
            let w_season = f64::from(self.tun.season_weight(season, PreyKind::Grudge)) / 100.0;
            offer(
                goal,
                PreyKind::Grudge,
                pull * w_season + need * 0.1 - cost(gpos),
            );
        }
        // Villages' livestock.
        let w_village = f64::from(self.tun.season_weight(season, PreyKind::Village)) / 100.0;
        // Bold dragons fear a village's defenders less (C25): twice the risk at boldness 0,
        // none at 255. Its iron golems add to the risk (SINK.md §4.6).
        let bend = (2.0 - f64::from(temper.bold) / 128.0).max(0.0);
        for node in self.index.villages_near(t, origin.frame, ox, oz, range) {
            let n = &w.nodes[node as usize];
            let dist = flight::dist(t, (ox, oz), (i64::from(n.pos.x), i64::from(n.pos.z)));
            if dist > range as f64 {
                continue;
            }
            // A village that paid this dragon's lair tribute within a season is left alone.
            if x.lair.is_some_and(|l| self.pays(node, l, at)) {
                continue;
            }
            let fear = self.village_risk(w, node as usize) as f64 * bend;
            let food = self
                .village_food(w, node as usize, sp.village_share_milli)
                .min(need);
            if food <= 0.0 {
                continue;
            }
            offer(
                Target::Village { node },
                PreyKind::Village,
                food * w_village - fear - cost(n.pos),
            );
        }
        // Roads.
        let w_road = f64::from(self.tun.season_weight(season, PreyKind::Road)) / 100.0;
        for e in self.index.roads_near(t, origin.frame, ox, oz, range) {
            let edge = &w.edges[e as usize];
            let (a, b) = (w.nodes[edge.a].pos, w.nodes[edge.b].pos);
            let (qa, qb) = (
                (i64::from(a.x), i64::from(a.z)),
                (i64::from(b.x), i64::from(b.z)),
            );
            let f = flight::closest_on(t, qa, qb, (ox, oz), ROAD_MARGIN, 1000 - ROAD_MARGIN);
            let p = flight::lerp(t, qa, qb, f);
            let pt = Pos::new(a.frame, p.0 as i32, p.1 as i32);
            if flight::dist(t, (ox, oz), p) > range as f64 {
                continue;
            }
            let food = self
                .road_food(w, e as usize, sp.catch_milli, sp.caravan_share_milli)
                .min(need);
            if food <= 0.0 {
                continue;
            }
            offer(
                Target::Road {
                    edge: e,
                    x: pt.x,
                    z: pt.z,
                    frac_milli: f,
                },
                PreyKind::Road,
                food * w_road - cost(pt),
            );
        }
        // Wild herds (PLAN.md §10.2): where each will be when the dragon arrives, and the
        // animals it could take there. A herd it ate from lately it follows farther.
        if let Some(hd) = &self.wild {
            let w_herd = f64::from(self.tun.season_weight(season, PreyKind::Herd)) / 100.0;
            let (px, pz) = (i64::from(pos.x), i64::from(pos.z));
            for o in hd.prey_near(origin.frame, ox, oz, range, at, d.to_bits()) {
                let fly = flight::dist(t, (px, pz), (i64::from(o.pos.0), i64::from(o.pos.1)));
                let arrive = at + flight::flight_ticks(fly, sp.speed);
                let (hx, hz) = hd.position(o.herd, arrive);
                let food = (o.available as f64 * f64::from(o.food_each)).min(need);
                if food <= 0.0 {
                    continue;
                }
                let pt = Pos::new(origin.frame, hx as i32, hz as i32);
                offer(
                    Target::Herd {
                        herd: o.herd.to_bits(),
                        frame: origin.frame,
                        x: pt.x,
                        z: pt.z,
                    },
                    PreyKind::Herd,
                    food * w_herd - f64::from(self.tun.herd_risk) - cost(pt),
                );
            }
        }
        all
    }

    fn class_of_world(&self, w: &World, g: GoodId) -> PreyClass {
        let def = w.goods.get(g);
        self.tun
            .prey
            .class(&def.name, matches!(def.kind, GoodKind::Currency(_)))
    }

    /// Expected fullness from a raid on a village's livestock.
    fn village_food(&self, w: &World, node: usize, share_milli: i64) -> f64 {
        if !w.nodes[node].kind.is_storehouse() {
            return 0.0;
        }
        // The stock as last anchored: an estimate is enough to weigh, and it costs no copy.
        let Some(region) = w.markets.get(w.nodes[node].region) else {
            return 0.0;
        };
        let m = region.cell().model();
        let mut food = 0.0;
        for (g, _) in w.goods.iter() {
            if let PreyClass::Food {
                per_item,
                herd: true,
            } = self.class_of_world(w, g)
            {
                let avail = m.stock(g) as f64 * share_milli as f64 / 1000.0;
                food += avail * f64::from(per_item) / Item::World(g).unit() as f64;
            }
        }
        food
    }

    /// Expected fullness from a patrol of a road: the caravans on it (its travel time over the
    /// caravan period, each way), the chance each is caught, and the edible share of what each
    /// carries, from the flows' current rates.
    fn road_food(&self, w: &World, e: usize, catch_milli: i64, share_milli: i64) -> f64 {
        let mut total = 0.0;
        let d = w.edges[e].d as f64;
        for dir in &w.edges[e].dirs {
            let period = dir.lattice.current_period().max(1) as f64;
            // Caravans on the road at once: its walk over the departure period. The expected
            // cargo met is the walk times the rate whatever the period (bigger, rarer caravans
            // carry more each), so batching changes the odds, not the mean.
            let passing = d / period;
            let mut per_caravan = 0.0;
            for (g, p) in dir.goods.iter() {
                if let PreyClass::Food { per_item, .. } = self.class_of_world(w, *g) {
                    let cargo = p.current_rate() as f64 * period;
                    per_caravan += cargo * share_milli as f64 / 1000.0 * f64::from(per_item)
                        / Item::World(*g).unit() as f64;
                }
            }
            total += passing * catch_milli as f64 / 1000.0 * per_caravan;
        }
        total
    }

    fn target_name(&self, w: &World, t: Target) -> String {
        match t {
            Target::Road { edge, .. } => self.road_name(w, edge),
            Target::Village { node } => format!("village {}", w.nodes[node as usize].key),
            Target::Home { player, .. } => format!("the home of player {}", player.to_bits()),
            Target::Herd { herd, .. } => self
                .wild
                .as_ref()
                .and_then(|h| h.herd(HerdId::from_bits(herd)))
                .map_or_else(|| format!("herd {herd:#x}"), |x| x.name.clone()),
            Target::District { district, .. } => format!("district {district}"),
        }
    }

    fn road_name(&self, w: &World, edge: u32) -> String {
        let e = &w.edges[edge as usize];
        format!("road {}-{}", w.nodes[e.a].key, w.nodes[e.b].key)
    }

    /// Moves a starving dragon to an empty roost of its species, if one is in reach (a
    /// migration). Returns whether it left.
    fn migrate(&mut self, w: &mut World, d: DragonId, at: Tick) -> bool {
        let x = &self.dragons[d];
        if x.fruitless < self.tun.fruitless_before_move {
            return false;
        }
        let Some(l) = self.vacant_lair(d, at) else {
            return false;
        };
        self.stats.migrations += 1;
        let name = self.lairs[l as usize].name.clone();
        self.note(
            at,
            "migrate",
            Some(d),
            format!("finds nothing to eat; migrates to {name}"),
        );
        self.leave_lair(w, d, at);
        self.lairs[l as usize].plan += 1; // its respawn event is void: it is claimed
        self.lairs[l as usize].vacant_since = None;
        self.track_lair(l);
        self.fly(w, d, Purpose::Claim(l), at);
        true
    }

    /// The nearest roost of `d`'s species with nobody in it and nobody on the way. Looks up the
    /// lair cells within range (one range query per row of regions), never every lair, and reads
    /// only the vacant ones' rows ([`Dragons::vacant`]).
    pub(crate) fn vacant_lair(&self, d: DragonId, at: Tick) -> Option<LairId> {
        let x = &self.dragons[d];
        let (p, _) = x.position(&self.topo, &self.lairs, at);
        let range = self.tun.species(x.species).grudge_range;
        let r = i64::from(range);
        let (px, pz) = (i64::from(p.x), i64::from(p.z));
        let t = self.topo(p.frame);
        // A fire dragon weighs the lair's distance to oil with its own (OG-7): a lair with no oil
        // in reach comes after every one with some.
        let oily = self.oil_gated(d);
        let reach = i64::from(self.tun.oil.lair_reach);
        let mut best: Option<(f64, LairId)> = None;
        for (rx, rz) in self.regions_near(p.frame, Rect::around(px, pz, r)) {
            if let Some(ls) = self.lair_cells.get(&(p.frame, rx, rz)) {
                for &i in ls {
                    if !self.vacant.contains(&i) {
                        continue;
                    }
                    let l = &self.lairs[i as usize];
                    if l.species != x.species
                        || !l.occupants.is_empty()
                        || l.vacant_since.is_none()
                        || Some(i) == x.lair
                    {
                        continue;
                    }
                    let dd = flight::dist(t, (px, pz), (i64::from(l.pos.x), i64::from(l.pos.z)));
                    if dd > f64::from(range) {
                        continue;
                    }
                    let key = if oily {
                        let (lx, lz) = (i64::from(l.pos.x), i64::from(l.pos.z));
                        dd + self
                            .oil
                            .near(t, p.frame, lx, lz, reach)
                            .map_or(1e9, |n| n.dist)
                    } else {
                        dd
                    };
                    if best.is_none_or(|b| key.total_cmp(&b.0).then(i.cmp(&b.1)).is_lt()) {
                        best = Some((key, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }

    // ---- arriving, hunting, eating -----------------------------------------------------------------

    fn leg_end(&mut self, w: &mut World, infra: &mut dyn Infrastructure, d: DragonId, at: Tick) {
        let State::Flying(purpose) = self.dragons[d].state else {
            return;
        };
        match purpose {
            Purpose::Home => self.roost(w, d, at),
            Purpose::Claim(l) => {
                self.claim(w, d, l, at);
                self.roost(w, d, at);
            }
            Purpose::Hunt(t @ Target::Home { .. }) => {
                self.raid_home(w, infra, d, t, at);
            }
            Purpose::Hunt(t @ Target::District { .. }) => {
                self.raid_district(w, infra, d, t, at);
            }
            Purpose::Refuel { field, frame, x, z } => {
                self.refuel_arrive(w, infra, d, field, frame, x, z, at)
            }
            Purpose::Hunt(t) => {
                let sp = self.tun.species(self.dragons[d].species).clone();
                let dwell = match t {
                    Target::Road { .. } => sp.patrol_ticks,
                    _ => sp.dwell_ticks,
                };
                let until = at + dwell;
                let plan = self.bump(d);
                let pos = self.target_pos(w, t);
                self.dragons.get_mut(d).expect("live").state = State::Hunting { target: t, until };
                self.track(d);
                self.book_leg(w, d, flight::stay(pos, at, until, BODY_RADIUS), sp.cruise_y);
                self.schedule(until, Ev::HuntEnd { d, plan });
            }
        }
    }

    fn hunt_end(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let State::Hunting { target, .. } = self.dragons[d].state else {
            return;
        };
        self.stats.hunts += 1;
        let (food, taken, victims, kind) = match target {
            Target::Road { edge, .. } => {
                // A patrol: everything on the road when it ends.
                let (f, t, v) = self.hunt_road(w, d, edge, 0, Tick::ZERO, Tick(u64::MAX), at);
                (f, t, v, PreyKind::Road)
            }
            Target::Village { node } => {
                let (f, t, v) = self.hunt_village(w, d, node as usize, at);
                (f, t, v, PreyKind::Village)
            }
            Target::Home { .. } | Target::District { .. } => {
                (0, Vec::new(), Vec::new(), PreyKind::Grudge)
            }
            Target::Herd { herd, x, z, .. } => {
                let need = {
                    let x = &self.dragons[d];
                    let sp = self.tun.species(x.species);
                    (self.tun.full - x.fullness_at(&self.tun, at)).min(sp.appetite)
                };
                let p = self.wild.as_mut().map_or_else(Default::default, |h| {
                    h.predate(
                        HerdId::from_bits(herd),
                        i64::from(x),
                        i64::from(z),
                        at,
                        need,
                        d.to_bits(),
                    )
                });
                let taken = match p.species {
                    Some(s) if p.animals > 0 => {
                        self.stats.herd_hunts += 1;
                        vec![(s.entity_id().to_string(), p.animals as i64)]
                    }
                    _ => Vec::new(),
                };
                (p.food, taken, Vec::new(), PreyKind::Herd)
            }
        };
        self.feed(d, at, food, &victims);
        let name = self.target_name(w, target);
        let quarry = self.quarry(w, target, kind, name);
        self.record_hunt(at, d, quarry, food, taken);
        self.after_hunt(w, d, at, food);
    }

    /// After a hunt: home if fed enough, else another target (a bout chains at most
    /// `bout_hunts`).
    fn after_hunt(&mut self, w: &mut World, d: DragonId, at: Tick, food: i32) {
        let x = self.dragons.get_mut(d).expect("live");
        if food > 0 {
            x.fruitless = 0;
        } else {
            x.fruitless = x.fruitless.saturating_add(1);
            self.stats.fruitless += 1;
        }
        let x = &self.dragons[d];
        let hungry = x.fullness_at(&self.tun, at) < self.tun.hunt_below;
        if hungry && x.bout < self.tun.bout_hunts {
            let chain = x.bout;
            self.decide(w, d, at, chain);
        } else {
            self.fly(w, d, Purpose::Home, at);
        }
    }

    pub(crate) fn feed(&mut self, d: DragonId, at: Tick, food: i32, victims: &[ActorId]) {
        // Nothing to change: leave the row (and its page) clean.
        if food <= 0 && victims.is_empty() {
            return;
        }
        let keep = self.tun.victims_kept;
        let tun = self.tun.clone();
        let x = self.dragons.get_mut(d).expect("live");
        x.eat(&tun, food, at);
        for v in victims {
            x.victims.retain(|o| o != v);
            x.victims.push(*v);
            if x.victims.len() > keep {
                x.victims.remove(0);
            }
        }
    }

    /// The archive's description of a hunt on `target`: its place, and its id where it has one.
    fn quarry(&self, w: &World, target: Target, kind: PreyKind, name: String) -> Quarry {
        let id = match target {
            Target::Herd { herd, .. } => u128::from(herd),
            Target::Village { node } => u128::from(w.nodes[node as usize].key),
            Target::Home { player, .. } => aid(player),
            Target::Road { .. } => 0,
            Target::District { district, .. } => u128::from(district),
        };
        Quarry {
            kind,
            name,
            pos: Some(self.target_pos(w, target)),
            id,
        }
    }

    pub(crate) fn record_hunt(
        &mut self,
        at: Tick,
        d: DragonId,
        quarry: Quarry,
        food: i32,
        taken: Vec<(String, i64)>,
    ) {
        let Quarry {
            kind,
            name: target,
            pos,
            id,
        } = quarry;
        let season = self.tun.calendar.season_at(at);
        if food > 0 {
            self.prey_by_season[season.index()][kind.index()] += 1;
            self.food_by_season[season.index()][kind.index()] += i64::from(food);
        }
        if self.tape.is_on() {
            let here =
                pos.unwrap_or_else(|| self.dragons[d].position(&self.topo, &self.lairs, at).0);
            self.tape
                .event(
                    kinds::DRAGONS_HUNT,
                    spot(at, here),
                    did(d),
                    id,
                    i64::from(food),
                )
                .text(|| {
                    format!(
                        "{} at {target}: +{food} fullness{}",
                        kind.name(),
                        if taken.is_empty() {
                            String::new()
                        } else {
                            format!(", took {}", items_text(&taken))
                        }
                    )
                });
        }
        if !self.log_enabled {
            return;
        }
        let dragon = self.dragons[d].name.clone();
        self.note(
            at,
            "prey",
            Some(d),
            format!(
                "{} at {target}: +{food} fullness{}",
                kind.name(),
                if taken.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", took {}",
                        taken
                            .iter()
                            .map(|(n, a)| format!("{n} {a}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ),
        );
        self.hunts.push(HuntRecord {
            tick: at.0,
            day: day_of(at),
            season,
            dragon,
            kind,
            target,
            food,
            taken,
        });
    }

    /// Takes goods into the ledger by class; returns fullness gained and the names taken.
    fn take_world(
        &mut self,
        w: &World,
        g: GoodId,
        amount: i64,
        out: &mut Vec<(String, i64)>,
    ) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let class = self.class_of_world(w, g);
        out.push((w.goods.get(g).name.clone(), amount));
        self.ledger.take(Item::World(g), amount, class)
    }

    pub(crate) fn hoard(&mut self, d: DragonId, item: Item, amount: i64) {
        if amount <= 0 {
            return;
        }
        // A dragon with no lair yet carries it until it claims one.
        match self.dragons[d].lair {
            Some(l) => *self.lairs[l as usize].hoard.entry(item).or_default() += amount,
            None => {
                *self
                    .dragons
                    .get_mut(d)
                    .expect("live")
                    .carried
                    .entry(item)
                    .or_default() += amount
            }
        }
    }

    /// Caravans passing the point `frac_milli` along `edge` during `[from, to]`, each caught
    /// with the species' chance (one counter-RNG draw keyed by the dragon and the caravan): the
    /// hazard on the flow, integrated over the hunt (SCALE.md §3). Caught ones become rows and
    /// lose a share of their cargo (rtsim-world's `prey_on_traveller`).
    #[allow(clippy::too_many_arguments)]
    fn hunt_road(
        &mut self,
        w: &mut World,
        d: DragonId,
        edge: u32,
        frac_milli: u32,
        from: Tick,
        to: Tick,
        at: Tick,
    ) -> (i32, Vec<(String, i64)>, Vec<ActorId>) {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let key = StreamKey::new(self.seed, SYS_HUNT, x.seed);
        let mut food = 0;
        let mut taken = Vec::new();
        let mut victims = Vec::new();
        let _ = at;
        for dir in 0..2u8 {
            let f = if dir == 0 {
                u64::from(frac_milli)
            } else {
                1000 - u64::from(frac_milli)
            };
            for t in w.travellers(edge, dir) {
                if t.cargo.values().all(|&a| a <= 0) {
                    continue;
                }
                // A crossing catches what passes its point in its window; a patrol (the whole
                // window) catches everything on the road.
                if (from, to) != (Tick::ZERO, Tick(u64::MAX)) {
                    let pass = t.departed + (t.arrives.0.saturating_sub(t.departed.0)) * f / 1000;
                    if pass < from || pass > to {
                        continue;
                    }
                }
                let k = t.key;
                let slot = fnv1a64(
                    &[
                        u64::from(k.edge).to_le_bytes(),
                        u64::from(k.dir).to_le_bytes(),
                        k.departure.to_le_bytes(),
                    ]
                    .concat(),
                );
                if key.slot(slot).below(1000) >= sp.catch_milli.clamp(0, 1000) as u64 {
                    continue;
                }
                let Some((_, lost)) = w.prey_on_traveller(k, sp.caravan_share_milli) else {
                    continue;
                };
                self.stats.caravans_attacked += 1;
                for (g, a) in lost {
                    let gained = self.take_world(w, g, a, &mut taken);
                    if matches!(self.class_of_world(w, g), PreyClass::Valuable) {
                        self.hoard(d, Item::World(g), a);
                    }
                    food += gained;
                }
                let home = w.nodes[t.from].actor;
                if !victims.contains(&home) {
                    victims.push(home);
                }
            }
        }
        (food, taken, victims)
    }

    /// A raid on a village's livestock: up to the species' share of each herd good, no more than
    /// it can eat. The village sees it (an assault on it by the dragon).
    fn hunt_village(
        &mut self,
        w: &mut World,
        d: DragonId,
        node: usize,
        at: Tick,
    ) -> (i32, Vec<(String, i64)>, Vec<ActorId>) {
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let mut need = (self.tun.full - x.fullness_at(&self.tun, at)).min(sp.appetite);
        let mut food = 0;
        let mut taken = Vec::new();
        if !w.nodes[node].kind.is_storehouse() {
            return (0, taken, Vec::new());
        }
        let m = w.market(node);
        let goods: Vec<GoodId> = w.goods.ids().collect();
        for g in goods {
            if need <= 0 {
                break;
            }
            let PreyClass::Food {
                per_item,
                herd: true,
            } = self.class_of_world(w, g)
            else {
                continue;
            };
            let stock = m.stock(g);
            let unit = Item::World(g).unit();
            let cap = stock * sp.village_share_milli / 1000;
            let per = i64::from(per_item.max(1));
            let want = (i64::from(need) * unit + per - 1) / per;
            let t = w.take_stock(node, g, cap.min(want), at);
            let gained = self.take_world(w, g, t, &mut taken);
            food += gained;
            need -= gained;
        }
        if food > 0 {
            self.stats.village_raids += 1;
            let village = w.nodes[node].actor;
            let (p, y) = (w.nodes[node].pos, 64);
            let id = w.knowledge.record(Report {
                who: self.dragons[d].actor,
                whom: village,
                kind: ReportKind::Assault,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: place(p, y),
            });
            w.knowledge.witness(village, id, at);
            self.raided(w, d, node, at);
            return (food, taken, vec![village]);
        }
        (food, taken, Vec::new())
    }

    /// A grudge raid: the districts with a chunk at the hated player's home (OGC50), through the
    /// industry threat interface. A watched one is raided too, and fights for real: the dragon
    /// materializes there (P28, OGC56).
    fn raid_home(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        t: Target,
        at: Tick,
    ) {
        let Target::Home {
            player,
            frame,
            x,
            z,
        } = t
        else {
            return;
        };
        let sites = infra.districts_reaching(frame, i64::from(x), i64::from(z), HOME_RADIUS);
        let r = self.raid_sites(infra, d, sites, at, true);
        let quarry = |pos: Pos| Quarry {
            kind: PreyKind::Grudge,
            name: format!("the home of player {}", player.to_bits()),
            pos: Some(pos),
            id: aid(player),
        };
        if let Some(site) = r.watched {
            // The player is there: a real fight. Materialize at the district, with what it
            // already ate at unwatched districts on the way (the ledger has it).
            if r.any {
                self.feed(d, at, r.food, &[]);
                self.record_hunt(at, d, quarry(site.pos), r.food, r.taken);
            }
            let cruise = self.tun.species(self.dragons[d].species).cruise_y;
            self.materialize(w, d, player, true, false, Some((site.pos, cruise)), at);
            return;
        }
        self.feed(d, at, r.food, &[]);
        self.record_hunt(at, d, quarry(Pos::new(frame, x, z)), r.food, r.taken);
        if !r.any {
            let x = self.dragons.get_mut(d).expect("live");
            x.fruitless = x.fruitless.saturating_add(1);
        }
        self.fly(w, d, Purpose::Home, at);
    }

    /// Raids `sites` in order, at most `raid_districts` of them, through the threat interface;
    /// stops at the first watched one, raided too (P28, OGC56), where the caller materializes it
    /// when it comes in person: always if `always_in_person` (the hated player is the target),
    /// else when a player was last seen within two view radii of it ([`Dragons::player_near`]).
    pub(crate) fn raid_sites(
        &mut self,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        sites: Vec<DistrictSite>,
        at: Tick,
        always_in_person: bool,
    ) -> Raided {
        let mut r = Raided::default();
        for site in sites.into_iter().take(self.tun.raid_districts) {
            let in_person = site.watched
                && (always_in_person
                    || self
                        .player_near(site.pos, 2 * self.tun.view_radius)
                        .is_some());
            if let Some(gained) = self.raid_site(infra, d, site, &mut r.taken, at, in_person) {
                r.any = true;
                r.food += gained;
            }
            if site.watched {
                r.watched = Some(site);
                break;
            }
        }
        r
    }

    /// One district raided: a threat of the dragon's passes. A fire dragon's passes burn oil,
    /// those beyond its reserve strike without fire (OG-13), and its theft drinks fuel first, up
    /// to the reserve's room (OG-5). Defences that stopped passes wound it. A watched district is
    /// raided the same way at once, blocks and theft (P28 as Ben decided it, OGC56). `in_person`,
    /// the dragon is about to fight there for real: its real breaths burn its oil and real blows
    /// wound it, so the raid does neither (OGC57).
    fn raid_site(
        &mut self,
        infra: &mut dyn Infrastructure,
        d: DragonId,
        site: DistrictSite,
        taken: &mut Vec<(String, i64)>,
        at: Tick,
        in_person: bool,
    ) -> Option<i32> {
        let x0 = &self.dragons[d];
        let sp = self.tun.species(x0.species).clone();
        let stage = x0.stage(&sp, at);
        let passes = sp.passes[stage.index()].max(1);
        let op = self.oil_passes(d, passes, at);
        let threat = Threat {
            id: fnv1a64(
                &[
                    self.dragons[d].seed.to_le_bytes(),
                    at.0.to_le_bytes(),
                    site.id.0.to_le_bytes(),
                ]
                .concat(),
            ),
            source: ThreatSource::Dragon,
            class: ThreatClass::Flying,
            count: passes,
            strength: f64::from(passes) * sp.strength_per_pass[stage.index()] * op.factor,
            target: ThreatTarget::District(site.id),
            loads: None,
            at,
        };
        let o = if site.watched {
            infra.raid_in_sight(&threat, &self.tun.oil.fuels, op.room_mb.max(0))
        } else if op.room_mb > 0 {
            infra.raid_drinking(&threat, &self.tun.oil.fuels, op.room_mb)
        } else {
            infra.raid(&threat)
        }?;
        self.stats.district_raids += 1;
        if o.skipped.is_none() {
            if op.burn > 0 && !in_person {
                self.dragons
                    .get_mut(d)
                    .expect("live")
                    .burn_oil(&self.tun.oil, op.burn, at);
            }
            if op.dry {
                self.stats.dry_raids += 1;
            }
        }
        let drinking = op.room_mb > 0;
        let mut gained = 0;
        let mut drunk = 0i64;
        let first = taken.len();
        for (g, n) in o.stolen.iter() {
            let name = infra
                .good_name(g)
                .unwrap_or_else(|| format!("good {}", g.0));
            let (class, fuel) = self.stolen_class(&name, drinking);
            gained += self.ledger.take(Item::Industry(g), n, class);
            if fuel {
                drunk += n;
            } else if matches!(class, PreyClass::Valuable) {
                self.hoard(d, Item::Industry(g), n);
            }
            taken.push((name, n));
        }
        self.drink(d, drunk, at);
        let loot = &taken[first..];
        let stolen: i64 = loot.iter().map(|(_, n)| *n).sum();
        // A raid that took nothing and broke nothing is a minor row.
        let severity = if stolen > 0 || o.destroyed_blocks > 0 {
            Severity::Major
        } else {
            Severity::Minor
        };
        self.tape
            .event_as(
                kinds::DRAGONS_RAID,
                severity,
                spot(at, site.pos),
                did(d),
                u128::from(site.id.0),
                stolen,
            )
            .text(|| {
                format!(
                    "{} blocks destroyed, took {}{}",
                    o.destroyed_blocks,
                    if loot.is_empty() {
                        "nothing".to_string()
                    } else {
                        items_text(loot)
                    },
                    o.skipped
                        .map_or(String::new(), |s| format!(" (skipped: {s})"))
                )
            });
        // An abstract raid never kills a dragon (a death needs a real fight), so it keeps at
        // least a tenth of its health. In person, the real fight wounds it instead.
        if !in_person {
            let max = self.dragons[d].max_health(&sp, at);
            let h = self.dragons[d].health_at(&sp, at);
            let wound =
                max * (self.tun.raid_wound_milli as f32 / 1000.0) * (1.0 - o.survived as f32);
            self.dragons
                .get_mut(d)
                .expect("live")
                .set_health((h - wound).max(max * 0.1), at);
        }
        self.note(
            at,
            "raid",
            Some(d),
            format!(
                "raids district {} ({}): survived {:.2}{}{}, {} blocks destroyed{}{}{}",
                site.id.0,
                match (site.watched, in_person) {
                    (true, true) => "watched: it comes in person too",
                    (true, false) => "watched, nobody near: abstract only",
                    _ => "unwatched",
                },
                o.survived,
                o.floor.map_or(String::new(), |f| format!(
                    ", floored by an observed fight at {f:.2}"
                )),
                if o.untested {
                    ", untested defences"
                } else {
                    ""
                },
                o.destroyed_blocks,
                if op.dry {
                    ", some passes without fire"
                } else {
                    ""
                },
                if drunk > 0 {
                    format!(", drank {drunk} mB")
                } else {
                    String::new()
                },
                o.skipped
                    .map_or(String::new(), |s| format!(", skipped: {s}"))
            ),
        );
        Some(gained)
    }

    // ---- the boundary: materializing and what comes back -----------------------------------------

    fn seen(&mut self, w: &mut World, d: DragonId, player: ActorId, m: Meeting, at: Tick) {
        if !w.legs.is_meeting_live(&m) || !self.dragons.contains(d) {
            self.stats.seen_stale += 1;
            return;
        }
        if self.dragons[d].is_embodied() {
            return;
        }
        self.stats.seen += 1;
        let grudge =
            w.knowledge.sentiment(self.dragons[d].actor, player, at) <= self.tun.grudge_below;
        // It harasses when it hunts here: at its prey, or flying to prey near the player.
        let near = |tp: Pos| {
            self.homes.last_seen(player).is_some_and(|(p, _)| {
                flight::dist(
                    self.topo(p.frame),
                    (i64::from(p.x), i64::from(p.z)),
                    (i64::from(tp.x), i64::from(tp.z)),
                ) <= f64::from(2 * self.tun.view_radius)
            })
        };
        // A field flight's dragon harasses whoever it meets (In Control's are hostile).
        let visitor = self.dragons[d].visitor.is_some();
        let harass = grudge
            || visitor
            || match self.dragons[d].state {
                State::Hunting { .. } => true,
                State::Flying(Purpose::Hunt(Target::Home { player: p, .. })) => p == player,
                State::Flying(Purpose::Hunt(t)) => near(self.target_pos(w, t)),
                // Flying to drink at a field near the player: it comes in person (OGC19).
                State::Flying(Purpose::Refuel { frame, x, z, .. }) => near(Pos::new(frame, x, z)),
                _ => false,
            };
        self.materialize(w, d, player, harass, visitor, None, at);
    }

    /// Makes the dragon a Minecraft entity near `player`: the plugin spawns it with its NBT from
    /// engine state. A harassing dragon books a departure for when it gets bored (C24); one
    /// passing through books its time to cross the view; one at home stays while it is seen.
    /// `hostile` makes it attack `player` on sight without a grudge (a field flight's dragon, a
    /// raid on a district it hates). A fire dragon's body carries its oil reserve (MAT_OIL).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn materialize(
        &mut self,
        w: &mut World,
        d: DragonId,
        player: ActorId,
        harass: bool,
        hostile: bool,
        at_pos: Option<(Pos, i32)>,
        at: Tick,
    ) {
        let gated = self.oil_gated(d);
        let (pos, y) =
            at_pos.unwrap_or_else(|| self.dragons[d].position(&self.topo, &self.lairs, at));
        // Near a seam the body appears at the image on the player's side (TP11); across an
        // edge_stop seam it does not appear at all (TP12).
        let t = self.topo(pos.frame);
        let here = (i64::from(pos.x), i64::from(pos.z));
        // Where the player is: its interest area's centre, else where it was last seen.
        let viewer = self
            .players
            .get(&player)
            .and_then(|v| v.interest)
            .and_then(|st| w.legs.get(st.leg))
            .filter(|r| r.path.frame == pos.frame)
            .map(|r| (i64::from(r.path.x0), i64::from(r.path.z0)))
            .or_else(|| {
                self.homes
                    .last_seen(player)
                    .filter(|(p, _)| p.frame == pos.frame)
                    .map(|(p, _)| (i64::from(p.x), i64::from(p.z)))
            });
        let spawn = match viewer {
            Some(v) => t.embodied_toward(v, here),
            None => t.embodied_toward(here, here),
        };
        let Some(spawn) = spawn else {
            self.stats.seen_across_stop += 1;
            self.note(
                at,
                "materialize",
                Some(d),
                format!(
                    "stays abstract for player {}: across an edge_stop seam",
                    player.to_bits()
                ),
            );
            return;
        };
        let x = &self.dragons[d];
        let sp = self.tun.species(x.species).clone();
        let purpose = match x.state {
            State::Flying(p) => p,
            State::Hunting { target, .. } => Purpose::Hunt(target),
            _ => Purpose::Home,
        };
        let home = matches!(x.state, State::Roosting);
        let grudge = w.knowledge.sentiment(x.actor, player, at) <= self.tun.grudge_below;
        let temper = x.temperament();
        let starving = x.fullness_at(&self.tun, at) <= self.tun.starving;
        let depart_at = if home {
            Tick(u64::MAX)
        } else if harass && starving {
            // Hunger brings the departure forward (C24): a starving dragon only looks in.
            at + (sp.visit_ticks / 10).max(1)
        } else if harass {
            at + temper
                .weight(
                    Trait::Aggressive,
                    sp.visit_ticks.min(i32::MAX as u64) as i32,
                )
                .max(1) as u64
        } else {
            at + flight::flight_ticks(f64::from(2 * self.tun.view_radius), sp.speed)
        };
        let epoch = x.epoch + 1;
        let lair = x.lair.map(|l| &self.lairs[l as usize]);
        // The body's own frame: where it heads and where its home is, the short way from where
        // it appears and never through a stopped seam.
        let local = |p: Pos| t.embodied_route(spawn, (i64::from(p.x), i64::from(p.z)));
        let (target, _) = if harass {
            match self.homes.last_seen(player) {
                Some((p, _)) => (p, y),
                None => (pos, y),
            }
        } else {
            self.purpose_pos(w, d, purpose)
        };
        let mut flags = boundary::MAT_CLAMP_ABOVE_SURFACE;
        if lair.is_some() {
            flags |= boundary::MAT_HAS_HOME;
        }
        if home {
            flags |= boundary::MAT_SLEEPING;
        }
        if grudge || hostile {
            flags |= boundary::MAT_HOSTILE;
        }
        let oil = if gated {
            flags |= boundary::MAT_OIL;
            x.oil_level(&self.tun.oil, at)
        } else {
            0
        };
        let cmd = boundary::Materialize {
            agent: agent_of(d),
            epoch,
            flags,
            species: x.species,
            dimension: u32::from(pos.frame),
            age_ticks: x.age(at).min(i32::MAX as u64) as i32,
            pos: (spawn.0 as f64, f64::from(y), spawn.1 as f64),
            hunger: x.fullness_at(&self.tun, at),
            gender: x.gender,
            variant: u32::from(x.variant),
            health: x.health_at(&sp, at),
            home: lair.map_or((0, 0, 0), |l| {
                let h = local(l.pos);
                (h.0 as i32, l.y, h.1 as i32)
            }),
            target: {
                let g = local(target);
                (g.0 as f64, f64::from(sp.cruise_y), g.1 as f64)
            },
            target_player: if grudge || hostile {
                self.uuid_of(player)
            } else {
                (0, 0)
            },
            oil,
        };
        self.outbox.push(Cmd::Materialize(cmd));
        self.stats.materialized += 1;
        let health = x.health_at(&sp, at);
        let plan = self.bump(d);
        {
            let x = self.dragons.get_mut(d).expect("live");
            x.epoch = epoch;
            x.set_health(health, at);
            x.state = State::Embodied {
                epoch,
                since: at,
                depart_at,
                purpose,
                harass,
            };
            x.rest = Some((pos, y));
            if grudge {
                x.last_grudge = Some(at);
            }
        }
        self.track(d);
        self.cancel_leg(w, d);
        let what = format!(
            "enters view of player {}: materializes ({}{})",
            player.to_bits(),
            if home {
                "at home"
            } else if harass {
                "to harass"
            } else {
                "passing"
            },
            if grudge { ", hostile: a grudge" } else { "" }
        );
        self.note(at, "materialize", Some(d), what);
        if depart_at.0 != u64::MAX {
            let reason = if harass && starving {
                2
            } else if harass {
                1
            } else {
                4
            };
            self.schedule(depart_at, Ev::Depart { d, plan, reason });
        }
    }

    /// Tells the plugin to fly the dragon off (C24: it got bored, or wounds or hunger brought
    /// the departure forward). The plugin captures it when it is out of view.
    fn depart(&mut self, d: DragonId, reason: u32, at: Tick) {
        let x = &self.dragons[d];
        let State::Embodied {
            epoch,
            purpose,
            since,
            ..
        } = x.state
        else {
            return;
        };
        let (to, _) = match x.lair {
            Some(l) => (self.lairs[l as usize].pos, self.lairs[l as usize].y),
            None => x.position(&self.topo, &self.lairs, at),
        };
        // Toward home the short way from where the body is, round any stopped seam (TP12).
        let (here, _) = x.position(&self.topo, &self.lairs, at);
        let to = self.topo(to.frame).embodied_route(
            (i64::from(here.x), i64::from(here.z)),
            (i64::from(to.x), i64::from(to.z)),
        );
        let cruise = self.tun.species(x.species).cruise_y;
        self.outbox.push(Cmd::Depart(boundary::Depart {
            agent: agent_of(d),
            epoch,
            reason,
            timeout_ticks: DEPART_TIMEOUT,
            target: (to.0 as f64, f64::from(cruise), to.1 as f64),
        }));
        self.stats.departures += 1;
        if reason == 1 {
            self.stats.bored += 1;
        }
        let why = [
            "",
            "bored",
            "hungry",
            "wounded",
            "passing through",
            "recalled",
        ]
        .get(reason as usize)
        .copied()
        .unwrap_or("?");
        self.tape
            .event(
                kinds::DRAGONS_DEPART,
                spot(at, here),
                did(d),
                0,
                i64::from(reason),
            )
            .text(|| {
                format!(
                    "leaves ({why}) after {:.2} days in view",
                    day_of(Tick(at.0 - since.0))
                )
            });
        self.note(
            at,
            "depart",
            Some(d),
            format!(
                "leaves ({why}) after {:.2} days in view",
                day_of(Tick(at.0 - since.0))
            ),
        );
        self.bump(d);
        self.dragons.get_mut(d).expect("live").state = State::Leaving { epoch, purpose };
        self.track(d);
    }

    pub(crate) fn embodied(&self, agent: u64, epoch: u32) -> Option<DragonId> {
        let d = dragon_of(agent)?;
        let x = self.dragons.get(d)?;
        match x.state {
            State::Embodied { epoch: e, .. } | State::Leaving { epoch: e, .. } if e == epoch => {
                Some(d)
            }
            _ => None,
        }
    }

    /// A session starts: puppets never outlive one (docs/boundary.md §2.3), so every dragon the
    /// engine thought embodied is abstract again where it was last seen, and carries on.
    pub fn session_start(&mut self, w: &mut World, at: Tick) {
        let embodied: Vec<DragonId> = self.embodied.iter().copied().collect();
        for d in embodied {
            self.resume(w, d, at, true);
        }
        self.by_entity.clear();
        self.chests.clear();
    }

    /// An observation from the plugin, at `at`.
    pub fn ingest(&mut self, w: &mut World, infra: &mut dyn Infrastructure, o: &Obs, at: Tick) {
        let _ = infra;
        self.run_until(w, infra, at);
        match o {
            Obs::LairFound(l) => {
                let pos = self.canon(l.dimension, f64::from(l.home.0), f64::from(l.home.2));
                let name = format!(
                    "{} {}",
                    boundary::SpeciesDict::structure_name(l.species, l.kind),
                    l.lair
                );
                let new = !self.lair_by_key.contains_key(&l.lair);
                self.add_lair(w, l.lair, &name, l.species, l.kind, pos, l.home.1, at);
                // A new fire lair asks for the oil around it before its chunks generate (OG-2).
                if new && self.tun.species(l.species).uses_oil {
                    self.survey(l.dimension, pos.x, pos.z);
                }
            }
            Obs::Untagged(u) => self.untagged(w, u, at),
            Obs::Materialized(m) => {
                if let Some(d) = self.embodied(m.agent, m.epoch) {
                    self.by_entity.insert(m.entity, d);
                }
            }
            Obs::SpawnRejected(r) => {
                if let Some(d) = self.embodied(r.agent, r.epoch) {
                    self.resume(w, d, at, false);
                }
            }
            Obs::State(s) => {
                let Some(d) = self.embodied(s.agent, s.epoch) else {
                    return;
                };
                let (starving, full) = (self.tun.starving, self.tun.full);
                let here = self.canon(s.dimension, s.pos.0, s.pos.2);
                let x = self.dragons.get_mut(d).expect("live");
                x.set_health(s.health, at);
                // Ice and Fire's Hunger is 0 to 100; anything else from the plugin is clamped.
                x.set_fullness(s.hunger.clamp(0, full), at);
                x.rest = Some((here, s.pos.1 as i32));
                if s.hunger <= starving && matches!(x.state, State::Embodied { .. }) {
                    self.depart(d, 2, at);
                }
            }
            Obs::Hurt(h) => self.hurt(w, h, at),
            Obs::Died(dd) => {
                if let Some(d) = self.embodied(dd.agent, dd.epoch) {
                    let killer =
                        (dd.killer_kind == AttackerKind::Player).then(|| self.player(w, dd.killer));
                    let pos = (
                        self.canon(dd.dimension, dd.pos.0, dd.pos.2),
                        dd.pos.1 as i32,
                    );
                    self.kill(w, d, killer, pos, at);
                }
            }
            Obs::Dematerialized(m) => {
                let Some(d) = self.embodied(m.agent, m.epoch) else {
                    return;
                };
                {
                    let full = self.tun.full;
                    let here = self.canon(m.dimension, m.pos.0, m.pos.2);
                    let x = self.dragons.get_mut(d).expect("live");
                    x.set_health(m.health, at);
                    x.set_fullness(m.hunger.clamp(0, full), at);
                    x.rest = Some((here, m.pos.1 as i32));
                }
                self.by_entity.retain(|_, v| *v != d);
                self.adopted.retain(|_, v| *v != d);
                let leaving = matches!(self.dragons[d].state, State::Leaving { .. });
                let what = format!("captured (health {:.0}, hunger {})", m.health, m.hunger);
                self.note(at, "capture", Some(d), what);
                self.resume(w, d, at, leaving);
            }
            Obs::HoardRobbed(h) => self.robbed(w, h, at),
            Obs::Ate(a) => self.ate(w, a, at),
            Obs::OilChunks(o) => self.oil_chunks(o, at),
            Obs::Breaths(b) => self.breaths(b, at),
            Obs::OilConfig(c) => {
                let was = self.oil_flags;
                self.oil_flags = c.flags;
                self.nests_config(was, at);
            }
            Obs::NestFound(f) => self.nest_found(f, at),
            Obs::NestCensus(c) => self.nest_census(c, at),
            Obs::NestLoss(l) => self.nest_loss(l, at),
            Obs::NestHeart(h) => self.nest_heart(h, at),
            Obs::LairChest(c) => {
                if let Some(l) = self.lair_of_key(c.lair) {
                    let chest = Chest {
                        dimension: c.dimension,
                        pos: [c.pos.0, c.pos.1, c.pos.2],
                    };
                    self.chests.insert(l, chest);
                    self.place_tribute(l);
                }
            }
        }
    }

    /// A watched dragon ate animals. Near a village they were its herd: the village's stock
    /// falls by the count, through the books like an abstract raid (P28: consequences through
    /// stock and prices), and the dragon's fullness follows Ice and Fire's food points.
    fn ate(&mut self, w: &mut World, a: &boundary::Ate, at: Tick) {
        let Some(d) = self.embodied(a.agent, a.epoch) else {
            return;
        };
        let Some(&g) = self.herd_bodies.get(&a.prey) else {
            return;
        };
        let Some(node) = (a.village != 0).then(|| w.node_of_key(a.village)).flatten() else {
            return;
        };
        let want = i64::from(a.count) * Item::World(g).unit();
        let taken = w.take_stock(node, g, want, at);
        let mut names = Vec::new();
        let food = self.take_world(w, g, taken, &mut names);
        let village = w.nodes[node].actor;
        self.feed(d, at, food, &[village]);
        if taken > 0 {
            self.stats.village_raids += 1;
            let quarry = Quarry {
                kind: PreyKind::Village,
                name: format!("village {} (watched)", w.nodes[node].key),
                pos: Some(w.nodes[node].pos),
                id: u128::from(w.nodes[node].key),
            };
            self.record_hunt(at, d, quarry, food, names);
            self.raided(w, d, node, at);
        }
    }

    // ---- tribute (SINK.md §4.6) ------------------------------------------------------------------

    /// Dragon `d` raided village `node`. A lair dragon's raid makes the village its lair's
    /// tributary; a lairless dragon's only frightens it (golems first for a season).
    fn raided(&mut self, w: &mut World, d: DragonId, node: usize, at: Tick) {
        match self.dragons[d].lair {
            Some(l) => {
                self.tribute_begin(w, node, l, at);
            }
            None => w.village_raided(node, false, at),
        }
    }

    /// Village `node` becomes lair `lair`'s tributary: a new row (replacing another lair's) and
    /// the first payment at once. A village already paying this lair keeps its row and its
    /// season's clock. A village without a works purse pays nothing and keeps no row. Returns
    /// whether the village pays the lair afterwards.
    fn tribute_begin(&mut self, w: &mut World, node: usize, lair: LairId, at: Tick) -> bool {
        w.village_raided(node, true, at);
        let Ok(n) = u32::try_from(node) else {
            return false;
        };
        if self.tribute.get(&n).is_some_and(|r| r.lair == lair) {
            return true;
        }
        if !w.works_of(node).is_some_and(|s| s.tributary) {
            return false;
        }
        self.tribute_stamp += 1;
        let stamp = self.tribute_stamp;
        self.tribute.insert(
            n,
            TributeRow {
                lair,
                stamp,
                paid_at: None,
            },
        );
        self.tribute_pay(w, n, at);
        self.tribute.contains_key(&n)
    }

    /// TEST (TRIBUTE_START): village `village_key` becomes the tributary of lair `lair_key`, as
    /// if a dragon of the lair had raided it at `at`. False for an unknown village or lair, or a
    /// village that has no purse to pay from.
    pub fn tribute_start(
        &mut self,
        w: &mut World,
        infra: &mut dyn Infrastructure,
        village_key: u64,
        lair_key: u64,
        at: Tick,
    ) -> bool {
        self.run_until(w, infra, at);
        let (Some(node), Some(l)) = (w.node_of_key(village_key), self.lair_of_key(lair_key)) else {
            return false;
        };
        self.tribute_begin(w, node, l, at)
    }

    /// Village `n` pays its lair a season's tribute (MS-5), or, its purse dry, the tribute
    /// lapses. The coin joins the lair's hoard and waits there for its chest.
    fn tribute_pay(&mut self, w: &mut World, n: u32, at: Tick) {
        let Some(row) = self.tribute.get(&n).copied() else {
            return;
        };
        let Some((g, a)) = w.pay_tribute(n as usize, self.tun.tribute_share_milli, at) else {
            self.tribute_lapse(w, n, "its purse is dry", at);
            return;
        };
        let item = Item::World(g);
        self.ledger.pay_tribute(item, a);
        if let Some(l) = self.lairs.get_mut(row.lair) {
            *l.hoard.entry(item).or_default() += a;
            *l.unplaced.entry(item).or_default() += a / item.unit();
        }
        if let Some(r) = self.tribute.get_mut(&n) {
            r.paid_at = Some(at);
        }
        self.stats.tributes += 1;
        let (village, lair_key) = (
            &w.nodes[n as usize],
            self.lairs.get(row.lair).map_or(0, |l| l.key),
        );
        self.tape
            .event(
                kinds::DRAGONS_TRIBUTE,
                spot(at, village.pos),
                u128::from(village.key),
                u128::from(lair_key),
                a / item.unit(),
            )
            .text(|| {
                format!(
                    "village {} pays lair {lair_key} {} coin",
                    village.key,
                    a / item.unit()
                )
            });
        if self.log_enabled {
            let what = format!(
                "village {} pays lair {} {} coin",
                w.nodes[n as usize].key,
                self.lairs.get(row.lair).map_or(0, |l| l.key),
                a / item.unit()
            );
            self.note(at, "tribute", None, what);
        }
        self.place_tribute(row.lair);
        let next = self.tun.calendar.next_change(at);
        self.schedule(
            next,
            Ev::Tribute {
                node: n,
                stamp: row.stamp,
            },
        );
    }

    /// Village `n` pays no tribute any more: it buys works again, golems first.
    fn tribute_lapse(&mut self, w: &mut World, n: u32, why: &str, at: Tick) {
        let Some(row) = self.tribute.remove(&n) else {
            return;
        };
        self.stats.tribute_lapses += 1;
        w.tribute_lapsed(n as usize, at);
        let village = &w.nodes[n as usize];
        let lair_key = self.lairs.get(row.lair).map_or(0, |l| l.key);
        self.tape
            .event(
                kinds::DRAGONS_TRIBUTE_LAPSED,
                spot(at, village.pos),
                u128::from(village.key),
                u128::from(lair_key),
                0,
            )
            .text(|| {
                format!(
                    "village {} stops paying lair {lair_key}: {why}",
                    village.key
                )
            });
        if self.log_enabled {
            let what = format!("village {} stops paying: {why}", w.nodes[n as usize].key);
            self.note(at, "tribute", None, what);
        }
    }

    /// Whether village `node` paid lair `l` tribute within a season of `at`: its dragons leave
    /// it alone (SINK.md §4.6).
    fn pays(&self, node: u32, l: LairId, at: Tick) -> bool {
        self.tribute.get(&node).is_some_and(|r| {
            r.lair == l
                && r.paid_at
                    .is_some_and(|p| at.0 < p.0.saturating_add(self.tun.calendar.season_ticks))
        })
    }

    /// The fear of village `node`'s defenders before a dragon's boldness bends it, fullness
    /// points: the tunable's base and its iron golems' (SINK.md §4.6, MS-8).
    pub fn village_risk(&self, w: &World, node: usize) -> i64 {
        i64::from(self.tun.village_risk)
            + i64::from(w.golems(node)) * i64::from(self.tun.golem_risk)
    }

    /// Lair `l`'s unplaced coin, in stacks of at most [`TRIBUTE_STACK`], sent to its chest when
    /// the plugin has reported one. Each stack's op id is drawn from the lair's key and a
    /// per-lair counter, so a resent stack is the same op.
    fn place_tribute(&mut self, l: LairId) {
        let Some(&chest) = self.chests.get(&l) else {
            return;
        };
        let seed = self.seed;
        let Some(lair) = self.lairs.get_mut(l) else {
            return;
        };
        if lair.unplaced.is_empty() {
            return;
        }
        let key = StreamKey::new(seed, SYS_TRIBUTE, lair.key);
        for (item, mut count) in std::mem::take(&mut lair.unplaced) {
            while count > 0 {
                let n = count.min(TRIBUTE_STACK);
                let op = rtsim_world::ribbits::identity_op(key.block(lair.stacks, 0)[0], 0);
                lair.stacks += 1;
                self.placing.insert(
                    op,
                    Placing {
                        lair: l,
                        item,
                        count: n,
                        chest,
                    },
                );
                count -= n;
            }
        }
    }

    /// Village `node`'s tribute row, if it pays a lair.
    pub fn tribute_row(&self, node: usize) -> Option<TributeRow> {
        u32::try_from(node)
            .ok()
            .and_then(|n| self.tribute.get(&n).copied())
    }

    /// Every village that pays a lair, by node.
    pub fn tribute_rows(&self) -> impl Iterator<Item = (u32, TributeRow)> + '_ {
        self.tribute.iter().map(|(&n, &r)| (n, r))
    }

    /// The stacks of tribute coin sent to lairs' chests and not yet answered, by op id.
    pub fn placing(&self) -> impl Iterator<Item = (u64, &Placing)> + '_ {
        self.placing.iter().map(|(&op, p)| (op, p))
    }

    /// Lair `l`'s chest, if the plugin reported one this session.
    pub fn chest(&self, l: LairId) -> Option<Chest> {
        self.chests.get(&l).copied()
    }

    /// The plugin put a stack in its chest (APPLIED, DUPLICATE_OP). False for an op that is not
    /// a tribute stack.
    pub fn tribute_placed(&mut self, op: u64) -> bool {
        let Some(p) = self.placing.remove(&op) else {
            return false;
        };
        if let Some(l) = self.lairs.get_mut(p.lair) {
            l.placed += p.count;
        }
        self.stats.tribute_stacks += 1;
        true
    }

    /// The plugin could not put a stack in its chest (FAILED): the coin waits as unplaced, and
    /// the chest is forgotten until the plugin reports one again.
    pub fn tribute_failed(&mut self, op: u64) -> bool {
        let Some(p) = self.placing.remove(&op) else {
            return false;
        };
        if let Some(l) = self.lairs.get_mut(p.lair) {
            *l.unplaced.entry(p.item).or_default() += p.count;
        }
        self.chests.remove(&p.lair);
        true
    }

    /// Back to abstract after a visit: a dragon that left (bored, hungry, wounded) flies home; one
    /// that only passed carries on with what it was doing.
    fn resume(&mut self, w: &mut World, d: DragonId, at: Tick, left: bool) {
        let purpose = match self.dragons[d].state {
            State::Embodied { purpose, .. } | State::Leaving { purpose, .. } => purpose,
            _ => Purpose::Home,
        };
        let purpose = match purpose {
            Purpose::Claim(l) => Purpose::Claim(l),
            Purpose::Refuel { .. } => purpose,
            Purpose::Hunt(t)
                if !left && !matches!(t, Target::Home { .. } | Target::District { .. }) =>
            {
                Purpose::Hunt(t)
            }
            _ => Purpose::Home,
        };
        self.dragons.get_mut(d).expect("live").state = State::Flying(purpose);
        self.track(d);
        // A field flight's dragon that left a player's view is done with its visit.
        if left && self.dragons[d].visitor.is_some() {
            self.visitor_done(w, d, at);
            return;
        }
        let at_home = match (purpose, self.dragons[d].lair) {
            (Purpose::Home, Some(l)) => {
                let (p, _) = self.dragons[d].position(&self.topo, &self.lairs, at);
                let lp = self.lairs[l as usize].pos;
                flight::dist(
                    self.topo(p.frame),
                    (i64::from(p.x), i64::from(p.z)),
                    (i64::from(lp.x), i64::from(lp.z)),
                ) < 32.0
            }
            _ => false,
        };
        if at_home {
            self.roost(w, d, at);
        } else {
            self.fly(w, d, purpose, at);
        }
    }

    /// A player (or anything) hurt an embodied dragon. A player's wound is a report the dragon
    /// witnesses, and its lair with it (C3: dragons remember slights); it brings the departure
    /// forward (C24).
    fn hurt(&mut self, w: &mut World, h: &boundary::Hurt, at: Tick) {
        let Some(d) = self.embodied(h.agent, h.epoch) else {
            return;
        };
        self.stats.hurt += 1;
        let sp = self.tun.species(self.dragons[d].species).clone();
        let max = self.dragons[d].max_health(&sp, at).max(1.0);
        let share = f64::from(h.amount) / f64::from(max);
        self.dragons
            .get_mut(d)
            .expect("live")
            .set_health(h.health_after, at);
        if h.attacker_kind == AttackerKind::Player {
            let p = self.player(w, h.attacker);
            let unit = self.tun.wound_unit_milli as f64 / 1000.0;
            let severity =
                (f64::from(SEVERITY_UNIT) * (1.0 + share / unit)).clamp(1.0, 255.0) as u8;
            let (pos, y) = self.dragons[d].position(&self.topo, &self.lairs, at);
            let id = w.knowledge.record(Report {
                who: p,
                whom: self.dragons[d].actor,
                kind: ReportKind::Assault,
                severity,
                flags: 0,
                at,
                place: place(pos, y),
            });
            w.knowledge.witness(self.dragons[d].actor, id, at);
            if let Some(l) = self.dragons[d].lair {
                let la = self.lairs[l as usize].actor;
                w.knowledge.witness(la, id, at);
            }
            let s = w.knowledge.sentiment(self.dragons[d].actor, p, at);
            self.note(
                at,
                "hurt",
                Some(d),
                format!(
                    "wounded by player {} for {:.0} ({:.0}% of max): sentiment now {s}",
                    p.to_bits(),
                    h.amount,
                    share * 100.0
                ),
            );
        }
        // Wounds bring the departure forward.
        let State::Embodied {
            epoch,
            since,
            depart_at,
            purpose,
            harass,
        } = self.dragons[d].state
        else {
            return;
        };
        if depart_at.0 == u64::MAX {
            return;
        }
        let k = self.tun.wound_shortens_milli as f64 / 1000.0;
        let left = depart_at.0.saturating_sub(at.0) as f64 * (1.0 - k * share).max(0.0);
        let new = at + left as u64;
        if new >= depart_at {
            return;
        }
        if left < 1.0 {
            self.depart(d, 3, at);
            return;
        }
        let plan = self.bump(d);
        self.dragons.get_mut(d).expect("live").state = State::Embodied {
            epoch,
            since,
            depart_at: new,
            purpose,
            harass,
        };
        self.track(d);
        self.schedule(new, Ev::Depart { d, plan, reason: 3 });
    }

    /// A dragon died. Killed by a player: its kin and lair witness the killing (their grudge), and
    /// the settlements it preyed on think better of the slayer (a positive report the caravans
    /// carry, C24: rescues count). Its strongest memories go to its heir (C24).
    fn kill(
        &mut self,
        w: &mut World,
        d: DragonId,
        killer: Option<ActorId>,
        at_pos: (Pos, i32),
        at: Tick,
    ) {
        self.stats.deaths_violent += 1;
        let x = self.dragons[d].clone();
        self.tape_death(d, at_pos.0, killer, at);
        if let Some(p) = killer {
            let id = w.knowledge.record(Report {
                who: p,
                whom: x.actor,
                kind: ReportKind::Killing,
                severity: SEVERITY_UNIT,
                flags: 0,
                at,
                place: place(at_pos.0, at_pos.1),
            });
            let mut kin: Vec<ActorId> = Vec::new();
            for k in x
                .children
                .iter()
                .chain(x.mate.iter())
                .chain(x.parent.iter())
            {
                if let Some(o) = self.dragons.get(*k) {
                    kin.push(o.actor);
                }
            }
            if let Some(l) = x.lair {
                kin.push(self.lairs[l as usize].actor);
            }
            for k in &kin {
                w.knowledge.witness(*k, id, at);
            }
            let mut thanks = Vec::new();
            for v in &x.victims {
                let rid = w.knowledge.record(Report {
                    who: p,
                    whom: *v,
                    kind: ReportKind::Rescue,
                    severity: SEVERITY_UNIT,
                    flags: 0,
                    at,
                    place: place(at_pos.0, at_pos.1),
                });
                w.knowledge.witness(*v, rid, at);
                let ext = w.knowledge.actors().external(*v).unwrap_or(0);
                thanks.push(format!(
                    "village {ext} {:+}",
                    w.knowledge.sentiment(*v, p, at)
                ));
            }
            self.note(
                at,
                "death",
                Some(d),
                format!(
                    "killed by player {}: {} kin and its lair remember; the settlements it preyed on think better of the slayer ({})",
                    p.to_bits(),
                    kin.len().saturating_sub(x.lair.is_some() as usize),
                    thanks.join(", ")
                ),
            );
        } else {
            self.note(at, "death", Some(d), "killed".into());
        }
        let heir = w.knowledge.die(x.actor, at);
        // The heir is its youngest child, else its mate, else its lair (`refresh_heir`).
        if let Some(h) = heir.filter(|_| self.log_enabled) {
            let who = x
                .children
                .iter()
                .chain(x.mate.iter())
                .filter_map(|k| self.dragons.get(*k))
                .find(|o| o.actor == h)
                .map(|o| o.name.clone())
                .unwrap_or_else(|| "its lair".to_string());
            self.note(at, "heir", Some(d), format!("its grudges pass to {who}"));
        }
        self.dying_by_body = true;
        self.remove(w, d, at);
        self.dying_by_body = false;
    }

    /// Writes a dragon's death on the archive's tape: where it fell, who killed it (none when it
    /// died of age), and how old it was in days.
    fn tape_death(&mut self, d: DragonId, pos: Pos, killer: Option<ActorId>, at: Tick) {
        if !self.tape.is_on() {
            return;
        }
        let x = &self.dragons[d];
        let days = x.age(at) / TICKS_PER_DAY;
        self.tape
            .event(
                kinds::DRAGONS_DEATH,
                spot(at, pos),
                did(d),
                killer.map_or(0, aid),
                days as i64,
            )
            .text(|| match killer {
                Some(p) => format!("{} killed by player {}", x.name, p.to_bits()),
                None => format!("{} dies of age", x.name),
            });
    }

    /// Takes a dead dragon out: its lair may stand empty (the respawn clock starts), its kin
    /// lose a link and re-point their heirs.
    pub(crate) fn remove(&mut self, w: &mut World, d: DragonId, at: Tick) {
        self.end_visit(d);
        self.release_claim(d, at);
        self.cancel_leg(w, d);
        // A body still in the world goes with it.
        if let Some(State::Embodied { epoch, .. } | State::Leaving { epoch, .. }) =
            self.dragons.get(d).map(|x| x.state)
            && !self.dying_by_body
        {
            let (to, y) = self.dragons[d].position(&self.topo, &self.lairs, at);
            self.outbox.push(Cmd::Depart(boundary::Depart {
                agent: agent_of(d),
                epoch,
                reason: 5,
                timeout_ticks: 0,
                target: (f64::from(to.x), f64::from(y), f64::from(to.z)),
            }));
        }
        // What a lairless dragon carried is lost with it.
        if let Some(x) = self.dragons.get_mut(d) {
            for (item, n) in std::mem::take(&mut x.carried) {
                self.ledger.lose(item, n);
            }
        }
        self.leave_lair(w, d, at);
        let Some(x) = self.dragons.remove(d) else {
            return;
        };
        self.embodied.remove(&d);
        // Its bodies go from both maps (both hold only bodies in flight, so this is no pass over
        // every dragon); an adopted entry left behind would name a dead dragon for good.
        self.by_entity.retain(|_, v| *v != d);
        self.adopted.retain(|_, v| *v != d);
        let mut touched = Vec::new();
        if let Some(m) = x.mate
            && let Some(o) = self.dragons.get_mut(m)
        {
            o.mate = None;
            touched.push(m);
        }
        for c in &x.children {
            if let Some(o) = self.dragons.get_mut(*c) {
                o.parent = None;
            }
        }
        if let Some(p) = x.parent
            && let Some(o) = self.dragons.get_mut(p)
        {
            o.children.retain(|c| *c != d);
            touched.push(p);
        }
        for t in touched {
            self.refresh_heir(w, t);
        }
    }

    pub(crate) fn leave_lair(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(l) = self.dragons.get(d).and_then(|x| x.lair) else {
            return;
        };
        // Where it is stays where it is: a dragon at its lair is at the lair.
        let here = self.dragons[d].position(&self.topo, &self.lairs, at);
        let lair = &mut self.lairs[l as usize];
        lair.occupants.retain(|o| *o != d);
        if let Some(x) = self.dragons.get_mut(d) {
            x.lair = None;
            if x.rest.is_none() && !matches!(x.state, State::Flying(_) | State::Hunting { .. }) {
                x.rest = Some(here);
            }
        }
        if lair.occupants.is_empty() {
            lair.vacant_since = Some(at);
            lair.plan += 1;
            let plan = lair.plan;
            let delay = self.tun.respawn_delay_days * TICKS_PER_DAY;
            self.schedule(at + delay, Ev::Respawn { lair: l, plan });
        }
        self.track_lair(l);
        let _ = w;
    }

    /// Moves into a lair. A newcomer to an empty lair takes over the lair's strongest feelings
    /// (C24: the lair is the heir of last resort).
    fn occupy(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick, inherit: bool) {
        let lair = &mut self.lairs[l as usize];
        let was_empty = lair.occupants.is_empty();
        if !lair.occupants.contains(&d) {
            lair.occupants.push(d);
        }
        lair.vacant_since = None;
        lair.plan += 1;
        let la = lair.actor;
        let x = self.dragons.get_mut(d).expect("live");
        x.lair = Some(l);
        for (item, n) in std::mem::take(&mut x.carried) {
            *lair.hoard.entry(item).or_default() += n;
        }
        if inherit && was_empty {
            let mut feelings = w.knowledge.sentiment_snapshot(la, at);
            feelings.sort_by_key(|(t, v)| (std::cmp::Reverse(i32::from(*v).abs()), *t));
            let actor = self.dragons[d].actor;
            for (t, v) in feelings.into_iter().take(self.tun.lair_feelings) {
                w.knowledge.adjust(actor, t, i16::from(v), at);
            }
        }
        self.track_lair(l);
        self.refresh_heir(w, d);
    }

    /// Arriving at a roost to claim it: an empty one is taken (with its grudges); an occupied
    /// one is shared, and an unmated adult of the other sex there becomes its mate.
    fn claim(&mut self, w: &mut World, d: DragonId, l: LairId, at: Tick) {
        self.stats.claims += 1;
        let occupants = self.lairs[l as usize].occupants.clone();
        let name = self.lairs[l as usize].name.clone();
        let (lair_pos, lair_key) = (self.lairs[l as usize].pos, self.lairs[l as usize].key);
        self.tape
            .event(
                kinds::DRAGONS_CLAIM,
                spot(at, lair_pos),
                did(d),
                u128::from(lair_key),
                occupants.len() as i64,
            )
            .text(|| {
                if occupants.is_empty() {
                    format!("claims {name}")
                } else {
                    format!("claims {name}, shared with {}", occupants.len())
                }
            });
        self.occupy(w, d, l, at, occupants.is_empty());
        let me = &self.dragons[d];
        let sp = self.tun.species(me.species).clone();
        let partner = occupants.into_iter().find(|o| {
            self.dragons.get(*o).is_some_and(|x| {
                x.mate.is_none()
                    && x.gender != me.gender
                    && x.species == me.species
                    && x.stage(&sp, at).0 >= 3
                    && me.stage(&sp, at).0 >= 3
            })
        });
        self.note(at, "claim", Some(d), format!("claims {name}"));
        if let Some(o) = partner {
            self.pair(w, d, o, at);
            self.note(
                at,
                "pair",
                Some(d),
                format!("pairs with {} at {name}", self.dragons[o].name),
            );
        }
    }

    // ---- population -----------------------------------------------------------------------------

    fn breed(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        let Some(m) = x.mate.filter(|m| self.dragons.contains(*m)) else {
            return;
        };
        let sp = self.tun.species(x.species).clone();
        let (Some(l), true) = (x.lair, self.dragons[m].lair == x.lair) else {
            let t = at + sp.breed_days * TICKS_PER_DAY;
            self.schedule(t, Ev::Breed { d });
            return;
        };
        if !x.stage(&sp, at).is_mature() || !self.dragons[m].stage(&sp, at).is_mature() {
            let t = self.next_clutch(d, at);
            self.schedule(t, Ev::Breed { d });
            return;
        }
        let mut rng = StreamKey::new(self.seed, SYS_BREED, x.seed).slot(at.0);
        let gender = if rng.below(2) == 0 {
            Gender::Female
        } else {
            Gender::Male
        };
        let variant = rng.below(4) as u8;
        let name = format!("{}'s young {}", x.name, self.stats.births + 1);
        let species = x.species;
        let child = self.add_dragon(
            w,
            NewDragon {
                name,
                species,
                gender,
                variant,
                age_ticks: 0,
                lair: Some(l),
                fullness: 50,
                health: None,
                seed: None,
                at: None,
            },
            at,
        );
        self.stats.births += 1;
        self.set_parent(w, child, d, at);
        self.tape
            .event(
                kinds::DRAGONS_BIRTH,
                spot(at, self.lairs[l as usize].pos),
                did(child),
                did(d),
                1,
            )
            .text(|| {
                format!(
                    "{} hatches at {}",
                    self.dragons[child].name, self.lairs[l as usize].name
                )
            });
        self.note(
            at,
            "birth",
            Some(d),
            format!("hatches a young one, {}", self.dragons[child].name),
        );
        self.schedule(at + sp.breed_days * TICKS_PER_DAY, Ev::Breed { d });
    }

    fn disperse(&mut self, w: &mut World, d: DragonId, at: Tick) {
        let Some(x) = self.dragons.get(d) else {
            return;
        };
        if x.parent.is_none() || x.is_embodied() || !matches!(x.state, State::Roosting) {
            if x.parent.is_some() {
                self.schedule(at + 2 * TICKS_PER_DAY, Ev::Disperse { d });
            }
            return;
        }
        match self.vacant_lair(d, at) {
            Some(l) => {
                let name = self.lairs[l as usize].name.clone();
                self.note(
                    at,
                    "disperse",
                    Some(d),
                    format!("leaves its parents for {name}"),
                );
                self.leave_lair(w, d, at);
                self.lairs[l as usize].plan += 1;
                self.lairs[l as usize].vacant_since = None;
                self.track_lair(l);
                self.fly(w, d, Purpose::Claim(l), at);
            }
            None => self.schedule(at + 10 * TICKS_PER_DAY, Ev::Disperse { d }),
        }
    }

    /// The respawn rule (P13's recommendation, the TUNABLE delay): a roost empty for the delay
    /// draws a young dragon of its species from afar. Its arrival is a flight like any other, so
    /// a player can watch it come.
    fn respawn(&mut self, w: &mut World, l: LairId, plan: u32, at: Tick) {
        let lair = &self.lairs[l as usize];
        if lair.plan != plan || !lair.occupants.is_empty() || lair.vacant_since.is_none() {
            return;
        }
        self.stats.respawns += 1;
        let species = lair.species;
        let mut rng = StreamKey::new(self.seed, SYS_BREED, lair.key).slot(at.0);
        let angle = rng.next_f64() * std::f64::consts::TAU;
        let r = f64::from(self.tun.respawn_from_blocks);
        let (fx, fz) = self.topo(lair.pos.frame).canonical_i32(
            lair.pos.x + (r * angle.cos()) as i32,
            lair.pos.z + (r * angle.sin()) as i32,
        );
        let from = Pos::new(lair.pos.frame, fx, fz);
        let gender = if rng.below(2) == 0 {
            Gender::Female
        } else {
            Gender::Male
        };
        let variant = rng.below(4) as u8;
        let cruise = self.tun.species(species).cruise_y;
        let name = format!("{} newcomer", lair.name);
        let d = self.add_dragon(
            w,
            NewDragon {
                name,
                species,
                gender,
                variant,
                age_ticks: self.tun.respawn_age_days * TICKS_PER_DAY,
                lair: None,
                fullness: 50,
                health: None,
                seed: None,
                at: Some((from, cruise)),
            },
            at,
        );
        let lair = &mut self.lairs[l as usize];
        lair.plan += 1;
        lair.vacant_since = None;
        let name = lair.name.clone();
        let lair_key = lair.key;
        self.track_lair(l);
        self.tape
            .event(
                kinds::DRAGONS_ARRIVE,
                spot(at, from),
                did(d),
                u128::from(lair_key),
                0,
            )
            .text(|| format!("a young dragon flies in to claim {name}"));
        self.note(
            at,
            "respawn",
            Some(d),
            format!("{name} stood empty: a young dragon flies in to claim it"),
        );
        self.fly(w, d, Purpose::Claim(l), at);
    }

    /// An untagged dragon appeared in the world (P13's recommendation: the sim owns every
    /// dragon). Tamed ones are the player's (A10) and are left alone; a lair's own dragon from
    /// world generation is adopted as its occupant; spawn eggs and commands are adopted as
    /// wanderers. Natural and spawner dragons (In Control's desert dragons) are left alone, not
    /// adopted, discarded or tracked (R18 as amended by OG-1 and P13, 2026-10-20), with two
    /// exceptions that discard them as R18 had it: a fire one while field flights run, since the
    /// flights replace In Control's desert fire dragons and one never runs beside the other
    /// (OGB-6, docs/choices-oilgate.md OGC66); and every one with `tun.adopt_natural_dragons`.
    fn untagged(&mut self, w: &mut World, u: &boundary::Untagged, at: Tick) {
        if u.flags & boundary::UNTAGGED_TAMED != 0 || u.spawn == SpawnKind::Hatched {
            return;
        }
        if self.by_entity.contains_key(&u.entity) {
            return;
        }
        if let Some(&d) = self.adopted.get(&u.entity)
            && self.dragons.contains(d)
        {
            self.adopt_again(w, d, u, at);
            return;
        }
        // Left alone (a dragon this engine adopted before is still taken back above).
        if matches!(u.spawn, SpawnKind::Natural | SpawnKind::Spawner)
            && !self.tun.adopt_natural_dragons
            && !(u.species == Species::Fire && self.field_flights_on())
        {
            return;
        }
        let lair = (u.lair != 0).then(|| self.lair_of_key(u.lair)).flatten();
        let discard = match u.spawn {
            SpawnKind::Natural | SpawnKind::Spawner => Some(1),
            SpawnKind::Structure
                if lair.is_some_and(|l| !self.lairs[l as usize].occupants.is_empty()) =>
            {
                Some(2)
            }
            _ => None,
        };
        if let Some(reason) = discard {
            self.outbox.push(Cmd::Discard(boundary::Discard {
                entity: u.entity,
                reason,
            }));
            self.stats.discarded += 1;
            return;
        }
        let pos = self.canon(u.dimension, u.pos.0, u.pos.2);
        let d = self.add_dragon(
            w,
            NewDragon {
                name: format!("{} dragon {:x}", u.species.name(), u.entity.1 & 0xffff),
                species: u.species,
                gender: u.gender,
                variant: (u.variant % 4) as u8,
                age_ticks: u.age_ticks.max(0) as u64,
                lair,
                fullness: u.hunger,
                health: None,
                seed: Some(fnv1a64(
                    &[u.entity.0.to_le_bytes(), u.entity.1.to_le_bytes()].concat(),
                )),
                at: Some((pos, u.pos.1 as i32)),
            },
            at,
        );
        self.stats.adopted += 1;
        // It is a live entity now: adopt it as embodied, so the plugin keeps it until it leaves
        // view and then hands it back.
        let epoch = 1;
        let home = lair.map_or((0, 0, 0), |l| {
            let l = &self.lairs[l as usize];
            (l.pos.x, l.y, l.pos.z)
        });
        self.outbox.push(Cmd::Adopt(boundary::Adopt {
            entity: u.entity,
            agent: agent_of(d),
            epoch,
            flags: lair.is_some() as u32,
            home,
        }));
        self.by_entity.insert(u.entity, d);
        self.adopted.insert(u.entity, d);
        let plan = self.bump(d);
        let _ = plan;
        self.cancel_leg(w, d);
        let x = self.dragons.get_mut(d).expect("live");
        x.epoch = epoch;
        x.state = State::Embodied {
            epoch,
            since: at,
            depart_at: Tick(u64::MAX),
            purpose: Purpose::Home,
            harass: false,
        };
        self.track(d);
    }

    /// An entity the engine adopted before, reported untagged again: the same dragon takes the
    /// body back under a new epoch, where the body is (its abstract self is dropped where it
    /// was), and it is embodied until the plugin hands it back.
    fn adopt_again(&mut self, w: &mut World, d: DragonId, u: &boundary::Untagged, at: Tick) {
        self.stats.adopted += 1;
        self.bump(d);
        self.cancel_leg(w, d);
        let here = self.canon(u.dimension, u.pos.0, u.pos.2);
        let x = self.dragons.get_mut(d).expect("live");
        let epoch = x.epoch + 1;
        x.epoch = epoch;
        x.rest = Some((here, u.pos.1 as i32));
        x.state = State::Embodied {
            epoch,
            since: at,
            depart_at: Tick(u64::MAX),
            purpose: Purpose::Home,
            harass: false,
        };
        let lair = x.lair;
        self.track(d);
        let home = lair.map_or((0, 0, 0), |l| {
            let l = &self.lairs[l as usize];
            (l.pos.x, l.y, l.pos.z)
        });
        self.outbox.push(Cmd::Adopt(boundary::Adopt {
            entity: u.entity,
            agent: agent_of(d),
            epoch,
            flags: lair.is_some() as u32,
            home,
        }));
        self.by_entity.insert(u.entity, d);
    }

    /// A player robbed a lair's hoard: what the engine's hoard held comes out (the rest was the
    /// structure's own treasure, never in the ledger), and the lair and its dragons witness the
    /// robbery: a grudge (M1's egg theft would be the same report, hooks UNVERIFIED).
    fn robbed(&mut self, w: &mut World, h: &boundary::HoardRobbed, at: Tick) {
        let Some(l) = self.lair_of_key(h.lair) else {
            return;
        };
        self.stats.hoards_robbed += 1;
        let p = self.player(w, h.player);
        let mut count = 0i64;
        for &(id, n) in &h.items {
            count += i64::from(n.max(0));
            let Some(item) = self.items.get(&id).copied() else {
                continue;
            };
            let units = i64::from(n.max(0)) * item.unit();
            let held = self.lairs[l as usize]
                .hoard
                .get(&item)
                .copied()
                .unwrap_or(0);
            let take = units.min(held);
            if take > 0 {
                *self.lairs[l as usize].hoard.get_mut(&item).expect("held") -= take;
                self.ledger.rob(item, take);
            }
        }
        let lair = &self.lairs[l as usize];
        let severity = (i64::from(SEVERITY_UNIT) * (1 + count / 64)).clamp(1, 255) as u8;
        let id = w.knowledge.record(Report {
            who: p,
            whom: lair.actor,
            kind: ReportKind::Robbery,
            severity,
            flags: 0,
            at,
            place: place(lair.pos, lair.y),
        });
        let mut who = vec![lair.actor];
        for o in &lair.occupants {
            who.push(self.dragons[*o].actor);
        }
        for a in who {
            w.knowledge.witness(a, id, at);
        }
        let name = lair.name.clone();
        let feelings: Vec<String> = lair
            .occupants
            .iter()
            .map(|o| {
                let x = &self.dragons[*o];
                format!("{} {}", x.name, w.knowledge.sentiment(x.actor, p, at))
            })
            .collect();
        self.note(
            at,
            "robbed",
            None,
            format!(
                "player {} robs the hoard of {name} ({count} items); its dragons' feelings: {}",
                p.to_bits(),
                feelings.join(", ")
            ),
        );
    }

    // ---- checks ----------------------------------------------------------------------------------

    /// Conservation with predation (PLAN.md §8.4, §9): the world balances on its own (what
    /// dragons took is destroyed in its books); everything the world counts as destroyed is what
    /// this ledger says dragons took; taken = eaten + burnt + hoarded per good; hoards hold
    /// hoarded − robbed. `industry_stolen`, when given, must equal what dragons took from
    /// districts (true when dragons are the only threat).
    pub fn check(
        &self,
        w: &World,
        industry_stolen: Option<&rtsim_industry::ids::GoodVec>,
    ) -> Result<(), String> {
        let at = w.now();
        w.check_conserved(at).map_err(|e| format!("world: {e}"))?;
        self.ledger.check_split()?;
        let destroyed = w.destroyed_total(at);
        let taken = self.ledger.total_world(&self.ledger.taken);
        if destroyed != taken {
            return Err(format!(
                "the world counts {destroyed:?} destroyed but dragons took {taken:?}"
            ));
        }
        let mut held: BTreeMap<Item, i128> = BTreeMap::new();
        let mut vacant = BTreeSet::new();
        for (i, l) in self.lairs.iter().enumerate() {
            for (k, v) in &l.hoard {
                *held.entry(*k).or_default() += i128::from(*v);
            }
            if l.occupants.is_empty() && l.vacant_since.is_some() {
                vacant.insert(i as LairId);
            }
        }
        if vacant != self.vacant {
            return Err(format!(
                "the vacant index holds {:?} but the lairs say {vacant:?}",
                self.vacant
            ));
        }
        let mut embodied = BTreeSet::new();
        for (h, x) in self.dragons.iter() {
            for (k, v) in &x.carried {
                *held.entry(*k).or_default() += i128::from(*v);
            }
            if x.is_embodied() {
                embodied.insert(h);
            }
        }
        if embodied != self.embodied {
            return Err(format!(
                "the embodied index holds {:?} but the dragons say {embodied:?}",
                self.embodied
            ));
        }
        // Tribute: what the villages paid is what the ledger booked, and every coin of it is in a
        // lair (unplaced or placed) or on its way to a chest.
        let paid: BTreeMap<GoodId, i128> = w
            .works_totals()
            .1
            .into_iter()
            .filter(|(_, a)| *a != 0)
            .collect();
        let booked = self.ledger.total_world(&self.ledger.tribute);
        if paid != booked {
            return Err(format!(
                "villages paid {paid:?} tribute but the dragons booked {booked:?}"
            ));
        }
        let coins: i128 = booked.values().sum::<i128>() / i128::from(rtsim_trade::MILLI);
        let mut kept: i128 = self.placing.values().map(|p| i128::from(p.count)).sum();
        for (i, l) in self.lairs.iter().enumerate() {
            kept +=
                i128::from(l.placed) + l.unplaced.values().map(|&v| i128::from(v)).sum::<i128>();
            if l.unplaced.values().any(|&v| v <= 0) || l.placed < 0 {
                return Err(format!("lair {i} keeps a bad tribute count"));
            }
        }
        if coins != kept {
            return Err(format!(
                "villages paid {coins} coins of tribute but the lairs keep {kept}"
            ));
        }
        held.retain(|_, v| *v != 0);
        if held != self.ledger.in_hoards() {
            return Err(format!(
                "hoards hold {held:?} but the ledger says {:?}",
                self.ledger.in_hoards()
            ));
        }
        if let Some(stolen) = industry_stolen {
            let mine = self.ledger.total_industry(&self.ledger.taken);
            let theirs: BTreeMap<_, i128> = stolen
                .iter()
                .filter(|(_, n)| *n != 0)
                .map(|(g, n)| (g, i128::from(n)))
                .collect();
            if mine != theirs {
                return Err(format!(
                    "the industry engine booked {theirs:?} stolen but dragons took {mine:?}"
                ));
            }
        }
        Ok(())
    }

    /// A hash of the dragons' state, for determinism checks.
    pub fn state_hash(&self) -> u64 {
        let mut wr = Writer::new();
        for (h, x) in self.dragons.iter() {
            wr.u64(h.to_bits());
            wr.u64(x.seed);
            wr.u64(x.age_base);
            wr.u64(x.age_at.0);
            wr.raw(&x.fullness.to_le_bytes());
            wr.u64(x.fed_at.0);
            wr.raw(&x.health.to_bits().to_le_bytes());
            wr.u64(x.health_at.0);
            wr.u64(u64::from(x.plan));
            wr.u64(u64::from(x.epoch));
            wr.u64(x.lair.map_or(u64::MAX, u64::from));
            wr.raw(&x.oil.to_le_bytes());
            wr.u64(x.oil_at.0);
            wr.u64(x.visitor.map_or(u64::MAX, u64::from));
            if let Some(l) = x.leg {
                for v in [l.path.x0, l.path.z0, l.path.x1, l.path.z1] {
                    wr.raw(&v.to_le_bytes());
                }
                wr.u64(l.path.t0.0);
                wr.u64(l.path.t1.0);
            }
        }
        for l in self.lairs.iter() {
            wr.u64(l.key);
            wr.u64(l.occupants.len() as u64);
            for (k, v) in &l.hoard {
                wr.raw(k.key().as_bytes());
                wr.raw(&v.to_le_bytes());
            }
            for (k, v) in &l.unplaced {
                wr.raw(k.key().as_bytes());
                wr.raw(&v.to_le_bytes());
            }
            wr.raw(&l.placed.to_le_bytes());
            wr.u64(l.stacks);
        }
        wr.u64(u64::from(self.tribute_stamp));
        for (n, r) in &self.tribute {
            wr.u64(u64::from(*n));
            wr.u64(u64::from(r.lair));
            wr.u64(u64::from(r.stamp));
            wr.u64(r.paid_at.map_or(u64::MAX, |t| t.0));
        }
        for (op, p) in &self.placing {
            wr.u64(*op);
            wr.u64(u64::from(p.lair));
            wr.raw(p.item.key().as_bytes());
            wr.raw(&p.count.to_le_bytes());
            wr.u64(u64::from(p.chest.dimension));
            for v in p.chest.pos {
                wr.raw(&v.to_le_bytes());
            }
        }
        for m in [
            &self.ledger.taken,
            &self.ledger.eaten,
            &self.ledger.burnt,
            &self.ledger.hoarded,
            &self.ledger.tribute,
        ] {
            for (k, v) in m {
                wr.raw(k.key().as_bytes());
                wr.raw(&v.to_le_bytes());
            }
        }
        wr.u32(self.nests.day);
        wr.u32(self.nests.plan);
        for (k, n) in self.nests.rows.iter() {
            wr.u64(crate::nest::nest_id(*k));
            wr.bool(self.nests.awake.contains(k));
            crate::persist::put_nest(n, &mut wr);
        }
        wr.u64(self.oil.chunks);
        wr.u64(u64::from(self.oil.next_field));
        wr.u64(u64::from(self.oil.live));
        fnv1a64(wr.as_bytes())
    }

    /// What the engine holds, estimated: the row tables' resident pages (their rows' heap as last
    /// measured, at [`Dragons::enforce_budget`]) and the resident state. No pass over the rows.
    pub fn heap_bytes(&self) -> usize {
        self.memory().values().sum()
    }

    /// [`Dragons::heap_bytes`] by component, for the bench and the notes.
    pub fn memory(&self) -> BTreeMap<&'static str, usize> {
        BTreeMap::from([
            (
                "dragon rows (resident pages)",
                self.dragons.rows().heap_bytes(),
            ),
            ("lair rows (resident pages)", self.lairs.rows().heap_bytes()),
            ("lair indexes, embodied and vacant sets", self.index_bytes()),
            ("wheel", self.wheel.heap_bytes()),
            ("homes", self.homes.heap_bytes()),
            ("prey index", self.index.heap_bytes()),
            ("oil index", self.oil.heap_bytes()),
            ("oil gate maps", self.oil_gate_bytes()),
            ("oil nests", self.nests.heap_bytes()),
            (
                "log and players",
                self.log.capacity() * std::mem::size_of::<LogEntry>()
                    + self.players.len() * (std::mem::size_of::<(ActorId, PlayerView)>() + 16)
                    + (self.by_entity.len() + self.adopted.len()) * 40,
            ),
        ])
    }

    /// The resident lair indexes (`lair_by_key`, `lair_cells`, `vacant`) and the embodied set,
    /// estimated as B-tree entries at two-thirds fill plus each cell's vector.
    pub fn index_bytes(&self) -> usize {
        let node = |entry: usize| entry * 3 / 2;
        self.lair_by_key.len() * node(12)
            + self.lair_cells.len() * node(24 + 24)
            + self
                .lair_cells
                .values()
                .map(|v| (v.capacity() * 4).max(16))
                .sum::<usize>()
            + self.embodied.len() * node(8)
            + self.vacant.len() * node(4)
    }

    /// The districts a player's home holds, for display.
    pub fn home_districts(
        &self,
        infra: &mut dyn Infrastructure,
        p: ActorId,
        at: Tick,
    ) -> Vec<DistrictId> {
        match self.homes.home(p, at) {
            Some(h) => infra
                .districts_reaching(
                    h.pos.frame,
                    i64::from(h.pos.x),
                    i64::from(h.pos.z),
                    HOME_RADIUS,
                )
                .into_iter()
                .map(|s| s.id)
                .collect(),
            None => Vec::new(),
        }
    }
}

/// The dragons as a viewer of the engine's shared interest areas (one per player across every
/// module; overnight choices U2 to U6). Through it the dragons see exactly what their own areas
/// ([`Dragons::observe_player`]) showed them: the dragon legs a player's area crosses, and the
/// dragons at rest in a lair it covers. The home counter is fed apart, by
/// [`Dragons::observe_home`]. A player's view here keeps no area of its own (`interest` stays
/// `None`), so [`Dragons::player_left`] never cancels the shared one.
impl rtsim_world::interest::Viewer for Dragons {
    fn owns(&self, owner: rtsim_travel::legs::Owner) -> bool {
        owner_dragon(owner).is_some()
    }

    fn area_booked(&mut self, w: &mut World, player: ActorId, uuid: (u64, u64), area: Stamp) {
        let _ = (w, area);
        self.players.entry(player).or_insert(PlayerView {
            uuid,
            interest: None,
        });
    }

    fn crossings(&mut self, w: &mut World, player: ActorId, area: Stamp, found: &[Meeting]) {
        self.seen_legs(w, player, found);
        if let Some(path) = w.legs.get(area.leg).map(|r| r.path) {
            self.seen_at_lairs(player, area, path);
        }
    }
}
