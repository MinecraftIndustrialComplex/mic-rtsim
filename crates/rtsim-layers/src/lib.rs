// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every rtsim module's layers (VIEWER.md §7): their descriptors and the builders that write them
//! as parts (rtsim-parts), over a [`Src`] of borrowed module state. One registry for both places
//! the state is: the engine's tap, over the live modules, and the save viewer, over a decoded save
//! (rtsim-view). Moved here from rtsim-view's V1 `layers.rs`, which wrote scenes straight from a
//! save.
//!
//! A builder writes the things whose keys are in its selection ([`Sel`]): everything at once for a
//! save, one unit at a time in the tap (rtsim-parts' module docs: a unit is a range of the layer's
//! keys). Builders read through `&self` only, never past `wnow`, and inside the kernel's
//! resident-only reads in the tap, where a row whose page is not resident reads as absent and the
//! tap marks the unit not resident.

pub mod industry;

use std::collections::BTreeMap;
use std::ops::Range;

use rtsim_conflict::Conflict;
use rtsim_conflict::engine::Target as PartyTarget;
use rtsim_ecology::Dragons;
use rtsim_ecology::dragon::{Purpose, State, Target};
use rtsim_ecology::herds::Herds;
use rtsim_kernel::clock::Tick;
use rtsim_parts::part::kind as part_kind;
use rtsim_parts::{Group, LayerDesc, Layers, Mark, PartKey, PartWriter, RegionKey, Value};
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_ships::Ships;
use rtsim_ships::engine::Role;
use rtsim_travel::edge::Medium;
use rtsim_world::{Via, World};

pub use industry::{DistrictMark, IndustrySource, NetMark, SectionMark, StoreMark, Table};

/// Java's dictionary kind for names (rtsim-jni's `DICT_NAME`).
pub const DICT_NAME: u32 = 0x11;

/// Layer groups, in panel order.
pub const GROUPS: [(&str, &str); 7] = [
    ("settlements", "Settlements"),
    ("trade", "Markets"),
    ("travel", "Roads and travel"),
    ("ecology", "Dragons and herds"),
    ("ships", "Ships"),
    ("conflict", "Conflict"),
    ("industry", "Industry"),
];

/// Java's names (the journal's dictionary in a save, the link's copy in the engine).
pub trait Names {
    fn name(&self, kind: u32, id: u32) -> Option<&str>;
}

impl Names for BTreeMap<(u32, u32), String> {
    fn name(&self, kind: u32, id: u32) -> Option<&str> {
        self.get(&(kind, id)).map(String::as_str)
    }
}

impl<S: std::hash::BuildHasher> Names for std::collections::HashMap<(u32, u32), String, S> {
    fn name(&self, kind: u32, id: u32) -> Option<&str> {
        self.get(&(kind, id)).map(String::as_str)
    }
}

/// No names.
impl Names for () {
    fn name(&self, _: u32, _: u32) -> Option<&str> {
        None
    }
}

/// What a builder reads: each module's state, if it runs, and the clock.
#[derive(Clone, Copy)]
pub struct Src<'a> {
    pub world: Option<&'a World>,
    pub dragons: Option<&'a Dragons>,
    pub herds: Option<&'a Herds>,
    pub ships: Option<&'a Ships>,
    pub conflict: Option<&'a Conflict>,
    pub industry: Option<&'a dyn IndustrySource>,
    pub names: &'a dyn Names,
    /// The world tick (the phase 1 clock the modules share).
    pub wnow: Tick,
    /// World tick minus game time (the link's test offset; 0 in a real world).
    pub offset: u64,
}

impl Src<'_> {
    /// The game tick of a world tick.
    pub fn game(&self, t: Tick) -> i64 {
        t.0.saturating_sub(self.offset) as i64
    }

    /// Whether the module a layer reads is there.
    pub fn has(&self, module: &str) -> bool {
        match module {
            "phase1" => self.world.is_some(),
            "dragons" => self.dragons.is_some(),
            "herds" => self.herds.is_some(),
            "ships" => self.ships.is_some(),
            "conflict" => self.conflict.is_some(),
            "industry" => self.industry.is_some(),
            _ => false,
        }
    }
}

/// The things a builder writes: every one, or those with keys in a range (one unit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sel {
    All,
    Keys(Range<u64>),
}

impl Sel {
    pub fn range(&self) -> Range<u64> {
        match self {
            Sel::All => 0..u64::MAX,
            Sel::Keys(r) => r.clone(),
        }
    }

    /// The indices of a vector of `len` things in the selection.
    pub fn idx(&self, len: usize) -> Range<usize> {
        let r = self.range();
        let n = len as u64;
        (r.start.min(n) as usize)..(r.end.min(n) as usize)
    }
}

type Build = fn(&Src, &Sel, &mut PartWriter) -> Option<String>;
type Units = fn(&Src, u64) -> Vec<u64>;

/// One layer: its descriptor, the module it reads, how it is bucketed, and its builder.
pub struct Spec {
    pub desc: LayerDesc,
    pub module: &'static str,
    /// Keys a unit (TUNABLE): at most `MAX_ROWS` rows a unit.
    pub per_unit: u64,
    /// Bucketed by region as well as by unit (things that stay put).
    pub stay_put: bool,
    build: Build,
    units: Units,
}

impl Spec {
    /// A writer for this layer's parts, the layer at index `layer`.
    pub fn writer(&self, layer: u16) -> PartWriter {
        PartWriter::marks(layer, self.per_unit, self.stay_put)
    }

    /// Writes the selected things. Returns why the layer would be empty, in its own words.
    pub fn build(&self, src: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
        (self.build)(src, sel, out)
    }

    /// The units that may hold things now, in order.
    pub fn units(&self, src: &Src) -> Vec<u64> {
        if !src.has(self.module) {
            return Vec::new();
        }
        (self.units)(src, self.per_unit)
    }

    /// The keys of unit `u`.
    pub fn keys(&self, u: u64) -> Range<u64> {
        u * self.per_unit..(u + 1).saturating_mul(self.per_unit)
    }

    /// This layer bucketed `per_unit` keys a unit (as another build of the engine did).
    pub fn with_per_unit(&self, per_unit: u64) -> Spec {
        Spec {
            desc: self.desc.clone(),
            per_unit: per_unit.max(1),
            ..*self
        }
    }
}

/// Units 0 up to the one holding key `bound - 1`.
fn dense(bound: u64, per: u64) -> Vec<u64> {
    (0..bound.div_ceil(per)).collect()
}

/// The units that hold at least one key, found by jumping from key to key.
fn sparse(per: u64, next: impl Fn(u64) -> Option<u64>) -> Vec<u64> {
    let mut out = Vec::new();
    let mut from = 0u64;
    while let Some(k) = next(from) {
        let u = k / per;
        out.push(u);
        match (u + 1).checked_mul(per) {
            Some(f) => from = f,
            None => break,
        }
    }
    out
}

fn world_units(f: fn(&World) -> usize) -> impl Fn(&Src, u64) -> Vec<u64> {
    move |s, per| s.world.map_or_else(Vec::new, |w| dense(f(w) as u64, per))
}

/// Every layer.
pub fn specs() -> Vec<Spec> {
    fn s(
        desc: LayerDesc,
        module: &'static str,
        per_unit: u64,
        stay_put: bool,
        build: Build,
        units: Units,
    ) -> Spec {
        Spec {
            desc,
            module,
            per_unit,
            stay_put,
            build,
            units,
        }
    }
    vec![
        s(
            LayerDesc::new(
                "settle.villages",
                "settlements",
                "Villages",
                "point",
                "#e0b04a",
            )
            .describe("Villages, player bases, havens and lairs, sized by population")
            .priority(10)
            .legend(&[
                ("Human village", "#e0b04a"),
                ("Ribbit village", "#9b6bd6"),
                ("Player base", "#4ab0e0"),
                ("Haven", "#d0453a"),
                ("Lair", "#8fa3b8"),
            ])
            .fields(&["Population"]),
            "phase1",
            500,
            true,
            villages,
            |s, per| world_units(|w| w.nodes.len())(s, per),
        ),
        s(
            LayerDesc::new("trade.prices", "trade", "Price index", "point", "#e0b04a")
                .describe(
                    "Each storehouse market's mean price over its goods' anchors (1 is at anchor)",
                )
                .priority(9)
                .off()
                .legend(&[
                    ("Cheap (under 0.9)", "#3a9e5f"),
                    ("Near anchor", "#e0b04a"),
                    ("Dear (over 1.1)", "#d0453a"),
                ]),
            "phase1",
            64,
            true,
            prices,
            |s, per| world_units(|w| w.nodes.len())(s, per),
        ),
        s(
            LayerDesc::new(
                "travel.roads",
                "travel",
                "Roads and lanes",
                "edge",
                "#8a7f6a",
            )
            .describe("Edges between markets, wider with more goods in transit")
            .priority(2)
            .legend(&[
                ("Road", "#8a7f6a"),
                ("Sea lane", "#4a90c0"),
                ("Sky lane", "#a0a0d8"),
            ]),
            "phase1",
            500,
            true,
            roads,
            |s, per| world_units(|w| w.edges.len())(s, per),
        ),
        s(
            LayerDesc::new("travel.caravans", "travel", "Caravans", "leg", "#c98a3a")
                .describe("Caravans on their legs between markets")
                .priority(6)
                .zoom(0.0, 256.0)
                .glyph("circle", 4.0)
                .legend(&[("Outbound", "#c98a3a"), ("Returning", "#7a5a2a")])
                .fields(&["Stage"]),
            "phase1",
            64,
            false,
            caravans,
            |s, per| world_units(|w| w.caravans.capacity_slots())(s, per),
        ),
        s(
            LayerDesc::new(
                "travel.messengers",
                "travel",
                "Messengers",
                "leg",
                "#6aa0d0",
            )
            .describe("News packets on the road: waiting at their origin until they leave")
            .priority(3)
            .zoom(0.0, 64.0)
            .glyph("circle", 3.0),
            "phase1",
            500,
            false,
            messengers,
            |s, per| {
                s.world.map_or_else(Vec::new, |w| {
                    sparse(per, |from| {
                        let from = u32::try_from(from).ok()?;
                        w.packets.range(from..).next().map(|(k, _)| u64::from(*k))
                    })
                })
            },
        ),
        s(
            LayerDesc::new(
                "travel.bands",
                "travel",
                "Touring bands",
                "point",
                "#9b6bd6",
            )
            .describe("Ribbit bands: staying at a stop, or walking to the next")
            .kinds(&["point", "leg"])
            .priority(5)
            .glyph("star", 5.0)
            .legend(&[("Staying", "#9b6bd6"), ("Walking", "#c7a8ef")]),
            "phase1",
            64,
            false,
            bands,
            |s, per| world_units(|w| w.bands.len())(s, per),
        ),
        s(
            LayerDesc::new(
                "ecology.lairs",
                "ecology",
                "Dragon lairs",
                "point",
                "#b04a4a",
            )
            .describe("Roosts and caves")
            .priority(8)
            .glyph("triangle", 6.0)
            .legend(&[
                ("Roost", "#b04a4a"),
                ("Cave", "#7a3a3a"),
                ("Vacant", "#999999"),
            ]),
            "dragons",
            128,
            true,
            lairs,
            |s, per| {
                s.dragons
                    .map_or_else(Vec::new, |d| dense(d.lairs.len() as u64, per))
            },
        ),
        s(
            LayerDesc::new("ecology.dragons", "ecology", "Dragons", "point", "#d0453a")
                .describe("Dragons where they are, and their flight legs")
                .kinds(&["point", "leg"])
                .priority(9)
                .glyph("diamond", 6.0)
                .legend(&[
                    ("Roosting", "#d0453a"),
                    ("Flying", "#ff7b54"),
                    ("Hunting", "#ffb347"),
                    ("Near a player", "#e5484d"),
                    ("Leaving", "#a05050"),
                ])
                .fields(&["State"]),
            "dragons",
            64,
            false,
            dragons,
            |s, per| {
                s.dragons
                    .map_or_else(Vec::new, |d| dense(u64::from(d.dragons.next_index()), per))
            },
        ),
        s(
            LayerDesc::new("ecology.hunts", "ecology", "Hunts", "edge", "#ffb347")
                .describe("A line from each hunting dragon to its target")
                .priority(4)
                .width(1.5)
                .legend(&[
                    ("Road", "#ffb347"),
                    ("Village", "#e0b04a"),
                    ("Home", "#e5484d"),
                    ("Herd", "#7fb069"),
                    ("District", "#b48ead"),
                ]),
            "dragons",
            64,
            false,
            hunts,
            |s, per| {
                s.dragons
                    .map_or_else(Vec::new, |d| dense(u64::from(d.dragons.next_index()), per))
            },
        ),
        s(
            LayerDesc::new("ecology.herds", "ecology", "Herds", "area", "#7fb069")
                .describe("Wild herds, a disc of their footprint")
                .priority(7)
                .fields(&["Animals"]),
            "herds",
            8,
            false,
            herds,
            |s, per| {
                s.herds
                    .map_or_else(Vec::new, |h| dense(u64::from(h.herds.next_index()), per))
            },
        ),
        s(
            LayerDesc::new(
                "ecology.herd_routes",
                "ecology",
                "Herd routes",
                "path",
                "#a8d08d",
            )
            .describe("Where each herd is walking")
            .priority(1)
            .zoom(0.0, 256.0)
            .width(1.0),
            "herds",
            64,
            false,
            herd_routes,
            |s, per| {
                s.herds
                    .map_or_else(Vec::new, |h| dense(u64::from(h.herds.next_index()), per))
            },
        ),
        s(
            LayerDesc::new("ships.docks", "ships", "Docks", "point", "#4a90c0")
                .describe("Sea and sky docks")
                .priority(7)
                .glyph("square", 5.0)
                .legend(&[("Sea dock", "#4a90c0"), ("Sky dock", "#a0a0d8")]),
            "ships",
            128,
            true,
            docks,
            |s, per| {
                s.ships
                    .map_or_else(Vec::new, |x| dense(x.docks.len() as u64, per))
            },
        ),
        s(
            LayerDesc::new("ships.lanes", "ships", "Lanes", "path", "#4a90c0")
                .describe("Ship lanes, tinted by the pirates' share")
                .priority(2)
                .width(2.0)
                .legend(&[
                    ("Safe", "#4a90c0"),
                    ("Threatened", "#e0b04a"),
                    ("Dangerous", "#d0453a"),
                ]),
            "ships",
            256,
            true,
            lanes,
            |s, per| {
                s.ships
                    .map_or_else(Vec::new, |x| dense(x.lanes.len() as u64, per))
            },
        ),
        s(
            LayerDesc::new("ships.ships", "ships", "Ships", "point", "#2a6fa0")
                .describe("Active ships where they are")
                .priority(8)
                .glyph("triangle", 5.0)
                .legend(&[
                    ("Merchant", "#2a6fa0"),
                    ("Pirate", "#d0453a"),
                    ("Navy", "#e5e5e5"),
                ])
                .fields(&["Role"]),
            "ships",
            128,
            false,
            ships,
            |s, per| {
                s.ships
                    .map_or_else(Vec::new, |x| dense(u64::from(x.ships.next_index()), per))
            },
        ),
        s(
            LayerDesc::new(
                "conflict.factions",
                "conflict",
                "Factions",
                "area",
                "#c0504d",
            )
            .describe("Each faction's camp and reach")
            .priority(6),
            "conflict",
            64,
            true,
            factions,
            |s, per| {
                s.conflict
                    .map_or_else(Vec::new, |c| dense(c.factions.len() as u64, per))
            },
        ),
        s(
            LayerDesc::new("conflict.feuds", "conflict", "Feuds", "edge", "#e5484d")
                .describe("From a faction to what it feuds with, coloured by step")
                .priority(5)
                .width(2.0)
                .legend(&[
                    ("Notice", "#a0a0a0"),
                    ("Envoy", "#e0b04a"),
                    ("Scouts", "#ffb347"),
                    ("Raid", "#ff7b54"),
                    ("Invasion", "#e5484d"),
                    ("Siege", "#8b1a1a"),
                ]),
            "conflict",
            64,
            true,
            feuds,
            |s, per| {
                s.conflict
                    .map_or_else(Vec::new, |c| dense(c.factions.len() as u64, per))
            },
        ),
        s(
            LayerDesc::new(
                "conflict.parties",
                "conflict",
                "War parties",
                "leg",
                "#e5484d",
            )
            .describe("Parties out: their route, and where they are on it")
            .kinds(&["leg", "path"])
            .priority(9)
            .glyph("cross", 6.0)
            .legend(&[
                ("Envoy", "#e0b04a"),
                ("Scouts", "#ffb347"),
                ("Raid", "#ff7b54"),
                ("Invasion", "#e5484d"),
                ("Siege", "#8b1a1a"),
                ("Plunder", "#b04ab0"),
            ])
            .fields(&["Kind"]),
            "conflict",
            64,
            false,
            parties,
            |s, per| {
                s.conflict
                    .map_or_else(Vec::new, |c| dense(c.parties.capacity_slots() as u64, per))
            },
        ),
        s(
            LayerDesc::new(
                "industry.districts",
                "industry",
                "Districts",
                "area",
                "#5ab4ac",
            )
            .describe("Industrial districts, outlined by their chunks")
            .priority(8)
            .legend_st(&[
                ("Unloaded, run by rtsim", "#2a8a82", None),
                ("Being learned", "#b7791f", Some("dash")),
                ("Loaded, run by the game", "#2e8b57", None),
                ("Partly loaded, paused", "#57606a", Some("dot")),
                (
                    "Stopped: damaged, edited or not learnable",
                    "#d1242f",
                    Some("bad"),
                ),
                ("Learned, idle this season", "#57606a", Some("dot")),
            ])
            .fields(&["State", "Chunks"]),
            "industry",
            256,
            true,
            districts,
            |s, per| industry_units(s, Table::Districts, per),
        ),
        s(
            LayerDesc::new(
                "industry.stations",
                "industry",
                "Stations",
                "point",
                "#5ab4ac",
            )
            .describe("Create train stations")
            .priority(7)
            .glyph("square", 5.0),
            "industry",
            128,
            true,
            stations,
            |s, per| industry_units(s, Table::Stations, per),
        ),
        s(
            LayerDesc::new(
                "industry.lines",
                "industry",
                "Train lines",
                "edge",
                "#5ab4ac",
            )
            .describe(
                "Goods trains between station pairs, beside the track, wider with more trips a day",
            )
            // Over the track (80) it follows, under the stations (85).
            .priority(82)
            .line("flow")
            .label_bpp(8.0),
            "industry",
            64,
            true,
            lines,
            |s, per| industry_units(s, Table::Lines, per),
        ),
        s(
            LayerDesc::new("industry.trains", "industry", "Trains", "leg", "#2f8f87")
                .describe("Booked train legs")
                .priority(6)
                .glyph("square", 4.0),
            "industry",
            500,
            false,
            trains,
            |s, per| industry_units(s, Table::Legs, per),
        ),
        s(
            LayerDesc::new(
                "industry.sections",
                "industry",
                "Sections",
                "area",
                "#2a8a82",
            )
            .describe(
                "A district's working lines between stores (the glass box's sections), as boxes, \
                 coloured by what the check has made of each; drawn at a block a pixel and closer",
            )
            .zoom(0.0, 1.0)
            .priority(9)
            .legend_st(&[
                ("Learning", "#b7791f", Some("dash")),
                ("Passed", "#2a8a82", None),
                ("Failed its window", "#d1242f", Some("bad")),
                ("Stale: edited since it was learned", "#d1242f", Some("bad")),
                ("Frozen: its replay fell back", "#57606a", Some("dot")),
                ("Replaying while the district is away", "#2e8b57", None),
            ])
            .fields(&["State", "Nodes", "Stores"]),
            "industry",
            256,
            true,
            sections,
            |s, per| industry_units(s, Table::Sections, per),
        ),
        s(
            LayerDesc::new(
                "industry.stores",
                "industry",
                "Package stores",
                "edge",
                "#b7791f",
            )
            .describe(
                "Packages in transit on a chain whose frogports sit in more than one district: an \
                 edge between each pair of its frogports, wider with more packages held",
            )
            // Over the track and the lines, under the stations.
            .priority(83)
            .line("chain")
            .fields(&["Packages", "Frogports"]),
            "industry",
            256,
            true,
            stores,
            |s, per| industry_units(s, Table::Stores, per),
        ),
        s(
            LayerDesc::new(
                "industry.networks",
                "industry",
                "Shared networks",
                "edge",
                "#7c5cbf",
            )
            .describe(
                "A Create kinetic network whose blocks lie in several districts: an edge from its \
                 first part's source to each other part's, a ring where only one part is known",
            )
            .priority(81)
            .legend_st(&[
                ("Coupled: the parts solve as one network", "#7c5cbf", None),
                (
                    "Not coupled: a part missing or disagreeing",
                    "#d1242f",
                    Some("bad"),
                ),
            ])
            .fields(&["Status", "Parts"]),
            "industry",
            256,
            true,
            networks,
            |s, per| industry_units(s, Table::Networks, per),
        ),
    ]
}

fn industry_units(s: &Src, t: Table, per: u64) -> Vec<u64> {
    s.industry
        .map_or_else(Vec::new, |i| sparse(per, |from| i.next_key(t, from)))
}

/// The layer list `/api/layers` answers.
pub fn layers(specs: &[Spec]) -> Layers {
    Layers {
        groups: GROUPS
            .iter()
            .map(|(id, label)| Group {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect(),
        layers: specs.iter().map(|s| s.desc.clone()).collect(),
    }
}

/// The registry as a [`rtsim_parts::ViewSource`] over one [`Src`].
pub struct Source<'a> {
    pub src: Src<'a>,
    pub specs: &'a [Spec],
}

impl rtsim_parts::ViewSource for Source<'_> {
    fn layers(&self) -> Layers {
        layers(self.specs)
    }

    fn parts(&self) -> Vec<PartKey> {
        let mut out = Vec::new();
        for (i, s) in self.specs.iter().enumerate() {
            for u in s.units(&self.src) {
                out.push(PartKey::unit(i as u16, u));
            }
        }
        out
    }

    fn write_part(
        &self,
        key: PartKey,
        now: rtsim_parts::Tick,
        out: &mut PartWriter,
    ) -> Option<String> {
        let _ = now;
        let s = self.specs.get(usize::from(key.layer))?;
        if key.kind != part_kind::MARKS || !self.src.has(s.module) {
            return None;
        }
        s.build(&self.src, &Sel::Keys(s.keys(key.unit)), out)
    }

    fn write_samples(&self, _region: RegionKey, _now: rtsim_parts::Tick, _out: &mut PartWriter) {
        // No rtsim layer has an Individual level yet (VIEWER.md §6): nothing to sample.
    }
}

fn xz(p: Pos) -> [f64; 2] {
    [f64::from(p.x), f64::from(p.z)]
}

fn node_ci(k: NodeKind) -> u16 {
    match k {
        NodeKind::Village(Culture::Human) => 0,
        NodeKind::Village(Culture::Ribbit) => 1,
        NodeKind::PlayerBase { .. } => 2,
        NodeKind::Haven(_) => 3,
        NodeKind::Lair => 4,
    }
}

fn villages(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    for i in sel.idx(w.nodes.len()) {
        let n = &w.nodes[i];
        let pop = w.settlements.get(n.settlement).map_or(0, |s| s.pop);
        let s = (f64::from(pop.max(1)).sqrt() / 4.0).clamp(0.6, 3.0);
        out.push_with(
            i as u64,
            u32::from(n.pos.frame),
            &Mark::point(i, f64::from(n.pos.x), f64::from(n.pos.z))
                .with_ci(node_ci(n.kind))
                .with_size(s),
            &[(0, Value::I64(i64::from(pop)))],
        );
    }
    None
}

/// A market's mean price over its goods' anchors (rtsim-trade's, which the archive's trade shifts
/// follow too).
pub fn price_index(m: &rtsim_trade::Market) -> Option<f64> {
    m.price_index()
}

fn prices(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    for i in sel.idx(w.nodes.len()) {
        let n = &w.nodes[i];
        let Some(ix) = w.try_market(i).as_ref().and_then(price_index) else {
            continue;
        };
        let ci = if ix < 0.9 {
            0
        } else if ix > 1.1 {
            2
        } else {
            1
        };
        out.push(
            i as u64,
            u32::from(n.pos.frame),
            &Mark::point(i, f64::from(n.pos.x), f64::from(n.pos.z))
                .with_ci(ci)
                .with_label(format!("{ix:.2}")),
        );
    }
    Some("no storehouse market has priced goods".into())
}

fn medium_ci(m: Medium) -> u16 {
    match m {
        Medium::Road => 0,
        Medium::Sea => 1,
        Medium::Sky => 2,
    }
}

fn roads(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    let b = w.cfg.trade.quantum.index(cx.wnow);
    let mut t = BTreeMap::new();
    for i in sel.idx(w.edges.len()) {
        let e = &w.edges[i];
        let (pa, pb) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
        if pa.frame != pb.frame {
            continue;
        }
        t.clear();
        for d in &e.dirs {
            d.in_transit(b, &mut t);
        }
        let items: i128 = t.values().sum::<i128>() / i128::from(rtsim_trade::MILLI);
        let width = 1.0 + (1.0 + items.max(0) as f64).log10();
        out.push(
            i as u64,
            u32::from(pa.frame),
            &Mark::Edge {
                id: Some(i.to_string()),
                a: xz(pa),
                b: xz(pb),
                w: Some(width),
                ci: Some(medium_ci(e.medium)),
                c: None,
            },
        );
    }
    None
}

fn caravans(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    for (h, c) in w.caravans.iter_range(sel.range()) {
        let (a, b) = (w.nodes[c.leg.from].pos, w.nodes[c.leg.to].pos);
        let (ci, stage) = match c.stage {
            rtsim_travel::caravan::Stage::Outbound => (0, "outbound"),
            rtsim_travel::caravan::Stage::Returning => (1, "returning"),
        };
        out.push_with(
            u64::from(h.index()),
            u32::from(a.frame),
            &Mark::leg(
                h.to_bits(),
                xz(a),
                xz(b),
                cx.game(c.leg.start),
                cx.game(c.leg.end),
            )
            .with_ci(ci),
            &[(0, Value::Str(stage.into()))],
        );
    }
    None
}

fn messengers(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    let q = w.cfg.trade.quantum;
    let r = sel.range();
    let lo = u32::try_from(r.start).unwrap_or(u32::MAX);
    let hi = u32::try_from(r.end).unwrap_or(u32::MAX);
    let all = matches!(sel, Sel::All);
    for (id, p) in w.packets.range(lo..) {
        if !all && *id >= hi {
            break;
        }
        let Via::Road(r) = p.via else { continue };
        let Some(e) = w.edges.get(r.edge as usize) else {
            continue;
        };
        let d = &e.dirs[usize::from(r.dir) & 1];
        let (a, b) = (w.nodes[d.from].pos, w.nodes[d.to].pos);
        let t0 = q.at(p.departs);
        let t1 = q.at(p.departs + e.news_walk().max(1));
        out.push(
            u64::from(*id),
            u32::from(a.frame),
            &Mark::leg(id, xz(a), xz(b), cx.game(t0), cx.game(t1)),
        );
    }
    Some("no news packet is on a road (packets with bands travel as the band)".into())
}

fn bands(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let w = cx.world?;
    let ql = w.cfg.trade.quantum.len();
    for i in sel.idx(w.bands.len()) {
        let band = &w.bands[i];
        let v = w.band_view(i, cx.wnow);
        let frame = u32::from(v.frame);
        match v.walking {
            Some((from, to, milli)) => {
                let stop = band
                    .route
                    .iter()
                    .enumerate()
                    .position(|(k, s)| *s == from && band.route[(k + 1) % band.route.len()] == to)
                    .unwrap_or(0);
                let walk = band.walks.get(stop).copied().unwrap_or(1).max(1) * ql;
                let gnow = cx.game(cx.wnow);
                let t0 = gnow - (walk as f64 * f64::from(milli) / 1000.0) as i64;
                out.push(
                    i as u64,
                    frame,
                    &Mark::leg(
                        i,
                        xz(w.nodes[from].pos),
                        xz(w.nodes[to].pos),
                        t0,
                        t0 + walk as i64,
                    )
                    .with_ci(1),
                );
            }
            None => out.push(
                i as u64,
                frame,
                &Mark::point(i, v.x as f64, v.z as f64)
                    .with_ci(0)
                    .with_size(f64::from(band.members.max(1)).sqrt().clamp(0.8, 2.0)),
            ),
        }
    }
    None
}

fn lairs(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let d = cx.dragons?;
    d.lairs.rows().for_each_in(sel.range(), |i, l| {
        let ci = if l.occupants.is_empty() {
            2
        } else {
            match l.kind {
                rtsim_ecology::dragon::LairKind::Roost => 0,
                rtsim_ecology::dragon::LairKind::Cave => 1,
            }
        };
        out.push(
            i,
            u32::from(l.pos.frame),
            &Mark::point(i, f64::from(l.pos.x), f64::from(l.pos.z))
                .with_ci(ci)
                .with_label(l.name.clone()),
        );
    });
    None
}

fn dragon_ci(s: &State) -> (u16, &'static str) {
    match s {
        State::Roosting => (0, "roosting"),
        State::Flying(_) => (1, "flying"),
        State::Hunting { .. } => (2, "hunting"),
        State::Embodied { .. } => (3, "near a player"),
        State::Leaving { .. } => (4, "leaving"),
    }
}

fn dragons(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let d = cx.dragons?;
    for (h, x) in d.dragons.iter_range(sel.range()) {
        let (ci, state) = dragon_ci(&x.state);
        let fields = [(0, Value::Str(state.into()))];
        if let (State::Flying(_), Some(l)) = (x.state, x.leg) {
            let p = l.path;
            out.push_with(
                u64::from(h.index()),
                u32::from(p.frame),
                &Mark::leg(
                    h.to_bits(),
                    [f64::from(p.x0), f64::from(p.z0)],
                    [f64::from(p.x1), f64::from(p.z1)],
                    cx.game(p.t0),
                    cx.game(p.t1),
                )
                .with_ci(ci)
                .with_label(x.name.clone()),
                &fields,
            );
            continue;
        }
        let (pos, _) = x.position(&d.topo, &d.lairs, cx.wnow);
        out.push_with(
            u64::from(h.index()),
            u32::from(pos.frame),
            &Mark::point(h.to_bits(), f64::from(pos.x), f64::from(pos.z))
                .with_ci(ci)
                .with_label(x.name.clone()),
            &fields,
        );
    }
    None
}

/// Where a hunt's target is, and its legend index.
pub fn target_pos(world: Option<&World>, t: &Target) -> Option<(u16, [f64; 2], u16)> {
    Some(match *t {
        Target::Road { edge, x, z, .. } => {
            let w = world?;
            let e = w.edges.get(edge as usize)?;
            (w.nodes[e.a].pos.frame, [f64::from(x), f64::from(z)], 0)
        }
        Target::Village { node } => {
            let p = world?.nodes.get(node as usize)?.pos;
            (p.frame, xz(p), 1)
        }
        Target::Home { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 2),
        Target::Herd { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 3),
        Target::District { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 4),
    })
}

/// The target of a dragon that is hunting or flying to a hunt.
pub fn hunt_target(s: &State) -> Option<Target> {
    match *s {
        State::Hunting { target, .. } => Some(target),
        State::Flying(Purpose::Hunt(t)) => Some(t),
        State::Embodied {
            purpose: Purpose::Hunt(t),
            ..
        }
        | State::Leaving {
            purpose: Purpose::Hunt(t),
            ..
        } => Some(t),
        _ => None,
    }
}

fn hunts(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let d = cx.dragons?;
    for (h, x) in d.dragons.iter_range(sel.range()) {
        let Some(t) = hunt_target(&x.state) else {
            continue;
        };
        let Some((frame, b, ci)) = target_pos(cx.world, &t) else {
            continue;
        };
        let (pos, _) = x.position(&d.topo, &d.lairs, cx.wnow);
        if pos.frame != frame {
            continue;
        }
        out.push(
            u64::from(h.index()),
            u32::from(frame),
            &Mark::Edge {
                id: Some(h.to_bits().to_string()),
                a: xz(pos),
                b,
                w: None,
                ci: Some(ci),
                c: None,
            },
        );
    }
    Some("no dragon is hunting".into())
}

fn herds(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let hs = cx.herds?;
    for (h, x) in hs.herds.iter_range(sel.range()) {
        let (px, pz) = hs.position(h, cx.wnow);
        let r = f64::from(hs.footprint(h, cx.wnow).max(4));
        let n = hs.count(h, cx.wnow);
        out.push_with(
            u64::from(h.index()),
            u32::from(x.frame),
            &Mark::disc(Some(h.to_bits().to_string()), px as f64, pz as f64, r)
                .with_label(format!("{} ({n})", x.name)),
            &[(0, Value::I64(n as i64))],
        );
    }
    None
}

fn herd_routes(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let hs = cx.herds?;
    for (h, x) in hs.herds.iter_range(sel.range()) {
        if x.route.points.len() < 2 {
            continue;
        }
        out.push(
            u64::from(h.index()),
            u32::from(x.frame),
            &Mark::Path {
                id: Some(h.to_bits().to_string()),
                p: x.route
                    .points
                    .iter()
                    .map(|&(a, b)| [f64::from(a), f64::from(b)])
                    .collect(),
                ci: None,
                c: None,
                w: None,
            },
        );
    }
    Some("every herd is staying put".into())
}

fn docks(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let s = cx.ships?;
    for i in sel.idx(s.docks.len()) {
        let d = &s.docks[i];
        out.push(
            i as u64,
            u32::from(d.frame),
            &Mark::point(i, f64::from(d.x), f64::from(d.z))
                .with_ci(u16::from(d.medium == Medium::Sky)),
        );
    }
    None
}

fn lanes(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let s = cx.ships?;
    for i in sel.idx(s.lanes.len()) {
        let l = &s.lanes[i];
        let share = s.lane_share(i as u32);
        let ci = if share >= 200 {
            2
        } else if share > 0 {
            1
        } else {
            0
        };
        out.push(
            i as u64,
            u32::from(l.frame),
            &Mark::Path {
                id: Some(i.to_string()),
                p: l.path
                    .pts
                    .iter()
                    .map(|&(a, b)| [f64::from(a), f64::from(b)])
                    .collect(),
                ci: Some(ci),
                c: None,
                w: None,
            },
        );
    }
    None
}

fn ships(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let s = cx.ships?;
    let w = cx.world?;
    for (h, x) in s.ships.iter_range(sel.range()) {
        if !x.is_active() {
            continue;
        }
        let (px, pz) = s.position(w, h, cx.wnow);
        let (ci, role) = match x.role {
            Role::Merchant => (0, "merchant"),
            Role::Pirate { .. } => (1, "pirate"),
            Role::Navy { .. } => (2, "navy"),
        };
        out.push_with(
            u64::from(h.index()),
            u32::from(x.frame),
            &Mark::point(h.to_bits(), f64::from(px), f64::from(pz)).with_ci(ci),
            &[(0, Value::Str(role.into()))],
        );
    }
    Some("no ship is active".into())
}

/// A culture's colour on the map.
pub fn culture_color(i: u16) -> &'static str {
    const C: [&str; 8] = [
        "#c0504d", "#8064a2", "#4bacc6", "#f79646", "#9bbb59", "#7f7f7f", "#c00000", "#1f497d",
    ];
    C[usize::from(i) % C.len()]
}

fn factions(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let c = cx.conflict?;
    for f in sel.idx(c.factions.len()) {
        let f = f as u32;
        let Some(x) = c.factions.get(f) else { continue };
        out.push(
            u64::from(f),
            u32::from(x.pos.frame),
            &Mark::disc(
                Some(f.to_string()),
                f64::from(x.pos.x),
                f64::from(x.pos.z),
                f64::from(x.radius.max(16)),
            )
            .with_color(culture_color(x.culture))
            .with_label(c.faction_name(f)),
        );
    }
    None
}

fn feuds(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let c = cx.conflict?;
    for f in sel.idx(c.factions.len()) {
        let f = f as u32;
        let Some(x) = c.factions.get(f) else { continue };
        for (actor, feud) in c.feuds_of(f) {
            let to = feud.focus.map(|fo| fo.pos).or_else(|| {
                let p = c.parties.get(feud.party?)?;
                party_target_pos(cx.world, &p.target)
            });
            let Some(to) = to else { continue };
            if to.frame != x.pos.frame {
                continue;
            }
            let ci = (feud.step as u16).saturating_sub(1).min(5);
            out.push(
                u64::from(f),
                u32::from(x.pos.frame),
                &Mark::Edge {
                    id: Some(format!("{f}:{}", actor.to_bits())),
                    a: xz(x.pos),
                    b: xz(to),
                    w: None,
                    ci: Some(ci),
                    c: None,
                },
            );
        }
    }
    Some("no feud has a focus or a party out yet".into())
}

/// Where a party is headed.
pub fn party_target_pos(world: Option<&World>, t: &PartyTarget) -> Option<Pos> {
    match *t {
        PartyTarget::District(_, p) | PartyTarget::Pos(p) => Some(p),
        PartyTarget::Node(n) => Some(world?.nodes.get(n)?.pos),
    }
}

fn parties(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let c = cx.conflict?;
    for (h, p) in c.parties.iter_range(sel.range()) {
        let key = u64::from(h.index());
        let frame = u32::from(p.frame);
        let ci = p.kind.index() as u16;
        let fields = [(0, Value::Str(format!("{:?}", p.kind)))];
        let pts: Vec<[f64; 2]> = p
            .route
            .iter()
            .map(|&(a, b)| [f64::from(a), f64::from(b)])
            .collect();
        if pts.len() >= 2 {
            out.push(
                key,
                frame,
                &Mark::Path {
                    id: Some(h.to_bits().to_string()),
                    p: pts.clone(),
                    ci: Some(ci),
                    c: None,
                    w: Some(1.0),
                },
            );
        }
        let seg = (p.seg as usize).min(pts.len().saturating_sub(2));
        match (
            pts.get(seg),
            pts.get(seg + 1),
            p.times.get(seg),
            p.times.get(seg + 1),
        ) {
            (Some(a), Some(b), Some(t0), Some(t1)) => out.push_with(
                key,
                frame,
                &Mark::leg(h.to_bits(), *a, *b, cx.game(*t0), cx.game(*t1)).with_ci(ci),
                &fields,
            ),
            _ => {
                let (x, z) = c.party_pos(h, cx.wnow);
                out.push_with(
                    key,
                    frame,
                    &Mark::point(h.to_bits(), f64::from(x), f64::from(z)).with_ci(ci),
                    &fields,
                );
            }
        }
    }
    Some("no party is out".into())
}

/// A district state's legend index.
pub fn district_ci(state: &str) -> u16 {
    match state {
        "running" => 0,
        "learning" => 1,
        "live" => 2,
        "suspended" => 3,
        "out of season" => 5,
        _ => 4,
    }
}

/// The outline a district's legend entry draws (`dash` learning, `dot` idle, `bad` stopped).
pub fn district_st(ci: u16) -> Option<&'static str> {
    match ci {
        1 => Some("dash"),
        3 | 5 => Some("dot"),
        4 => Some("bad"),
        _ => None,
    }
}

fn districts(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.districts(sel.range(), &mut |d| {
        if d.chunks == 0 {
            return;
        }
        let ci = district_ci(d.state);
        let mut m = Mark::rings(Some(d.id.to_string()), d.block_rings())
            .with_ci(ci)
            .with_st(district_st(ci));
        if let Some(n) = cx.names.name(DICT_NAME, d.name) {
            m = m.with_label(n);
        }
        out.push_with(
            d.id,
            d.dim,
            &m,
            &[
                (0, Value::Str(d.state.into())),
                (1, Value::I64(i64::from(d.chunks))),
            ],
        );
    });
    Some("no district has chunks".into())
}

fn stations(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.stations(sel.range(), &mut |s| {
        let mut m = Mark::point(s.station.0, f64::from(s.pos.x), f64::from(s.pos.z));
        if let Some(n) = cx.names.name(DICT_NAME, s.name) {
            m = m.with_label(n);
        }
        out.push(s.station.0, s.dim, &m);
    });
    None
}

fn station_pos(v: &dyn IndustrySource, id: u64) -> Option<(u32, [f64; 2])> {
    let s = v.station(id)?;
    Some((s.dim, [f64::from(s.pos.x), f64::from(s.pos.z)]))
}

fn lines(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.lines(sel.range(), &mut |(a, b), l| {
        let (Some((fa, pa)), Some((fb, pb))) = (station_pos(v, a), station_pos(v, b)) else {
            return;
        };
        if fa != fb {
            return;
        }
        out.push(
            a,
            fa,
            &Mark::Edge {
                id: Some(format!("{a}-{b}")),
                a: pa,
                b: pb,
                w: Some(industry::line_width(l)),
                ci: None,
                c: None,
            },
        );
    });
    Some("no train ran between two known stations".into())
}

fn sections(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.sections(sel.range(), &mut |m| {
        let mark = Mark::rect(Some(format!("{}.{}", m.district, m.index)), m.block_box)
            .with_ci(m.state as u16)
            .with_st(industry::section_st(m.state))
            .with_label(format!(
                "Section {} of district {}: {}",
                m.index,
                m.district,
                industry::section_state_words(m.state)
            ));
        out.push_with(
            m.district,
            m.dim,
            &mark,
            &[
                (0, Value::Str(industry::section_state_words(m.state).into())),
                (1, Value::I64(i64::from(m.nodes))),
                (2, Value::I64(i64::from(m.stores))),
            ],
        );
    });
    Some(
        v.glass_why()
            .unwrap_or_else(|| "no compiled district has sections in this save".into()),
    )
}

fn stores(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.stores(sel.range(), &mut |s| {
        // Wider with the packages held: 2 for an empty store, up to 7.
        let w = (2.0 + 2.0 * (1.0 + s.packages as f64).log10()).min(7.0);
        for &(i, j) in &s.links {
            let (Some(a), Some(b)) = (s.ports.get(usize::from(i)), s.ports.get(usize::from(j)))
            else {
                continue;
            };
            out.push_with(
                s.id,
                s.dim,
                &Mark::Edge {
                    id: Some(format!("{}:{i}-{j}", s.id)),
                    a: [a.x, a.z],
                    b: [b.x, b.z],
                    w: Some(w),
                    ci: None,
                    c: None,
                },
                &[
                    (0, Value::I64(s.packages as i64)),
                    (1, Value::I64(s.ports.len() as i64)),
                ],
            );
        }
    });
    Some(
        v.stores_why()
            .unwrap_or_else(|| "no package store in this save".into()),
    )
}

fn networks(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.networks(sel.range(), &mut |n| {
        let ci = u16::from(!n.coupled);
        let fields = [
            (0, Value::Str(n.status.into())),
            (1, Value::I64(n.parts.len() as i64)),
        ];
        let Some(first) = n.parts.first() else {
            return;
        };
        if n.parts.len() == 1 {
            out.push_with(
                n.id,
                n.dim,
                &Mark::Point {
                    id: format!("{}", n.id),
                    x: first.x,
                    z: first.z,
                    ci: Some(ci),
                    c: None,
                    g: None,
                    s: None,
                    l: Some(format!("Network {}: {}", n.id, n.status)),
                    ring: true,
                },
                &fields,
            );
            return;
        }
        for (k, p) in n.parts.iter().enumerate().skip(1) {
            out.push_with(
                n.id,
                n.dim,
                &Mark::Edge {
                    id: Some(format!("{}:{k}", n.id)),
                    a: [first.x, first.z],
                    b: [p.x, p.z],
                    w: Some(3.0),
                    ci: Some(ci),
                    c: None,
                },
                &fields,
            );
        }
    });
    Some(
        v.glass_why()
            .unwrap_or_else(|| "no kinetic network spans two districts in this save".into()),
    )
}

fn trains(cx: &Src, sel: &Sel, out: &mut PartWriter) -> Option<String> {
    let v = cx.industry?;
    v.legs(sel.range(), &mut |id, l| {
        let Some((fa, pa)) = station_pos(v, l.from.0) else {
            return;
        };
        let pb =
            l.to.and_then(|t| station_pos(v, t.0))
                .filter(|(f, _)| *f == fa)
                .map_or(pa, |(_, p)| p);
        out.push(
            id,
            fa,
            &Mark::leg(id, pa, pb, l.depart.0 as i64, l.eta.0 as i64),
        );
    });
    Some("no train leg is booked".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_industry::engine::LineEdge;
    use rtsim_industry::obs::StationDecl;
    use rtsim_industry::threat::Leg;

    /// Every layer's unit holds at most `MAX_ROWS` keys, ids are unique, and a layer bucketed as
    /// another build did covers its keys with the other build's units.
    #[test]
    fn units_are_capped_and_rebucket() {
        let specs = specs();
        let mut ids: Vec<&str> = specs.iter().map(|s| s.desc.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), specs.len(), "unique layer ids");
        for s in &specs {
            assert!(
                (1..=u64::from(rtsim_parts::MAX_ROWS)).contains(&s.per_unit),
                "{}: {} keys a unit",
                s.desc.id,
                s.per_unit
            );
            let other = s.with_per_unit(s.per_unit * 3);
            assert_eq!(other.desc.id, s.desc.id);
            assert_eq!(other.stay_put, s.stay_put);
            assert_eq!(other.keys(1), s.per_unit * 3..s.per_unit * 6);
            assert_eq!(s.with_per_unit(0).per_unit, 1);
        }
    }

    /// An industry source that holds only slice S7's three tables.
    struct Reach {
        sections: Vec<SectionMark>,
        stores: Vec<StoreMark>,
        nets: Vec<NetMark>,
    }

    impl IndustrySource for Reach {
        fn districts(&self, _: Range<u64>, _: &mut dyn FnMut(DistrictMark)) {}
        fn stations(&self, _: Range<u64>, _: &mut dyn FnMut(&StationDecl)) {}
        fn station(&self, _: u64) -> Option<&StationDecl> {
            None
        }
        fn lines(&self, _: Range<u64>, _: &mut dyn FnMut((u64, u64), &LineEdge)) {}
        fn legs(&self, _: Range<u64>, _: &mut dyn FnMut(u64, &Leg)) {}
        fn sections(&self, ids: Range<u64>, f: &mut dyn FnMut(&SectionMark)) {
            for s in self.sections.iter().filter(|s| ids.contains(&s.district)) {
                f(s);
            }
        }
        fn stores(&self, ids: Range<u64>, f: &mut dyn FnMut(&StoreMark)) {
            for s in self.stores.iter().filter(|s| ids.contains(&s.id)) {
                f(s);
            }
        }
        fn networks(&self, ids: Range<u64>, f: &mut dyn FnMut(&NetMark)) {
            for s in self.nets.iter().filter(|s| ids.contains(&s.id)) {
                f(s);
            }
        }
        fn next_key(&self, t: Table, from: u64) -> Option<u64> {
            let keys: Vec<u64> = match t {
                Table::Sections => self.sections.iter().map(|s| s.district).collect(),
                Table::Stores => self.stores.iter().map(|s| s.id).collect(),
                Table::Networks => self.nets.iter().map(|s| s.id).collect(),
                _ => Vec::new(),
            };
            keys.into_iter().filter(|&k| k >= from).min()
        }
    }

    fn section(district: u64, index: u32, state: u32) -> SectionMark {
        SectionMark {
            district,
            dim: 0,
            index,
            state,
            working: true,
            nodes: 12,
            stores: 3,
            shared_stores: 1,
            unit: Some(0),
            block_box: [0.0, 0.0, 10.0, 10.0],
        }
    }

    fn port(x: f64, district: u64) -> industry::StorePort {
        industry::StorePort {
            x,
            z: 0.0,
            district: Some(district),
            star: false,
            packages: 4,
        }
    }

    fn layer<'a>(specs: &'a [Spec], id: &str) -> (usize, &'a Spec) {
        specs
            .iter()
            .enumerate()
            .find(|(_, s)| s.desc.id == id)
            .unwrap_or_else(|| panic!("layer {id}"))
    }

    /// Slice S7's layers draw one box a section, an edge a linked pair of frogports and an edge
    /// a part of a shared network; the sections only at a block a pixel and closer.
    #[test]
    fn the_reach_layers_draw_sections_stores_and_networks() {
        let names: BTreeMap<(u32, u32), String> = BTreeMap::new();
        let reach = Reach {
            sections: vec![section(1, 0, 1), section(1, 1, 2), section(2, 0, 0)],
            stores: vec![StoreMark {
                id: 7,
                dim: 0,
                ports: vec![port(0.0, 1), port(100.0, 2), port(200.0, 3)],
                links: vec![(0, 1), (1, 2), (0, 9)],
                packages: 12,
                residue_packages: 0,
            }],
            nets: vec![
                NetMark {
                    id: 0,
                    dim: 0,
                    status: "coupled",
                    coupled: true,
                    parts: (1..=3)
                        .map(|d| industry::NetPart {
                            district: d,
                            x: d as f64 * 50.0,
                            z: 0.0,
                        })
                        .collect(),
                },
                NetMark {
                    id: 1,
                    dim: 0,
                    status: "not coupled",
                    coupled: false,
                    parts: vec![industry::NetPart {
                        district: 4,
                        x: 0.0,
                        z: 0.0,
                    }],
                },
            ],
        };
        let src = Src {
            world: None,
            dragons: None,
            herds: None,
            ships: None,
            conflict: None,
            industry: Some(&reach),
            names: &names,
            wnow: Tick(0),
            offset: 0,
        };
        let specs = specs();
        let rows = |id: &str| {
            let (i, s) = layer(&specs, id);
            let mut out = s.writer(i as u16);
            s.build(&src, &Sel::All, &mut out);
            out.rows()
        };
        assert_eq!(rows("industry.sections"), 3, "a box a section");
        // Two links with both ends known; the third names a port the store does not have.
        assert_eq!(rows("industry.stores"), 2, "an edge a linked pair");
        // Three parts make two edges from the first; one part makes a ring.
        assert_eq!(rows("industry.networks"), 3);
        let (_, s) = layer(&specs, "industry.sections");
        assert!(s.desc.max_bpp <= 1.0, "sections draw below a block a pixel");
        // Each layer's units cover its keys.
        let tables = [
            ("industry.sections", vec![1, 2]),
            ("industry.stores", vec![7]),
            ("industry.networks", vec![0, 1]),
        ];
        for (id, keys) in tables {
            let (_, s) = layer(&specs, id);
            let units = s.units(&src);
            let covered: Vec<u64> = keys
                .iter()
                .copied()
                .filter(|&k| units.iter().any(|&u| s.keys(u).contains(&k)))
                .collect();
            assert_eq!(covered, keys, "{id}: its units cover its keys");
        }
    }
}
