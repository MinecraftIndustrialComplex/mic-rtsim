// SPDX-License-Identifier: AGPL-3.0-or-later
//! Phase 1's event archive hooks (VIEWER.md §10.1): villages joining, bases marked and unmarked,
//! houses, caravans and bands on the road, and trade shifts. The world records into its own tape
//! (and its knowledge into the knowledge's) where it already does the thing; the engine takes the
//! tapes each step. Nothing here is state: a restart starts the trade shifts' baselines again.
//!
//! Ids: a village or base by its node key, a caravan by its row handle, a band by its seed, an
//! actor (a player) by its knowledge handle.

use rtsim_archive::{At, Kind, Severity, kinds};
use rtsim_kernel::clock::Tick;
use rtsim_travel::legs::Path;

use crate::world::World;

/// A market's price index moving by more than this share since its last recorded value is a trade
/// shift (TUNABLE).
pub const SHIFT_SHARE: f64 = 0.10;

impl World {
    /// Node `i`'s place at `at`.
    pub(crate) fn node_at(&self, i: usize, at: Tick) -> At {
        let p = self.nodes[i].pos;
        At::new(at.0, u32::from(p.frame), p.x, p.z)
    }

    /// Node `i`'s archive id: its key.
    pub(crate) fn node_id(&self, i: usize) -> u128 {
        u128::from(self.nodes[i].key)
    }

    /// A booked path as one leg row.
    pub(crate) fn tape_leg(&mut self, kind: Kind, path: &Path, subject: u128) {
        self.tape.leg(
            kind,
            u32::from(path.frame),
            (path.t0.0, path.x0, path.z0),
            (path.t1.0, path.x1, path.z1),
            subject,
        );
    }

    /// Band `b`'s walk to stop `next`, booked as its stay before begins at boundary `nb`.
    pub(crate) fn tape_band_leg(&mut self, b: usize, next: usize, nb: u64) {
        let band = &self.bands[b];
        let n = band.route.len();
        let prev = (next + n - 1) % n;
        let q = self.cfg.trade.quantum;
        let t0 = q.at(band.next_departure(prev, nb));
        let t1 = q.at(band.stay_start(next, nb));
        let (a, z) = (
            self.nodes[band.route[prev]].pos,
            self.nodes[band.route[next]].pos,
        );
        let seed = band.seed;
        self.tape.leg(
            kinds::TRAVEL_BAND_LEG,
            u32::from(a.frame),
            (t0.0, a.x, a.z),
            (t1.0, z.x, z.z),
            u128::from(seed),
        );
    }

    /// Market `i`'s price index after one of its own events: a row when it moved more than
    /// [`SHIFT_SHARE`] since the last one recorded. The first index seen is the baseline, with no
    /// row, so a quiet market writes nothing and a restart writes no burst.
    pub(crate) fn trade_shift(&mut self, i: usize, at: Tick, ix: f64) {
        let Some(&was) = self.shift_seen.get(&i) else {
            self.shift_seen.insert(i, ix);
            return;
        };
        if was <= 0.0 || ((ix - was) / was).abs() <= SHIFT_SHARE {
            return;
        }
        self.shift_seen.insert(i, ix);
        let (place, id) = (self.node_at(i, at), self.node_id(i));
        // Magnitude: the index in thousandths of the anchor; the move in per mille is the payload's.
        let severity = if ((ix - was) / was).abs() > 3.0 * SHIFT_SHARE {
            Severity::Notable
        } else {
            Severity::Minor
        };
        self.tape
            .event_as(
                kinds::TRADE_SHIFT,
                severity,
                place,
                id,
                0,
                (ix * 1000.0).round() as i64,
            )
            .text(|| format!("price index {was:.2} to {ix:.2}"));
    }
}
