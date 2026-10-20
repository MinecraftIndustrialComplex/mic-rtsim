// SPDX-License-Identifier: AGPL-3.0-or-later
//! Timetables: where a ship is is a function of the clock (Veloren's airships, research §4).
//!
//! A **service** is a loop sailed one way round on a fixed timetable. Each hop of it has four
//! phases: **dock** at its first stop, **depart** (the harbour, or the ascent for a sky ship),
//! **cruise**, and **approach** (the harbour, or the descent) to the next stop. Durations are whole
//! ticks from the lane's length and the medium's speeds, so the service's period `P` is fixed.
//!
//! A ship on a service has a **clock offset** `o`, saved with it (Veloren saves neither its
//! offsets nor its route assignments and resets every airship on every boot): at tick `t` it is
//! at `(t + o) mod P` into the timetable. A restart, a load or a read at any moment gives the same
//! answer, so ships never teleport. Ships of one service are spaced `P / n` apart, never closer
//! than a dock stay plus both harbour phases, so one platform per service and stop is enough.
//!
//! **Altitude.** A sky ship's unloaded position is always at its service's cruise height except
//! when docked (Veloren `!5091`: ships unloaded while low got stuck in terrain on load); the
//! loaded guidance ([`Fix::guide_y`]) climbs and descends through the harbour phases for the
//! plugin to steer by. Sea ships sail at sea level.
//!
//! **Traffic control** is in the timetable itself (Veloren's `next_pilot` was never called):
//! platforms are assigned per (service, stop) at build time, at most the dock's berths, and the two
//! services of a loop cruise at different heights.

use rtsim_kernel::clock::Tick;
use rtsim_space::topology::{Iso, Topologies};
use rtsim_travel::Medium;
use serde::Serialize;

use crate::network::{Dock, Hop, Lane, Loop};
use crate::tunables::ShipTunables;

/// What a service is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ServiceKind {
    /// Merchant ships carrying the lanes' flows.
    Merchant,
    /// A navy's patrol (M2): its ships lower the hazard on the lanes they sail.
    Patrol,
}

/// One hop's timetable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Leg {
    pub hop: Hop,
    pub from_dock: u32,
    pub to_dock: u32,
    /// Ticks into the period where this hop's dock stay starts.
    pub start: u64,
    pub dock: u64,
    pub depart: u64,
    pub cruise: u64,
    pub approach: u64,
    /// Blocks covered departing and approaching.
    pub depart_blocks: u64,
    pub approach_blocks: u64,
    /// The platform at `from_dock`.
    pub platform: u8,
}

impl Leg {
    pub fn sail(&self) -> u64 {
        self.depart + self.cruise + self.approach
    }

    pub fn total(&self) -> u64 {
        self.dock + self.sail()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    pub kind: ServiceKind,
    pub medium: Medium,
    pub frame: u16,
    pub legs: Vec<Leg>,
    pub period: u64,
    pub cruise_y: i32,
    /// Ships it runs.
    pub ships: u32,
    /// For a patrol, its navy.
    pub navy: Option<u32>,
}

/// Which phase a ship is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Phase {
    Dock = 0,
    Depart = 1,
    Cruise = 2,
    Approach = 3,
}

/// Where a ship is at a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fix {
    /// Hop index in the service.
    pub leg: usize,
    /// Absolute hop number: loops completed × hops + `leg`. Identifies one voyage of one ship.
    pub voyage: u64,
    pub phase: Phase,
    pub x: i32,
    pub z: i32,
    /// The unloaded Y: the forced cruise height except when docked.
    pub y: i32,
    /// The loaded guidance Y: through the harbour phases.
    pub guide_y: i32,
    /// When this voyage's dock stay began, its departure, its cruise, its approach and its
    /// arrival, in ticks (signed: the first voyage may have started before tick 0).
    pub times: VoyageTimes,
    /// The motion from the lane's chart to the canonical frame at this fix (TP7): the identity
    /// from [`Service::fix`], the fold from [`Service::fix_in`].
    pub chart: Iso,
}

/// A voyage's phase boundaries in absolute ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoyageTimes {
    pub docked: i64,
    pub depart: i64,
    pub cruise: i64,
    pub approach: i64,
    pub arrive: i64,
}

fn ceil_div(a: f64, b: f64) -> u64 {
    ((a / b.max(1e-9)).ceil() as u64).max(1)
}

/// The timetable of a loop sailed as given. `dock_ticks` per stop (0 for a patrol's stops other
/// than its base, which it only passes).
pub fn service_of(
    l: &Loop,
    lanes: &[Lane],
    docks: &[Dock],
    tun: &ShipTunables,
    kind: ServiceKind,
    index: usize,
    dock_at: impl Fn(u32) -> u64,
) -> Service {
    let first = &lanes[l.hops[0].lane as usize];
    let medium = first.medium;
    let mt = tun.medium(medium);
    let mut legs = Vec::with_capacity(l.hops.len());
    let mut t = 0u64;
    for (j, h) in l.hops.iter().enumerate() {
        let lane = &lanes[h.lane as usize];
        let len = lane.path.length().max(1);
        let harbour = mt.harbour_blocks.min(len / 3);
        let slow = mt.speed * mt.slow_share;
        let cruise_len = len - 2 * harbour;
        let from_dock = l.stops[j];
        let to_dock = l.stops[(j + 1) % l.stops.len()];
        let dock = dock_at(from_dock);
        let leg = Leg {
            hop: *h,
            from_dock,
            to_dock,
            start: t,
            dock,
            depart: ceil_div(harbour as f64, slow),
            cruise: ceil_div(cruise_len as f64, mt.speed),
            approach: ceil_div(harbour as f64, slow),
            depart_blocks: harbour,
            approach_blocks: harbour,
            platform: 0,
        };
        t += leg.total();
        legs.push(leg);
    }
    let period = t.max(1);
    // Spacing: at least a dock stay plus both harbour phases between ships, so one platform per
    // service and stop holds one ship at a time.
    let gap = legs
        .iter()
        .map(|g| g.dock + g.depart + g.approach)
        .max()
        .unwrap_or(1)
        .max(1);
    let by_headway = period.div_ceil(tun.headway_ticks.max(1));
    let by_gap = (period / gap).max(1);
    let ships = (by_headway.min(by_gap) as u32).clamp(1, tun.max_ships_per_service.max(1));
    let cruise_y = match medium {
        Medium::Sky => tun.cruise_y[index % tun.cruise_y.len()],
        _ => tun.sea_level,
    };
    let _ = docks;
    Service {
        kind,
        medium,
        frame: first.frame,
        legs,
        period,
        cruise_y,
        ships,
        navy: None,
    }
}

impl Service {
    /// The hop a timetable time `tau` (`0..period`) falls in.
    pub fn leg_at(&self, tau: u64) -> usize {
        match self.legs.binary_search_by(|g| g.start.cmp(&tau)) {
            Ok(k) => k,
            Err(k) => k.saturating_sub(1),
        }
    }

    /// The offset of ship `k` of `n`, from the service's own phase.
    pub fn offset_of(&self, phase: u64, k: u32, n: u32) -> u64 {
        (phase + self.period * u64::from(k) / u64::from(n.max(1))) % self.period
    }

    /// The times of voyage `voyage` of a ship with clock offset `offset`.
    pub fn times(&self, offset: u64, voyage: u64) -> VoyageTimes {
        let n = self.legs.len() as u64;
        let (m, j) = (voyage / n, (voyage % n) as usize);
        let g = &self.legs[j];
        let docked = (m * self.period + g.start) as i64 - offset as i64;
        let depart = docked + g.dock as i64;
        let cruise = depart + g.depart as i64;
        let approach = cruise + g.cruise as i64;
        let arrive = approach + g.approach as i64;
        VoyageTimes {
            docked,
            depart,
            cruise,
            approach,
            arrive,
        }
    }

    /// Where a ship with clock offset `offset` is at `t`.
    pub fn fix(&self, lanes: &[Lane], docks: &[Dock], offset: u64, t: Tick) -> Fix {
        let s = t.0 + offset;
        let (m, tau) = (s / self.period, s % self.period);
        let j = self.leg_at(tau);
        let g = &self.legs[j];
        let voyage = m * self.legs.len() as u64 + j as u64;
        let times = self.times(offset, voyage);
        let u = tau - g.start;
        let lane = &lanes[g.hop.lane as usize];
        let len = lane.path.length();
        let (fd, td) = (&docks[g.from_dock as usize], &docks[g.to_dock as usize]);
        let frac = |a: u64, span: u64| -> f64 {
            if span == 0 {
                1.0
            } else {
                (a.min(span)) as f64 / span as f64
            }
        };
        let (phase, along, guide_y) = if u < g.dock {
            (Phase::Dock, 0u64, fd.y)
        } else if u < g.dock + g.depart {
            let f = frac(u - g.dock, g.depart);
            (
                Phase::Depart,
                (g.depart_blocks as f64 * f).round() as u64,
                lerp_i(fd.y, self.cruise_y, f),
            )
        } else if u < g.dock + g.depart + g.cruise {
            let f = frac(u - g.dock - g.depart, g.cruise);
            let mid = len - g.depart_blocks - g.approach_blocks;
            (
                Phase::Cruise,
                g.depart_blocks + (mid as f64 * f).round() as u64,
                self.cruise_y,
            )
        } else {
            let f = frac(u - g.dock - g.depart - g.cruise, g.approach);
            (
                Phase::Approach,
                len - g.approach_blocks + (g.approach_blocks as f64 * f).round() as u64,
                lerp_i(self.cruise_y, td.y, f),
            )
        };
        let s_ab = if g.hop.forward {
            along
        } else {
            len - along.min(len)
        };
        let (x, z) = lane.path.at(s_ab);
        let y = match (self.medium, phase) {
            (Medium::Sky, Phase::Dock) => fd.y,
            (Medium::Sky, _) => self.cruise_y,
            _ => self.cruise_y,
        };
        let guide_y = if self.medium == Medium::Sky {
            guide_y
        } else {
            self.cruise_y
        };
        Fix {
            leg: j,
            voyage,
            phase,
            x,
            z,
            y,
            guide_y,
            times,
            chart: Iso::IDENTITY,
        }
    }

    /// [`Service::fix`] on a world with seams: the position is canonical, and `chart` says how
    /// the lane's chart maps onto it there.
    pub fn fix_in(
        &self,
        topo: &Topologies,
        lanes: &[Lane],
        docks: &[Dock],
        offset: u64,
        t: Tick,
    ) -> Fix {
        let mut f = self.fix(lanes, docks, offset, t);
        let s = t.0 + offset;
        let j = self.leg_at(s % self.period);
        let frame = lanes[self.legs[j].hop.lane as usize].frame;
        let fold = topo.of(frame).fold(i64::from(f.x), i64::from(f.z));
        f.x = fold.x as i32;
        f.z = fold.z as i32;
        f.chart = fold.iso;
        f
    }

    /// Voyages of a ship with clock offset `offset` on hop `j` whose span from dock stay to
    /// arrival meets `[t0, t1]`: a closed form, a handful at most.
    pub fn voyages_meeting(&self, offset: u64, j: usize, t0: Tick, t1: Tick) -> Vec<u64> {
        let n = self.legs.len() as u64;
        let g = &self.legs[j];
        let p = self.period as i128;
        // docked(m) = m·P + start - offset; arrive(m) = docked(m) + total.
        let base = i128::from(g.start) - i128::from(offset);
        let total = i128::from(g.total());
        let lo = (i128::from(t0.0) - base - total).div_euclid(p);
        let hi = (i128::from(t1.0) - base).div_euclid(p);
        let mut out = Vec::new();
        for m in lo.max(0)..=hi {
            let docked = m * p + base;
            let arrive = docked + total;
            if arrive >= i128::from(t0.0) && docked <= i128::from(t1.0) {
                out.push(m as u64 * n + j as u64);
            }
        }
        out
    }

    /// The departure (end of the dock stay) at or before `t` of a ship with offset `offset` on
    /// hop `j`, as (voyage, tick); `None` before its first.
    pub fn departure_at_or_before(&self, offset: u64, j: usize, t: i64) -> Option<(u64, i64)> {
        let n = self.legs.len() as u64;
        let g = &self.legs[j];
        let p = self.period as i128;
        let base = i128::from(g.start) + i128::from(g.dock) - i128::from(offset);
        let m = (i128::from(t) - base).div_euclid(p);
        if m < 0 {
            return None;
        }
        Some((m as u64 * n + j as u64, (m * p + base) as i64))
    }

    pub fn heap_bytes(&self) -> usize {
        self.legs.capacity() * std::mem::size_of::<Leg>()
    }
}

fn lerp_i(a: i32, b: i32, f: f64) -> i32 {
    (f64::from(a) + (f64::from(b) - f64::from(a)) * f).round() as i32
}

/// Assigns platforms: per dock, one per (service, hop) that docks there, in service order. Returns
/// the docks over their berths, with how many they need.
pub fn assign_platforms(services: &mut [Service], docks: &[Dock]) -> Vec<(u32, u32)> {
    let mut used: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    for s in services.iter_mut() {
        for g in s.legs.iter_mut() {
            if g.dock == 0 {
                continue;
            }
            let k = used.entry(g.from_dock).or_insert(0);
            let berths = docks[g.from_dock as usize].berths.max(1);
            g.platform = (*k % berths) as u8;
            *k += 1;
        }
    }
    used.into_iter()
        .filter(|&(d, k)| k > docks[d as usize].berths.max(1))
        .collect()
}
