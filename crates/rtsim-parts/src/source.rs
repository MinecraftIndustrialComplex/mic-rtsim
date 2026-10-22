// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where the state is (VIEWER.md §4.3): the engine's step tail, the headless host, or the sidecar
//! over a save. Every method takes `&self`: a source cannot change state.

use crate::desc::Layers;
use crate::mark::Tick;
use crate::part::{PartKey, PartWriter, RegionKey};

pub trait ViewSource {
    /// Layer descriptors, in the order [`PartKey::layer`] indexes.
    fn layers(&self) -> Layers;
    /// The units this source writes now (keys with no region: a stay-put layer's unit writes a
    /// part per region its rows fall in). Stable: the same thing is in the same unit every time.
    fn parts(&self) -> Vec<PartKey>;
    /// Writes one unit as of `now` as generic rows into `out` (a writer for the unit's layer).
    /// Returns why the layer would be empty, in its builder's words.
    fn write_part(&self, key: PartKey, now: Tick, out: &mut PartWriter) -> Option<String>;
    /// The Individual level for one region (the travellers on its roads, a herd's animals there):
    /// pure samples, never taken. A source with no Individual-level layer writes nothing.
    fn write_samples(&self, region: RegionKey, now: Tick, out: &mut PartWriter);
}
