// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every module's layers (VIEWER.md §7), drawn from a save. The descriptors and builders are
//! rtsim-layers', shared with the engine's tap: here they write parts over the decoded save, and
//! the one reader ([`crate::parts`]) draws the parts into one scene per frame, as it draws the
//! live view store's.
//!
//! Each builder runs under `catch_unwind`: a paged module's reads panic when a page cannot be read
//! (a writer's compaction removed it under the viewer), and such a layer is reported, not fatal.

use std::panic::{AssertUnwindSafe, catch_unwind};

use mic_viewer::api::Layers;
use rtsim_conflict::engine::Target as PartyTarget;
use rtsim_ecology::dragon::Target;
use rtsim_kernel::clock::Tick;
use rtsim_layers::{IndustrySource, Sel, Src};
use rtsim_settle::Pos;

pub use crate::parts::Drawn;
pub use rtsim_layers::{GROUPS, Spec, culture_color, district_ci, hunt_target, price_index, specs};

use crate::load::{Saved, Status};

impl rtsim_layers::Names for crate::journal::Names {
    fn name(&self, kind: u32, id: u32) -> Option<&str> {
        self.get(kind, id)
    }
}

/// The layer list `/api/layers` answers. A layer empty in this save says why through its marks
/// (`/api/marks` `empty`, from the scene), which the page shows under the layer's name.
pub fn layers(specs: &[Spec]) -> Layers {
    crate::parts::viewer_layers(&rtsim_layers::layers(specs))
}

/// What a builder reads.
pub struct Ctx<'a> {
    pub saved: &'a Saved,
    /// The save's world tick (the phase 1 clock the modules share).
    pub wnow: Tick,
}

impl Ctx<'_> {
    pub fn new(saved: &Saved) -> Ctx<'_> {
        let wnow = saved
            .phase1
            .as_ref()
            .map_or_else(|| Tick(saved.now().max(0) as u64), |p| p.world.now());
        Ctx { saved, wnow }
    }

    pub fn game(&self, t: Tick) -> i64 {
        self.saved.game(t)
    }

    /// The save's modules as the layers read them.
    pub fn src(&self) -> Src<'_> {
        let s = self.saved;
        Src {
            world: s.phase1.as_ref().map(|p| &p.world),
            dragons: s.dragons.as_ref(),
            herds: s.herds.as_ref(),
            ships: s.ships.as_ref(),
            conflict: s.conflict.as_ref(),
            industry: s.industry.as_ref().map(|i| i as &dyn IndustrySource),
            names: &s.names,
            wnow: self.wnow,
            offset: s.phase1.as_ref().map_or(0, |p| p.offset),
        }
    }
}

/// Runs every builder over the save, into parts, and reads the parts. A layer whose builder
/// panics draws nothing at all.
pub fn build(saved: &Saved, specs: &[Spec]) -> Drawn {
    let cx = Ctx::new(saved);
    let src = cx.src();
    let mut drawn = Drawn::default();
    for (i, s) in specs.iter().enumerate() {
        let id = s.desc.id.clone();
        if let Some(why) = absent(saved, s.module) {
            drawn.counts.insert(id.clone(), 0);
            drawn.reasons.insert(id, why);
            continue;
        }
        let mut out = s.writer(i as u16);
        let r = catch_unwind(AssertUnwindSafe(|| s.build(&src, &Sel::All, &mut out)));
        match r {
            Ok(why) => {
                let mut n = 0;
                for (_, p) in out.finish() {
                    n += drawn.part(&id, &p.bytes).unwrap_or(0);
                }
                drawn.counts.insert(id.clone(), n);
                if n == 0 {
                    let why = why.map_or_else(
                        || "nothing of this kind in the save".into(),
                        |w| saved.scrub(&w),
                    );
                    drawn.reasons.insert(id, why);
                }
            }
            Err(p) => {
                let msg = saved.scrub(&crate::load::panic_text(p));
                drawn.counts.insert(id.clone(), 0);
                drawn
                    .reasons
                    .insert(id, format!("reading its pages failed ({msg})"));
            }
        }
    }
    drawn
}

/// Why a module's layers draw nothing at all in this save, if so.
pub fn absent(saved: &Saved, module: &str) -> Option<String> {
    match saved.module(module).map(|m| &m.status) {
        None | Some(Status::Absent) => Some(format!("no {module} pages in this save")),
        Some(Status::Unreadable(e)) => Some(format!("the {module} pages did not read: {e}")),
        Some(Status::Loaded) => None,
    }
}

/// Where a hunt's target is, and its legend index.
pub fn target_pos(saved: &Saved, t: &Target) -> Option<(u16, [f64; 2], u16)> {
    rtsim_layers::target_pos(saved.phase1.as_ref().map(|p| &p.world), t)
}

/// Where a party is headed.
pub fn party_target_pos(saved: &Saved, t: &PartyTarget) -> Option<Pos> {
    rtsim_layers::party_target_pos(saved.phase1.as_ref().map(|p| &p.world), t)
}
