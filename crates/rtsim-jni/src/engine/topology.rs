// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every level's topology (docs/boundary.md §11, docs/choices-topology.md): the TOPOLOGY records
//! the bridge sends, kept as one shared [`Topologies`] value and handed to every module that
//! measures the world.
//!
//! - **The record** (0x00B0, latest wins, `aux` = the dimension): Java sends one per level at
//!   every session start, before anything journaled is replayed, and again when a level's
//!   topology changes (Deep Time's wrap engine installing a layout ends its edge_stop). Its codec
//!   is `rtsim_space::topology::record`.
//! - **Session state** (TP6): the topologies are not saved. A world's layout is fixed, so the
//!   value Java sends at the next session start is the one the pages were made under; a frame
//!   nobody described is flat, which is every module's behaviour before this layer.
//! - **Handing it on** ([`sync`]): modules hold an `Arc` of the value and compare pointers, so a
//!   step that changes nothing costs a pointer comparison per module. A module replaced by a load
//!   or a switch gets it again at the next step.

use std::collections::BTreeMap;
use std::sync::Arc;

use rtsim_space::topology::record::{self, Source};
use rtsim_space::topology::{SharedTopologies, Topologies};

use super::{State, log_info, log_warn};
use crate::wire::Rec;

/// The topologies the step keeps (`State::topology`).
pub(super) struct Link {
    pub(super) shared: SharedTopologies,
    /// Who described each frame, for logs and `/rtsim perf`.
    pub(super) sources: BTreeMap<u16, Source>,
    pub(super) records: u64,
    pub(super) refused: u64,
}

impl Default for Link {
    fn default() -> Self {
        Link {
            shared: rtsim_space::topology::all_flat(),
            sources: BTreeMap::new(),
            records: 0,
            refused: 0,
        }
    }
}

/// A latest-wins record: TOPOLOGY.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != record::TOPOLOGY {
        return;
    }
    st.topology.records += 1;
    let frame = rec.aux as u16;
    let Some((spec, source)) = record::decode(rec.bytes) else {
        st.topology.refused += 1;
        log_warn(&format!(
            "a TOPOLOGY record for dimension {frame} does not decode; the level keeps its topology"
        ));
        return;
    };
    let t = match spec.build() {
        Ok(t) => t,
        Err(e) => {
            st.topology.refused += 1;
            log_warn(&format!(
                "the TOPOLOGY record for dimension {frame} from {} describes no world ({e}); the level keeps its topology",
                source.name()
            ));
            return;
        }
    };
    if *st.topology.shared.of(frame) == t {
        return;
    }
    let mut next: Topologies = (*st.topology.shared).clone();
    let what = describe(&t);
    next.set(frame, t);
    st.topology.shared = Arc::new(next);
    st.topology.sources.insert(frame, source);
    let name = st
        .world
        .java_dict
        .get(&(crate::wire::DICT_DIMENSION, u32::from(frame)))
        .cloned()
        .unwrap_or_default();
    log_info(&format!(
        "dimension {frame} ({name}): {what}, from {}",
        source.name()
    ));
    sync(st);
}

fn describe(t: &rtsim_space::topology::Topology) -> String {
    use rtsim_space::topology::Topology;
    match t {
        Topology::Flat { border: None } => "flat".into(),
        Topology::Flat { border: Some(b) } => {
            format!(
                "flat inside the border [{}, {}) x [{}, {})",
                b.x0, b.x1, b.z0, b.z1
            )
        }
        Topology::XWrap { x0, period, .. } => format!("x wraps every {period} blocks from {x0}"),
        Topology::Glued(g) => {
            let d = g.glued.domain();
            let stopped: Vec<&str> = g
                .glued
                .edges()
                .filter(|e| e.stopped)
                .map(|e| e.side.name())
                .collect();
            format!(
                "glued over [{}, {}) x [{}, {}), {} edges, {} cone points{}",
                d.x0,
                d.x1,
                d.z0,
                d.z1,
                g.glued.edges().count(),
                t.cones().len(),
                if stopped.is_empty() {
                    String::new()
                } else {
                    format!(", edge_stop on {}", stopped.join(" and "))
                }
            )
        }
    }
}

/// Hands the topologies to every module that measures the world. Cheap when nothing changed.
pub(super) fn sync(st: &mut State) {
    let t = &st.topology.shared;
    st.dragons.engine.set_topologies(t.clone());
    st.dragons.index.set_topologies(t.clone());
    st.herds.engine.set_topologies(t.clone());
    st.ships.engine.set_topologies(t.clone());
    st.ships.index.set_topologies(t.clone());
    st.conflict.engine.set_topologies(t.clone());
    st.conflict.index.set_topologies(t.clone());
}
