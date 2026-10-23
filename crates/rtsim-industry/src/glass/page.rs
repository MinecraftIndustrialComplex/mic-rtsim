// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box's pages (DECISIONS.md G2-9, Ben 2026-10-21: "the glass box's state lives in the
//! page store"; choices G2-80 to G2-85 in docs/choices-glassbox-g2.md).
//!
//! - **One page per district's glass state** (`persist::GLASS_TABLE`, id = the district's id),
//!   written whole by a save when it changed since the one before, like the industry engine's
//!   other pages (docs/choices-industry-dirty-pages.md): the compile it holds as its records
//!   (the graph's indices are rebuilt by [`Graph::build`] on restore), the check's open window
//!   and its passed one, the solve and its regime cache, the glass replay's parts with their
//!   regime state, marks and running cycle, and a half-assembled compile.
//! - **The counters across districts** ([`GlassStats`], records received) go in the globals page.
//! - **Not saved:** the solve's wall-clock time (`Compiled::solve_micros`), a measurement of this
//!   process. Everything else is, so a restored engine replays exactly as one that never stopped
//!   (tested hash for hash in `tests.rs`).
//! - The page starts with [`GLASS_MAGIC`], so an older format fails by name. A page that does not
//!   decode makes the whole restore fail, as any other industry page does (the host then starts
//!   the industry engine afresh and says so).
//! - **Sections** (SECTIONS.md §5, slice S2): the compile's sections are not saved but recomputed
//!   from its records, and the page keeps their fingerprints and a hash of its counted stores
//!   (slice S3). A page whose keys are not those its compile cuts into now (a changed
//!   `glass.sectionsMax` or `glass.countedStoresMax`, or a changed fold), or one of
//!   the format before sections ([`GLASS_MAGIC_IGL1`]), before unit windows
//!   ([`GLASS_MAGIC_IGL2`]) or before the station discards ([`GLASS_MAGIC_IGL3`]), drops that
//!   district's glass state alone ([`PageRead::Dropped`]): the
//!   district learns again. The judging units are recomputed with the sections (a row cap's cut
//!   held, `DistrictGlass::sections_cap`), their bounds from the solve, and their saved windows are
//!   checked against them.

use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};

use super::GlassStats;
use super::check::DistrictGlass;
use super::graph::Graph;
use super::records::{
    DomainLevel, EdgeKind, GraphDomain, GraphEdge, GraphEnd, GraphHead, GraphNode, GraphRecipe,
    Member, NodeKind, RateEntry, Term,
};
use super::solve::{GoodBound, Regime, Solution, StoreEdge};
use super::store::{Assembler, Assembly};
use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

/// The glass page's magic ("IGL4": the glass page with judging units' windows and the station
/// discards' count, SECTIONS.md §5, choice SC63).
pub const GLASS_MAGIC: u32 = u32::from_le_bytes(*b"IGL4");

/// The magic of the glass page before the station discards' count (slices S2b, S2c): read as a
/// dropped district, never decoded.
pub const GLASS_MAGIC_IGL3: u32 = u32::from_le_bytes(*b"IGL3");

/// The magic of the glass page before sections (G2-9): read as a dropped district, never decoded.
pub const GLASS_MAGIC_IGL1: u32 = u32::from_le_bytes(*b"IGL1");

/// The magic of the glass page with sections but no unit windows (slice S2a): read as a dropped
/// district, never decoded.
pub const GLASS_MAGIC_IGL2: u32 = u32::from_le_bytes(*b"IGL2");

/// A district's glass page: the state, then its compile's cut keys (`Sections::cut_keys`: the
/// section fingerprints and its counted stores; none without a compile).
pub fn district_page(dg: &DistrictGlass) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(GLASS_MAGIC);
    dg.put(&mut w);
    dg.compiled
        .as_ref()
        .map(|c| c.sections.cut_keys())
        .put(&mut w);
    w.into_bytes()
}

/// What a glass page reads back to.
#[derive(Debug)]
pub enum PageRead {
    /// The district's glass state, boxed: it is large and a dropped read carries only why.
    Kept(Box<DistrictGlass>),
    /// The district's glass state is dropped, and why.
    Dropped(&'static str),
}

impl PageRead {
    /// The state, or None for a dropped one.
    pub fn kept(self) -> Option<DistrictGlass> {
        match self {
            PageRead::Kept(dg) => Some(*dg),
            PageRead::Dropped(_) => None,
        }
    }
}

/// The glass state a [`district_page`] holds, its sections recomputed with `sections_max`
/// (`glass.sectionsMax`) and `counted_max` (`glass.countedStoresMax`). An error only for a page
/// that does not decode.
pub fn read_district_page(b: &[u8], sections_max: usize, counted_max: usize) -> Result<PageRead> {
    let mut r = Reader::new(b);
    let magic = r.u32()?;
    if magic == GLASS_MAGIC_IGL1 {
        return Ok(PageRead::Dropped(
            "a page of the format before sections (IGL1)",
        ));
    }
    if magic == GLASS_MAGIC_IGL2 {
        return Ok(PageRead::Dropped(
            "a page of the format before unit windows (IGL2)",
        ));
    }
    if magic == GLASS_MAGIC_IGL3 {
        return Ok(PageRead::Dropped(
            "a page of the format before the station discards (IGL3)",
        ));
    }
    if magic != GLASS_MAGIC {
        return Err(CodecError::Invalid(format!(
            "glass page magic {magic:#010x}, not {GLASS_MAGIC:#010x}: another format"
        )));
    }
    let mut dg = DistrictGlass::get(&mut r)?;
    let stored = Option::<Vec<u64>>::get(&mut r)?;
    r.finish()?;
    match (dg.compiled.as_mut(), stored) {
        (None, None) => {}
        (Some(c), Some(fps)) => {
            // A row cap's cut holds below `glass.sectionsMax` (choice SC57).
            let max = dg
                .sections_cap
                .map_or(sections_max, |cap| (cap as usize).min(sections_max));
            c.sections =
                super::sections::Sections::cut_counted(&c.graph, max, sections_max, counted_max);
            if c.sections.cut_keys() != fps {
                return Ok(PageRead::Dropped(
                    "sections that are not those its compile cuts into now",
                ));
            }
            if !c.restore_units() {
                return invalid("glass unit windows of other units");
            }
        }
        _ => return invalid("glass page sections without their compile"),
    }
    Ok(PageRead::Kept(Box::new(dg)))
}

// ---- the compile's records (records.rs) ----------------------------------------------------

persist_struct!(GraphHead {
    district,
    compile,
    index
});
persist_unit_enum!(
    NodeKind,
    [
        NodeKind::Store,
        NodeKind::Buffer,
        NodeKind::Machine,
        NodeKind::Mover,
        NodeKind::Junction,
        NodeKind::Heater,
        NodeKind::Source,
        NodeKind::Sink,
        NodeKind::BlackBox,
        NodeKind::Controller,
        NodeKind::Port,
        NodeKind::Boiler,
    ]
);
persist_unit_enum!(
    EdgeKind,
    [
        EdgeKind::Adjacency,
        EdgeKind::Belt,
        EdgeKind::Chute,
        EdgeKind::Pipe,
        EdgeKind::Chain,
        EdgeKind::AirZone,
        EdgeKind::ArmReach,
        EdgeKind::Heat,
        EdgeKind::Control,
        EdgeKind::Observe,
        EdgeKind::Line,
        EdgeKind::Drop,
    ]
);
persist_struct!(RateEntry {
    level,
    recipe,
    batch,
    ticks_num,
    ticks_den,
    flags
});
persist_struct!(GraphNode {
    head,
    kind,
    model,
    domain,
    inventory,
    flags,
    pos,
    rule,
    capacity,
    params,
    rates
});
persist_struct!(GraphEdge {
    head,
    kind,
    from,
    to,
    order,
    flags,
    capacity,
    latency,
    param,
    filter
});
persist_struct!(Term {
    good,
    flags,
    num,
    den
});
persist_struct!(GraphRecipe {
    head,
    recipe_type,
    heat,
    inputs,
    outputs,
    duration,
    name
});
persist_struct!(DomainLevel {
    selector,
    speed_num,
    speed_den,
    capacity
});
persist_struct!(Member {
    node,
    count,
    ratio_num,
    ratio_den,
    impact
});
persist_struct!(GraphDomain {
    head,
    network,
    current,
    driver,
    flags,
    live_stress,
    live_capacity,
    levels,
    members
});
persist_struct!(GraphEnd {
    district,
    compile,
    flags,
    window,
    records,
    nodes,
    edges,
    recipes,
    domains,
    reason,
    fingerprint,
    frame,
    revision,
    compile_ticks
});

/// A compile as its records; its indices are rebuilt (the build is a function of the records).
impl Persist for Graph {
    fn put(&self, w: &mut Writer) {
        self.end.put(w);
        self.nodes.put(w);
        self.edges.put(w);
        self.recipes.put(w);
        self.domains.put(w);
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let end = GraphEnd::get(r)?;
        let nodes = Persist::get(r)?;
        let edges = Persist::get(r)?;
        let recipes = Persist::get(r)?;
        let domains = Persist::get(r)?;
        Graph::build(end, nodes, edges, recipes, domains)
            .map_err(|e| CodecError::Invalid(format!("glass graph: {e:?}")))
    }
}

persist_struct!(Assembly {
    compile,
    nodes,
    edges,
    recipes,
    domains,
    count,
    broken
});
persist_struct!(Assembler { asm });

// ---- the solve (solve.rs) -------------------------------------------------------------------

persist_struct!(Regime {
    held,
    room,
    read,
    gate_off,
    derate,
    demoted,
    burning
});
persist_struct!(GoodBound {
    good,
    class,
    in_l,
    out_l,
    in_u,
    out_u,
    in_u_lp,
    out_u_lp,
    in_u_lin,
    out_u_lin,
    var_rate,
    batch,
    why
});
persist_struct!(StoreEdge {
    edge,
    good,
    flow,
    into,
    class,
    why
});
persist_struct!(Solution {
    goods,
    per_store,
    rounds,
    converged,
    saturated,
    edge_flow,
    store_edges,
    edge_goods,
    dom_level,
    duty,
    fed,
    src_var,
    roll_var,
    has_unbounded,
    lp_solves,
    lp_skipped,
    lp_tight,
    work,
    lp_work,
    starved
});

persist_struct!(GlassStats {
    compiles_accepted,
    resends_ignored,
    compiles_dropped,
    compiles_stale,
    not_compilable,
    compiles_invalid,
    compiles_kept,
    solves,
    windows_completed,
    windows_failed,
    probe_sets_sent,
    demotions,
    probe_samples,
    replays,
    fallbacks,
    count_samples,
    count_sets_sent
});

impl Persist for DistrictGlass {
    fn put(&self, w: &mut Writer) {
        self.discards.put(w);
        self.last_discard.put(w);
        self.asm.put(w);
        self.compiled.put(w);
        self.not_compilable.put(w);
        self.ticking.put(w);
        self.demoted.put(w);
        self.replay.put(w);
        self.replay_model.put(w);
        self.replay_since.put(w);
        self.fallback.put(w);
        self.numbers_reset.put(w);
        self.sections_cap.put(w);
        self.stopped.put(w);
        self.away.put(w);
        self.joins.put(w);
    }
    fn get(r: &mut Reader<'_>) -> Result<Self> {
        let dg = DistrictGlass {
            discards: Persist::get(r)?,
            last_discard: Persist::get(r)?,
            asm: Persist::get(r)?,
            compiled: Persist::get(r)?,
            not_compilable: Persist::get(r)?,
            ticking: Persist::get(r)?,
            demoted: Persist::get(r)?,
            replay: Persist::get(r)?,
            replay_model: Persist::get(r)?,
            replay_since: Persist::get(r)?,
            fallback: Persist::get(r)?,
            numbers_reset: Persist::get(r)?,
            sections_cap: Persist::get(r)?,
            stopped: Persist::get(r)?,
            away: Persist::get(r)?,
            joins: Persist::get(r)?,
        };
        // The replay indexes its compile's nodes: a page whose two disagree is refused.
        if let (Some(c), Some(rp)) = (&dg.compiled, &dg.replay)
            && !rp.fits(c.graph.nodes.len())
        {
            return invalid("glass replay of another compile's nodes");
        }
        Ok(dg)
    }
}
