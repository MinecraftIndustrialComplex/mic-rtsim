// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box (GLASSBOX.md, C28; Ben's G-B1 to G-B19): compiled Create factories.
//!
//! Phase G1 (G-B18, §11.3) is read only: the Java compiler sends each district's graph
//! ([`records`], [`codec`]); the engine stores it, solves it for the measured-behaviour floor L
//! and the LP ceiling U per good, and checks it against the boundary measurement for one window
//! (G-B14). Nothing it computes changes the world: the replay still runs the C19 black box.
//!
//! Choices made building it are numbered GB# in docs/choices-glassbox-g1.md.

pub mod codec;
pub mod records;

use rtsim_kernel::clock::Tick;

use records::GlassObs;

/// The glass box's state across districts.
#[derive(Debug, Default)]
pub struct Glass {
    /// Glass records received.
    pub records: u64,
}

impl Glass {
    /// One glass record.
    pub fn ingest(&mut self, _at: Tick, _g: &GlassObs) {
        self.records += 1;
    }
}
