// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box's records (GLASSBOX.md §1.4, G-B19; docs/industry-boundary.md §9): the compiled
//! graph of a district, sent by the Java compiler as latest-wins records with a compile number
//! and a record count, as stock sweeps are (F5), plus the probe and chunk-ticking kinds the
//! design names (§5.3, §6.1).
//!
//! The graph is the design's node and edge tables (§1.1, §1.2) in wire form. Everything a solve
//! needs is in these records; the engine never calls Java (PLAN.md §3.2). Java has already run
//! each machine's own countdown (G-B5), so a node's rate table holds exact tick counts per speed
//! level, recipe and batch.
//!
//! Units: items are counted in items, fluids in millibuckets, time in ticks. Amounts per craft
//! and rates are exact rationals (§3.7).

use serde::Serialize;

use crate::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodVec};

/// `Option<u32>` index on the wire: none.
pub const NONE: u32 = 0xFFFF_FFFF;

/// What every graph record carries after its game time: the district, the compile it belongs to
/// (Java's per-district counter, from 1) and the record's index within its kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct GraphHead {
    pub district: DistrictId,
    pub compile: u32,
    pub index: u32,
}

/// A node's kind (GLASSBOX.md §1.1): the header's `aux` of IND_GRAPH_NODE.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum NodeKind {
    /// Boundary stock (I3): a vault, chest, tank, packager target or postbox. Its `inventory` is
    /// the district's IND_INVENTORY index.
    Store,
    /// Work in progress: a basin, depot, belt's contents or chute slot. Passes goods; its capacity
    /// only decides blocked or not. Probes read it (§5.3).
    Buffer,
    /// Turns goods into goods by its recipes, at the rates of its table.
    Machine,
    /// Moves goods at a capacity shared by every good it carries: funnel, arm, hopper, chute,
    /// belt, pump, chain conveyor, packager, frogport.
    Mover,
    /// Splits or merges by its rule: brass and andesite tunnels, pipe branches, belt side
    /// insertion.
    Junction,
    /// Heat from fuel (a blaze burner): burns its fuel recipe whenever lit; HEAT edges carry its
    /// level to machines and boilers.
    Heater,
    /// Goods from outside the graph at a closed-form rate: animals (G-B17, C32), a water source,
    /// a cobblestone generator, a deposit.
    Source,
    /// Goods leave the world (a fan burning items, a void).
    Sink,
    /// A measured process (G-B13): the residual for machines no model claims.
    BlackBox,
    /// Gates nodes by stock: a threshold switch or factory gauge (G-B8). OBSERVE edges name what
    /// it reads, CONTROL edges what it gates.
    Controller,
    /// Where goods cross the district (a station, postbox, ship dock): events only.
    Port,
    /// A steam boiler (GLASSBOX.md §2.5): takes water and heat; its level drives a kinetic
    /// domain's speed level (the domain's `driver`).
    Boiler,
}

/// An edge's kind (GLASSBOX.md §1.2): the header's `aux` of IND_GRAPH_EDGE.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum EdgeKind {
    /// A machine, mover or store next to another: instant, as the target accepts.
    Adjacency,
    Belt,
    Chute,
    Pipe,
    Chain,
    /// An air current over a belt or depot (fan processing).
    AirZone,
    /// A mechanical arm's interaction point.
    ArmReach,
    /// Heat from a heater to a machine or boiler (`param`: the heat level it carries).
    Heat,
    /// A controller gating a node (`param`: [`CONTROL_STOP_WHEN_ON`] or [`CONTROL_RUN_WHEN_ON`]).
    Control,
    /// What a controller reads (from a store to the controller).
    Observe,
    /// A periodic line (a train or ship), G3.
    Line,
    /// Item entities on the ground between a drop and a collector (a learned edge, §10).
    Drop,
}

/// A CONTROL edge's `param`: the gated node stops while the controller's signal is on (a powered
/// funnel locks).
pub const CONTROL_STOP_WHEN_ON: i64 = 1;
/// A CONTROL edge's `param`: the gated node runs only while the signal is on.
pub const CONTROL_RUN_WHEN_ON: i64 = 2;

/// Node flags (IND_GRAPH_NODE at 44).
pub mod node_flags {
    /// The node's rule or rate is bounded, not exact (random routing, synchronize, a calibrated
    /// hand-off still TUNABLE): goods through it are checked against [L, U] (§3.5).
    pub const BOUNDED: u32 = 1;
    /// A machine that stops when any output is blocked (`MillstoneBlockEntity.tick`): the default
    /// for Create's processing machines.
    pub const ANY_OUTPUT_BLOCKS: u32 = 2;
    /// The rate is the expectation of a random process (a sheep's regrowth, G-B11): the
    /// checksum adds its variance (`params[2] / params[3]` per craft, second moment).
    pub const STOCHASTIC: u32 = 4;
    /// The node holds fluids (millibuckets).
    pub const FLUID: u32 = 8;
    /// The node's rate table includes a TUNABLE calibration (§2.9) not yet measured.
    pub const CALIBRATED: u32 = 16;
    /// The node lives on a moving frame (a Sable sub-level, C32): its position is in the frame.
    pub const FRAME: u32 = 32;
}

/// Edge flags (IND_GRAPH_EDGE at 44).
pub mod edge_flags {
    /// The filter list is a deny list (everything but these goods); else an allow list, and an
    /// empty allow list allows everything.
    pub const DENY: u32 = 1;
    pub const FLUID: u32 = 2;
    /// The rule on this edge is bounded (belt side insertion, a merge of equal priority).
    pub const BOUNDED: u32 = 4;
    /// The edge leaves `from` as one of its filtered sides (brass tunnels try them first).
    pub const FILTERED_SIDE: u32 = 8;
}

/// Rate entry flags.
pub mod rate_flags {
    /// The entry is the node's shared capacity (movers: units per tick across every good), not
    /// one recipe's.
    pub const SHARED: u32 = 1;
    /// The entry is the node's first cycle after a stop (wind-up), for latency (§4.4).
    pub const FIRST_CYCLE: u32 = 2;
}

/// Model codes (IND_GRAPH_NODE at 32): which of the design's templates (§7.1) or Create
/// components the node is. The solve reads the kind, the flags and the tables; the model code is
/// for reports, demotion by kind (G-B14) and calibration tables. Unknown codes are kept.
pub mod model {
    pub const UNKNOWN: u32 = 0;
    pub const STORE_ITEMS: u32 = 1;
    pub const STORE_FLUID: u32 = 2;
    pub const STORE_ENERGY: u32 = 3;
    pub const MILLSTONE: u32 = 10;
    pub const CRUSHER: u32 = 11;
    pub const PRESS: u32 = 12;
    pub const MIXER: u32 = 13;
    pub const SAW: u32 = 14;
    pub const DEPLOYER: u32 = 15;
    pub const FAN_ZONE: u32 = 16;
    pub const CRAFTER: u32 = 17;
    pub const SPOUT: u32 = 18;
    pub const DRAIN: u32 = 19;
    pub const BASIN: u32 = 20;
    pub const TIMED: u32 = 21;
    pub const SEQUENCED: u32 = 22;
    pub const BELT: u32 = 30;
    pub const CHUTE: u32 = 31;
    pub const FUNNEL_ANDESITE: u32 = 32;
    pub const FUNNEL_BRASS: u32 = 33;
    pub const ARM: u32 = 34;
    pub const HOPPER: u32 = 35;
    pub const PUMP: u32 = 36;
    pub const TUNNEL_BRASS: u32 = 37;
    pub const TUNNEL_ANDESITE: u32 = 38;
    pub const DEPOT: u32 = 39;
    pub const SMART_CHUTE: u32 = 40;
    pub const PACKAGER: u32 = 41;
    pub const CHAIN_CONVEYOR: u32 = 42;
    pub const FROGPORT: u32 = 43;
    pub const PIPE: u32 = 44;
    pub const BLAZE_BURNER: u32 = 50;
    pub const BOILER: u32 = 51;
    pub const THRESHOLD_SWITCH: u32 = 60;
    pub const FACTORY_GAUGE: u32 = 61;
    pub const ANIMAL_SHEEP: u32 = 70;
    pub const SPAWN_SPACE: u32 = 71;
    pub const WATER_SOURCE: u32 = 72;
    pub const BLACK_BOX: u32 = 80;
    pub const SINK_BURN: u32 = 90;
    pub const SINK_VOID: u32 = 91;
}

/// Routing rules (IND_GRAPH_NODE at 64), GLASSBOX.md §3.3. The first seven are Create's brass
/// tunnel `SelectionMode` ordinals plus one.
pub mod rule {
    pub const NONE: u32 = 0;
    pub const SPLIT: u32 = 1;
    pub const FORCED_SPLIT: u32 = 2;
    pub const ROUND_ROBIN: u32 = 3;
    pub const FORCED_ROUND_ROBIN: u32 = 4;
    pub const PREFER_NEAREST: u32 = 5;
    pub const RANDOMIZE: u32 = 6;
    pub const SYNCHRONIZE: u32 = 7;
    /// An arm's or funnel's "the first that works" (priority in edge order).
    pub const PREFER_FIRST: u32 = 8;
    /// Merges of equal priority: proportional to supply (bounded).
    pub const MERGE: u32 = 9;
    /// Andesite tunnel: one item to each open side per passing stack.
    pub const ANDESITE_SPLIT: u32 = 10;
    /// Belt side insertion: the main line first, side inputs share the gaps (bounded).
    pub const SIDE_INSERT: u32 = 11;
}

/// Recipe types (IND_GRAPH_RECIPE at 32), for reports and the calibration tables.
pub mod recipe_type {
    pub const OTHER: u32 = 0;
    pub const MILLING: u32 = 1;
    pub const CRUSHING: u32 = 2;
    pub const PRESSING: u32 = 3;
    pub const MIXING: u32 = 4;
    pub const COMPACTING: u32 = 5;
    pub const CUTTING: u32 = 6;
    pub const DEPLOYING: u32 = 7;
    pub const SPLASHING: u32 = 8;
    pub const SMELTING: u32 = 9;
    pub const BLASTING: u32 = 10;
    pub const SMOKING: u32 = 11;
    pub const HAUNTING: u32 = 12;
    pub const CRAFTING: u32 = 13;
    pub const FILLING: u32 = 14;
    pub const EMPTYING: u32 = 15;
    pub const SEQUENCED: u32 = 16;
    pub const FUEL: u32 = 17;
    pub const SHEARING: u32 = 18;
    pub const STONECUTTING: u32 = 19;
    pub const ITEM_APPLICATION: u32 = 20;
    /// A mover's transfer (no transformation): used only in reports.
    pub const TRANSFER: u32 = 21;
    /// Boiler water into steam work (§2.5).
    pub const BOILING: u32 = 22;
}

/// Heat levels (recipe requirement at 36, HEAT edge `param`, heater `params[1]`): Create's
/// `HeatLevel` order (`BlazeBurnerBlock.HeatLevel`), with the recipe requirement read as "at
/// least".
pub mod heat {
    pub const NONE: u32 = 0;
    pub const SMOULDERING: u32 = 1;
    pub const FADING: u32 = 2;
    pub const KINDLED: u32 = 3;
    pub const SEETHING: u32 = 4;
    /// A recipe's `HeatCondition.HEATED`: not NONE and not SMOULDERING (`testBlazeBurner`).
    pub const NEEDS_HEATED: u32 = FADING;
    /// `HeatCondition.SUPERHEATED`: SEETHING.
    pub const NEEDS_SUPERHEATED: u32 = SEETHING;
}

/// One rate table entry (Rate24): at speed level `level` of the node's domain, the node completes
/// one operation of `recipe` (None: its transfer, or its only recipe) on a batch of `batch`
/// crafts (machines) or units (movers) every `ticks_num / ticks_den` ticks. A node outside any
/// domain uses level 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RateEntry {
    pub level: u32,
    pub recipe: Option<u32>,
    pub batch: u32,
    pub ticks_num: u32,
    pub ticks_den: u32,
    pub flags: u32,
}

/// IND_GRAPH_NODE (0x0048).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GraphNode {
    pub head: GraphHead,
    pub kind: NodeKind,
    /// A [`model`] code.
    pub model: u32,
    /// Its kinetic domain (IND_GRAPH_DOMAIN index), if it is kinetic.
    pub domain: Option<u32>,
    /// A store's IND_INVENTORY index.
    pub inventory: Option<u32>,
    /// [`node_flags`].
    pub flags: u32,
    /// Its block position in the compile's frame (the level, or a Sable sub-level's own frame).
    pub pos: BlockPos,
    /// A [`rule`] code for junctions, arms and funnels with several targets.
    pub rule: u32,
    /// A store's or buffer's capacity: item slots for items, millibuckets for fluids; 0 unknown.
    pub capacity: i64,
    /// Kind-specific values (docs/industry-boundary.md §9.2): a heater's remaining burn, level,
    /// burn ticks per fuel and insertion threshold; a boiler's size, engines, active heat and
    /// water supply; a controller's on-above, off-below, flags and level; an animal source's
    /// count, eligible count and the variance of one craft's output as a fraction.
    pub params: [i64; 4],
    pub rates: Vec<RateEntry>,
}

/// IND_GRAPH_EDGE (0x0049): goods (or heat, or a signal) from `from` to `to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GraphEdge {
    pub head: GraphHead,
    pub kind: EdgeKind,
    pub from: u32,
    pub to: u32,
    /// Branch order at `from` (0 first): distance order for prefer-nearest, index order for round
    /// robin and arm outputs.
    pub order: u32,
    /// [`edge_flags`].
    pub flags: u32,
    /// Units per tick as `(num, den)`; None: limited only by the nodes at its ends.
    pub capacity: Option<(u32, u32)>,
    pub latency: u32,
    /// Kind-specific: a HEAT edge's level, a CONTROL edge's mode, an entry size.
    pub param: i64,
    /// Allowed (or, with DENY, refused) goods; empty allows everything.
    pub filter: Vec<Good>,
}

/// One recipe term (Term24): `num / den` of `good` per craft. A chance output enters at its
/// expectation, `chance × count` (§1.3, G-B11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Term {
    pub good: Good,
    /// [`term_flags`].
    pub flags: u32,
    pub num: i64,
    pub den: i64,
}

pub mod term_flags {
    /// Held, not consumed (a deployer's tool on a shearing or item-application step).
    pub const CATALYST: u32 = 1;
    /// A chance output, at its expectation.
    pub const CHANCE: u32 = 2;
}

/// IND_GRAPH_RECIPE (0x004A): one recipe of the compile, referred to by rate entries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GraphRecipe {
    pub head: GraphHead,
    /// A [`recipe_type`] code.
    pub recipe_type: u32,
    /// The [`heat`] level the recipe needs at least.
    pub heat: u32,
    pub inputs: Vec<Term>,
    pub outputs: Vec<Term>,
    /// The recipe's own processing time, informative (the rate table has the cycles).
    pub duration: u32,
    /// The recipe id (NAME dictionary), 0 none.
    pub name: u32,
}

/// One kinetic speed level of a domain (Level24): the root source's speed and the domain's
/// capacity in SU at that level. `selector` is the value of the domain's driver that selects the
/// level (a boiler's level, 0 to 18), or 0 when the domain has no driver.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct DomainLevel {
    pub selector: u32,
    pub speed_num: i32,
    pub speed_den: u32,
    pub capacity: f64,
}

/// One member of a domain (Member24): `count` blocks of the same impact and ratio, optionally a
/// graph node. Speed is `ratio × the root's speed`; stress is `impact × |speed|`, summed in
/// double as `KineticNetwork.calculateStress` does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Member {
    pub node: Option<u32>,
    pub count: u32,
    pub ratio_num: i32,
    pub ratio_den: u32,
    pub impact: f64,
}

/// IND_GRAPH_DOMAIN (0x004B): one Create `KineticNetwork` (§1.1, §2.1).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GraphDomain {
    pub head: GraphHead,
    /// Create's network id (`KineticBlockEntity.network`).
    pub network: u64,
    /// The level in force at compile.
    pub current: u32,
    /// The node that selects the level (a boiler), if any.
    pub driver: Option<u32>,
    /// [`domain_flags`].
    pub flags: u32,
    /// Create's own stress and capacity at compile (`KineticNetwork`), for the compile check.
    pub live_stress: f64,
    pub live_capacity: f64,
    pub levels: Vec<DomainLevel>,
    pub members: Vec<Member>,
}

pub mod domain_flags {
    /// `isOverStressed()` was true at compile.
    pub const OVERSTRESSED: u32 = 1;
    /// Stress is disabled in the server config (never overstressed).
    pub const STRESS_DISABLED: u32 = 2;
}

/// IND_GRAPH_END (0x004C): closes a compile. The engine uses the compile only if it received
/// exactly `records` IND_GRAPH_NODE, EDGE, RECIPE and DOMAIN records with its number (F5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GraphEnd {
    pub district: DistrictId,
    pub compile: u32,
    /// [`end_flags`].
    pub flags: u32,
    pub records: u32,
    pub nodes: u32,
    pub edges: u32,
    pub recipes: u32,
    pub domains: u32,
    /// Why the district is not compilable, when [`end_flags::NOT_COMPILABLE`] is set (codes in
    /// docs/industry-boundary.md §9.2).
    pub reason: u32,
    /// The fingerprint the compile was taken at (G-B3).
    pub fingerprint: u64,
    /// The frame the positions are in: 0 the level; else the Sable sub-level's id (C32).
    pub frame: u64,
    /// The district revision the compile used.
    pub revision: u32,
    /// Ticks the compile spanned on the server (sliced extraction).
    pub compile_ticks: u32,
}

pub mod end_flags {
    /// Every block entity of the district was claimed by a model.
    pub const COMPLETE: u32 = 1;
    /// The district cannot be compiled (`reason`): the C19 box stays its only model.
    pub const NOT_COMPILABLE: u32 = 2;
    /// Some block entities are unknown: they are joined to the graph as one residual black box
    /// (G-B13).
    pub const BLACK_BOX: u32 = 4;
    /// A resend of a compile already sent (Java resends the last compile periodically, so a
    /// dropped record is redone).
    pub const RESEND: u32 = 8;
}

/// Not-compilable reasons (IND_GRAPH_END at 52).
pub mod not_compilable {
    pub const NONE: u32 = 0;
    /// The compile's fingerprint changed while it ran too many times.
    pub const UNSTABLE: u32 = 1;
    /// More nodes than `glass.maxNodes`.
    pub const TOO_BIG: u32 = 2;
    /// A moving contraption (I10) that the graph cannot hold.
    pub const CONTRAPTION: u32 = 3;
    pub const OTHER: u32 = 9;
}

/// IND_PROBE_SAMPLE (0x004D): a probed buffer's contents in a sweep (§5.3 localization). Not
/// counted in the sweep's records: a dropped probe sample only loses one probe point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProbeSample {
    pub district: DistrictId,
    pub sweep: u32,
    pub node: u32,
    pub compile: u32,
    pub goods: GoodVec,
}

/// IND_CHUNK_TICKING (0x040B, journaled): a chunk of a district started or stopped block-ticking
/// (§6.1, I7). Ticking is not the same as loaded: a loaded chunk outside the simulation distance
/// does not tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ChunkTicking {
    pub district: DistrictId,
    pub chunk: ChunkPos,
    pub ticking: bool,
}

/// The glass-box observations.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum GlassObs {
    Node(GraphNode),
    Edge(GraphEdge),
    Recipe(GraphRecipe),
    Domain(GraphDomain),
    End(GraphEnd),
    ProbeSample(ProbeSample),
    ChunkTicking(ChunkTicking),
}

impl GlassObs {
    pub fn is_journaled(&self) -> bool {
        matches!(self, GlassObs::ChunkTicking(_))
    }

    pub fn district(&self) -> DistrictId {
        match self {
            GlassObs::Node(n) => n.head.district,
            GlassObs::Edge(e) => e.head.district,
            GlassObs::Recipe(r) => r.head.district,
            GlassObs::Domain(d) => d.head.district,
            GlassObs::End(e) => e.district,
            GlassObs::ProbeSample(p) => p.district,
            GlassObs::ChunkTicking(c) => c.district,
        }
    }
}

/// IND_PROBE_SET (0x8406): the engine asks Java to add these nodes' buffers to the district's
/// sweeps (IND_PROBE_SAMPLE), until a new compile or an empty set (§5.3, G-B14 "search").
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProbeSet {
    pub district: DistrictId,
    pub compile: u32,
    pub nodes: Vec<u32>,
}
