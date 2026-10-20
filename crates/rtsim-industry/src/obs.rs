// SPDX-License-Identifier: AGPL-3.0-or-later
//! Observations the Java sampler sends (docs/industry-boundary.md §3): boundary stock, ports,
//! district membership and edits, load states, results of commands, and what threats need.
//!
//! These are the decoded forms; [`crate::wire`] reads and writes the byte layouts. Journaled kinds
//! carry the bridge's `obsSeq` in the record, which the host's `Replay::admit` consumes before the
//! industry engine sees the observation; only the game time reaches the engine.

use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;

use crate::ids::{BlockPos, ChannelId, ChunkPos, DistrictId, Good, GoodVec, StationId, TrainId};
use crate::threat::ThreatClass;

/// Why a district cannot be learned (INDUSTRY.md §6). The ledger shows the reason in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NotLearnable {
    RotationCrossesBoundary,
    ElectricLineCrossesBoundary,
    MixtureNotSteady,
    FiniteDepositUnknown,
    MovingContraption,
    SpawnFedFarm,
    Other(u32),
}

impl NotLearnable {
    pub fn code(self) -> u32 {
        match self {
            NotLearnable::RotationCrossesBoundary => 1,
            NotLearnable::ElectricLineCrossesBoundary => 2,
            NotLearnable::MixtureNotSteady => 3,
            NotLearnable::FiniteDepositUnknown => 4,
            NotLearnable::MovingContraption => 5,
            NotLearnable::SpawnFedFarm => 6,
            NotLearnable::Other(n) => n,
        }
    }

    pub fn from_code(n: u32) -> Option<NotLearnable> {
        Some(match n {
            0 => return None,
            1 => NotLearnable::RotationCrossesBoundary,
            2 => NotLearnable::ElectricLineCrossesBoundary,
            3 => NotLearnable::MixtureNotSteady,
            4 => NotLearnable::FiniteDepositUnknown,
            5 => NotLearnable::MovingContraption,
            6 => NotLearnable::SpawnFedFarm,
            n => NotLearnable::Other(n),
        })
    }

    pub fn words(self) -> &'static str {
        match self {
            NotLearnable::RotationCrossesBoundary => "rotation crosses the boundary",
            NotLearnable::ElectricLineCrossesBoundary => "electric line crosses the boundary",
            NotLearnable::MixtureNotSteady => "mixture composition not steady",
            NotLearnable::FiniteDepositUnknown => "finite deposit unknown",
            NotLearnable::MovingContraption => "moving drill",
            NotLearnable::SpawnFedFarm => "mob drops not simulated",
            NotLearnable::Other(_) => "not learnable",
        }
    }
}

/// What an inventory is (bit flags, docs/industry-boundary.md §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InvKind(pub u32);

impl InvKind {
    pub const ITEMS: u32 = 1;
    pub const FLUIDS: u32 = 2;
    pub const ENERGY: u32 = 4;
    /// A postbox; while its chunk is unloaded this is Create's `offlineBuffer` (18 packages).
    pub const POSTBOX: u32 = 8;
    /// The inventory behind a Create packager (`PackagerBlockEntity.targetInventory`).
    pub const PACKAGER_TARGET: u32 = 16;
    /// A plugin buffer (PowerGrid battery and the like).
    pub const PLUGIN: u32 = 32;

    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// A boundary inventory, declared once per district revision (INDUSTRY.md §2.2, I3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryDecl {
    pub district: DistrictId,
    pub revision: u32,
    pub index: u32,
    pub pos: BlockPos,
    pub kind: InvKind,
    /// Item slots (a Create vault: 20 per block, `vaultCapacity`, verified).
    pub item_slots: u32,
    /// Millibuckets (a Create tank: 8 buckets per block, `fluidTankCapacity`, verified).
    pub fluid_capacity: i64,
    pub energy_capacity: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DistrictDecl {
    pub district: DistrictId,
    /// Bumped by the Java side on every membership change.
    pub revision: u32,
    pub dim: u32,
    /// NAME dictionary entry: the first station's name, else coordinates (I2).
    pub name: u32,
    pub not_learnable: Option<NotLearnable>,
    pub inventory_count: u32,
}

/// A player edit (INDUSTRY.md §3.3): only `BlockEvent`s with a player make a district stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Placed,
    Broken,
    /// A wrench or configuration change on a machine.
    Configured,
    /// `/rtsim district rename | merge | split | exclude`.
    Command,
}

/// A learning window that must be thrown away (INDUSTRY.md §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardReason {
    UnattributedSession,
    CreativeBlock,
    Overstressed,
    ProgressiveContraption,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// No chunk of the district is block-ticking.
    Unloaded,
    /// Some are (I7: suspended).
    Partial,
    /// All are.
    Live,
}

/// A port: goods crossing the boundary while loaded (INDUSTRY.md §3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortKind {
    TrainCargo,
    Package,
    PlayerSession,
    PlayerToss,
    PlayerPickup,
    /// A player broke a stock block; its contents left.
    StockBroken,
    /// An Ender Transmission transmitter or receiver (I13); the counterpart is the channel.
    Channel,
    /// The engine's own reconcile or damage, applied: never production.
    Reconcile,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortTransfer {
    pub district: DistrictId,
    pub kind: PortKind,
    pub inventory: Option<u32>,
    /// Train, player (hashed UUID), or channel, by kind.
    pub counterpart: u64,
    /// The bridge's op id, if the transfer has one.
    pub op: Option<OpId>,
    /// Positive: into the district.
    pub goods: GoodVec,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carriage {
    pub index: u32,
    pub item_slots: u32,
    pub fluid_capacity: i64,
    pub goods: GoodVec,
}

/// `Train.arriveAt(GlobalStation)` at a district station.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainCall {
    pub train: TrainId,
    pub station: StationId,
    pub district: DistrictId,
    /// The station's district is unloaded: the engine decides the exchange (I15).
    pub simulated: bool,
    pub carriages: Vec<Carriage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainDeparture {
    pub train: TrainId,
    pub station: StationId,
    pub district: DistrictId,
    pub destination: Option<StationId>,
    /// The Java side's estimate from graph distance and speed.
    pub eta_ticks: u64,
    pub carriages: Vec<Carriage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationDecl {
    pub station: StationId,
    pub district: Option<DistrictId>,
    pub dim: u32,
    pub pos: BlockPos,
    pub name: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

/// Train mail at a postbox (`GlobalStation.runMailTransfer`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEvent {
    pub district: DistrictId,
    pub inventory: u32,
    pub direction: Direction,
    /// NAME dictionary entry of the package's address.
    pub address: u32,
    pub packages: u32,
    pub op: Option<OpId>,
    pub goods: GoodVec,
}

/// Which plugin owns a deposit (I11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum DepositPluginId {
    OreExcavation,
    DieselOil,
    DestroyCrude,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepositSample {
    pub district: DistrictId,
    pub good: Good,
    pub plugin: DepositPluginId,
    pub chunk: ChunkPos,
    pub remaining: i64,
    pub infinite: bool,
    pub sweep: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweepEnd {
    pub district: DistrictId,
    pub sweep: u32,
    /// STOCK_SAMPLE and DEPOSIT_SAMPLE records sent for this sweep.
    pub records: u32,
    pub inventories: u32,
    /// Every chunk was block-ticking for the whole sweep.
    pub live: bool,
    pub creative: bool,
    pub overstressed: bool,
}

/// Season and day conditions (I12), global.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conditions {
    pub season: u32,
    pub season_count: u32,
    pub season_length: u64,
    /// The game time season 0 last began (or any earlier start of season 0).
    pub season_origin: Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultEntry {
    pub inventory: u32,
    pub good: Good,
    pub requested: i64,
    pub applied: i64,
}

/// What Java did with one CHUNK_RECONCILE.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileResult {
    pub op: OpId,
    pub district: DistrictId,
    pub epoch: u32,
    pub chunk: ChunkPos,
    pub entries: Vec<ResultEntry>,
    pub blocks_destroyed: u32,
    /// Blocks whose state no longer matched, so they were skipped (I23).
    pub blocks_skipped: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriageResultEntry {
    pub carriage: u32,
    pub good: Good,
    pub requested: i64,
    pub applied: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarriageResult {
    pub op: OpId,
    pub train: TrainId,
    pub entries: Vec<CarriageResultEntry>,
}

/// Natural spawns measured while a player was in the district at night (I20).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnSample {
    pub district: DistrictId,
    /// Night ticks with a player present in this sample.
    pub observed_ticks: u64,
    /// Dark spawnable blocks in the observed area (district plus ring).
    pub dark_blocks: u64,
    /// Natural hostile spawns per class (`FinalizeSpawnEvent`, spawn type NATURAL).
    pub spawns: [u32; ThreatClass::COUNT],
}

/// A real fight in a district (I21 calibration).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FightObserved {
    pub district: DistrictId,
    pub class: ThreatClass,
    pub attackers: u32,
    /// The attackers' total strength, in the same units as [`crate::threat::Threat::strength`].
    pub attacker_strength: f32,
    pub attackers_killed: u32,
    pub blocks_destroyed: u32,
    pub machines_destroyed: u32,
    pub guards_lost: u32,
    pub items_stolen: i64,
    pub duration_ticks: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockFlags(pub u32);

impl BlockFlags {
    pub const PERIMETER: u32 = 1;
    pub const ROOFLESS: u32 = 2;
    pub const MACHINE: u32 = 4;
    pub const INVENTORY: u32 = 8;

    pub fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// A block an attack could destroy, from the survey (I21, I23).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExposedBlock {
    pub pos: BlockPos,
    /// BLOCK_STATE dictionary entry; Java breaks the block only if it still matches.
    pub state: u32,
    pub flags: BlockFlags,
    pub inventory: Option<u32>,
}

/// The defence vector, recomputed on player edits (I21).
#[derive(Clone, Debug, PartialEq)]
pub struct DefenceSurvey {
    pub district: DistrictId,
    pub revision: u32,
    pub mob_griefing: bool,
    pub perimeter_columns: u32,
    /// Columns of the perimeter that do not block (lower than 2 above the walk surface, not a
    /// fence or wall).
    pub perimeter_gaps: u32,
    pub dark_spawnable: u32,
    pub machine_blocks: u32,
    /// Machine block entities without sky access.
    pub roofed_machines: u32,
    pub kill_machines_at_gaps: u32,
    /// Auto-firing turret strength against each class, ammunition permitting.
    pub turret_strength: [f32; ThreatClass::COUNT],
    pub guards: [u32; ThreatClass::COUNT],
    pub ammo_good: Option<Good>,
    pub ammo_per_engagement: u32,
}

/// Everything the Java side sends the industry engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Obs {
    // Latest-wins state.
    StockSample {
        district: DistrictId,
        inventory: u32,
        sweep: u32,
        goods: GoodVec,
    },
    SweepEnd(SweepEnd),
    DepositSample(DepositSample),
    ChannelSample {
        channel: ChannelId,
        capacity: i64,
        goods: GoodVec,
    },
    LedgerRequest {
        district: DistrictId,
    },
    /// Whether a player is near enough to fight for real (C18, INDUSTRY.md §13.1): sent when it
    /// changes and every few seconds while it holds (latest-wins).
    Watched {
        district: DistrictId,
        watched: bool,
    },
    // Journaled events.
    District(DistrictDecl),
    DistrictChunks {
        district: DistrictId,
        revision: u32,
        part: u32,
        chunks: Vec<(i32, i32)>,
    },
    Inventory(InventoryDecl),
    DistrictRemoved {
        district: DistrictId,
    },
    Edit {
        district: DistrictId,
        kind: EditKind,
        player: (u64, u64),
        pos: BlockPos,
    },
    Discard {
        district: DistrictId,
        reason: DiscardReason,
    },
    LoadState {
        district: DistrictId,
        state: LoadState,
        ticking_chunks: u32,
        total_chunks: u32,
    },
    ChunkLoaded {
        district: DistrictId,
        chunk: ChunkPos,
        applied_epoch: u32,
    },
    Conditions(Conditions),
    /// A PowerGrid transmission line feeds `sink`'s loads from `source`'s generators (I13); `None`
    /// removes the coupling.
    PowerLink {
        sink: DistrictId,
        source: Option<DistrictId>,
    },
    Port(PortTransfer),
    TrainArrived(TrainCall),
    TrainDeparted(TrainDeparture),
    Station(StationDecl),
    Package(PackageEvent),
    ReconcileResult(ReconcileResult),
    CarriageResult(CarriageResult),
    SpawnSample(SpawnSample),
    Fight(FightObserved),
    Survey(DefenceSurvey),
    SurveyBlocks {
        district: DistrictId,
        revision: u32,
        part: u32,
        blocks: Vec<ExposedBlock>,
    },
}

/// An observation with the game time it happened at.
#[derive(Clone, Debug, PartialEq)]
pub struct Timed {
    pub at: Tick,
    pub obs: Obs,
}

impl Obs {
    /// Kinds below 0x0100 are latest-wins; the bridge may drop them when a buffer is full.
    pub fn is_journaled(&self) -> bool {
        !matches!(
            self,
            Obs::StockSample { .. }
                | Obs::SweepEnd(_)
                | Obs::DepositSample(_)
                | Obs::ChannelSample { .. }
                | Obs::LedgerRequest { .. }
                | Obs::Watched { .. }
        )
    }
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    impl Persist for InvKind {
        fn put(&self, w: &mut Writer) {
            self.0.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(InvKind(Persist::get(r)?))
        }
    }

    impl Persist for NotLearnable {
        fn put(&self, w: &mut Writer) {
            self.code().put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            let c: u32 = Persist::get(r)?;
            match NotLearnable::from_code(c) {
                Some(n) => Ok(n),
                None => invalid("not-learnable code 0"),
            }
        }
    }

    persist_unit_enum!(
        DepositPluginId,
        [DepositPluginId::OreExcavation, DepositPluginId::DieselOil, DepositPluginId::DestroyCrude]
    );
    persist_struct!(InventoryDecl { district, revision, index, pos, kind, item_slots, fluid_capacity, energy_capacity });
    persist_struct!(DepositSample { district, good, plugin, chunk, remaining, infinite, sweep });
    persist_struct!(StationDecl { station, district, dim, pos, name });
    persist_struct!(Carriage { index, item_slots, fluid_capacity, goods });
    persist_struct!(Conditions { season, season_count, season_length, season_origin });
}
