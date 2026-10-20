// SPDX-License-Identifier: AGPL-3.0-or-later
//! A minimal profiler: named zones, calls and microseconds per system.
//!
//! The host's profiler (Tracy in dev builds, a ring buffer for `/micperf`, PLAN.md §2) replaces the
//! sink later; the zone names and the start/stop calls in systems stay. One profiler per thread;
//! [`Profiler::merge`] combines workers' profilers after a parallel step.

use std::time::Instant;

/// A registered zone. Cheap to copy; look it up once, not per call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZoneId(u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ZoneStats {
    pub calls: u64,
    pub total_ns: u64,
    pub max_ns: u64,
}

/// One row of [`Profiler::report`].
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneReport {
    pub name: &'static str,
    pub calls: u64,
    pub total_us: f64,
    pub mean_us: f64,
    pub max_us: f64,
}

/// A started measurement. Pass it back to [`Profiler::stop`]; dropping it records nothing.
#[must_use]
pub struct Started {
    zone: ZoneId,
    at: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct Profiler {
    names: Vec<&'static str>,
    stats: Vec<ZoneStats>,
}

impl Profiler {
    pub fn new() -> Self {
        Self::default()
    }

    /// The zone called `name`, registered on first use.
    pub fn zone(&mut self, name: &'static str) -> ZoneId {
        if let Some(i) = self.names.iter().position(|n| *n == name) {
            return ZoneId(i as u16);
        }
        let id = u16::try_from(self.names.len()).expect("more than 65,535 zones");
        self.names.push(name);
        self.stats.push(ZoneStats::default());
        ZoneId(id)
    }

    pub fn start(&self, zone: ZoneId) -> Started {
        Started {
            zone,
            at: Instant::now(),
        }
    }

    pub fn stop(&mut self, s: Started) {
        let ns = u64::try_from(s.at.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.record(s.zone, ns);
    }

    pub fn record(&mut self, zone: ZoneId, ns: u64) {
        let z = &mut self.stats[zone.0 as usize];
        z.calls += 1;
        z.total_ns += ns;
        z.max_ns = z.max_ns.max(ns);
    }

    /// Runs `f` inside `zone`.
    pub fn measure<R>(&mut self, zone: ZoneId, f: impl FnOnce() -> R) -> R {
        let s = self.start(zone);
        let r = f();
        self.stop(s);
        r
    }

    pub fn stats(&self, zone: ZoneId) -> ZoneStats {
        self.stats[zone.0 as usize]
    }

    /// Adds another profiler's counts, matching zones by name.
    pub fn merge(&mut self, other: &Profiler) {
        for (name, s) in other.names.iter().zip(&other.stats) {
            let id = self.zone(name);
            let z = &mut self.stats[id.0 as usize];
            z.calls += s.calls;
            z.total_ns += s.total_ns;
            z.max_ns = z.max_ns.max(s.max_ns);
        }
    }

    pub fn reset(&mut self) {
        for s in &mut self.stats {
            *s = ZoneStats::default();
        }
    }

    /// Every zone with at least one call, most total time first.
    pub fn report(&self) -> Vec<ZoneReport> {
        let mut rows: Vec<ZoneReport> = self
            .names
            .iter()
            .zip(&self.stats)
            .filter(|(_, s)| s.calls > 0)
            .map(|(&name, s)| ZoneReport {
                name,
                calls: s.calls,
                total_us: s.total_ns as f64 / 1e3,
                mean_us: s.total_ns as f64 / 1e3 / s.calls as f64,
                max_us: s.max_ns as f64 / 1e3,
            })
            .collect();
        rows.sort_by(|a, b| b.total_us.total_cmp(&a.total_us));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_count_and_merge() {
        let mut p = Profiler::new();
        let a = p.zone("trade.clear");
        assert_eq!(p.zone("trade.clear"), a);
        let b = p.zone("travel.legs");
        p.measure(a, || std::hint::black_box(1 + 1));
        p.record(a, 2_000);
        p.record(b, 500);
        assert_eq!(p.stats(a).calls, 2);
        assert!(p.stats(a).max_ns >= 2_000);

        let mut q = Profiler::new();
        let b2 = q.zone("travel.legs");
        q.record(b2, 1_500);
        p.merge(&q);
        assert_eq!(p.stats(b).calls, 2);
        assert_eq!(p.stats(b).total_ns, 2_000);
        let r = p.report();
        assert_eq!(r[0].name, "trade.clear");
        assert_eq!(r[1].mean_us, 1.0);
        p.reset();
        assert!(p.report().is_empty());
    }
}
