// SPDX-License-Identifier: AGPL-3.0-or-later
//! The rail meter (MOWZIE.md §4.3, slice M7; docs/choices-mowzie-build.md MZB-300 to MZB-349).
//!
//! A train through a Frostmaw's range is a *leg*. Java samples trains and reports each pass
//! through a den's circle (RAIL_PASS); this module counts them per (faction, actor) as a decayed
//! number of legs and turns the count into one continuous source a day-rate high, pushed only when
//! it has moved enough to matter. Nothing is ticked: each meter books one wake, the earlier of the
//! moment its rate leaves the band it was last pushed in and a day after the push.
//!
//! - **The count** is `n(t) = n e^(-(t - t_ref)/τ)` with τ two days ([`RAIL_TAU`]); a steady stream
//!   of `h`-tick headways settles at `n = 1 / (1 - e^(-h/τ))` and a rate `n × 24,000 / τ` legs a day
//!   that is `24,000 / h` to within the headway's own sampling. Adds are exact: a pass whose exit
//!   is before `t_ref` adds what it would have decayed to, so a late report counts as if it had
//!   come on time.
//! - **Lines** are the pairs of stations a train ran between, kept per meter (at most
//!   [`MAX_LINES`]); the busiest gives the source's position and pin (the focus of a raid).

use super::*;

/// The decay time of the leg count, in ticks: two days (PLACEHOLDER, MOWZIE.md §4.3).
pub const RAIL_TAU: f64 = 48_000.0;
/// A meter whose rate has fallen to this many legs a day, with nothing pushed above the culture's
/// tolerance, is retired (MZB-306).
pub const RETIRE_PER_DAY: f64 = 0.01;
/// Lines kept per meter; a new one beyond this evicts the one with the smallest count
/// (PLACEHOLDER, MZB-307).
pub const MAX_LINES: usize = 16;

/// Where a rail source sits on the track: the y of the track at the source's position and the
/// station nearer to it that has a district (MZB-313). A source that has one is a rail source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pin {
    pub y: i32,
    pub station: Option<(Pos, DistrictId)>,
}

/// What a feud's focus keeps of the pin that set it (MZB-314).
pub type TrackFocus = Pin;

/// The key of the source a faction's meter for one actor pushes to: `fnv1a64("rail" ++ the
/// faction's key ++ the actor's bits)` (MZB-313).
pub fn rail_key(faction_key: u64, actor: ActorId) -> u64 {
    let mut b = Vec::with_capacity(20);
    b.extend_from_slice(b"rail");
    b.extend_from_slice(&faction_key.to_le_bytes());
    b.extend_from_slice(&actor.to_bits().to_le_bytes());
    fnv1a64(&b)
}

/// `n` as it stands at `t`, for a count last set at `t_ref`. A `t` before `t_ref` reads `n`.
fn decayed(n: f64, t_ref: Tick, t: Tick) -> f64 {
    n * (-(t.0.saturating_sub(t_ref.0) as f64) / RAIL_TAU).exp()
}

/// One leg at `t` added to the count `n` last set at `t_ref` (exact for a late `t`).
fn add_leg(n: &mut f64, t_ref: &mut Tick, t: Tick) {
    if t >= *t_ref {
        *n = decayed(*n, *t_ref, t) + 1.0;
        *t_ref = t;
    } else {
        *n += (-((t_ref.0 - t.0) as f64) / RAIL_TAU).exp();
    }
}

/// The legs a day a count of `n` stands for.
fn legs_per_day(n: f64) -> f64 {
    n * TICKS_PER_DAY as f64 / RAIL_TAU
}

/// The legs run between two stations (the lower id first), counted like a meter's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineCount {
    pub a: u64,
    pub b: u64,
    pub n: f64,
    pub t_ref: Tick,
    /// The latest credited pass's point on the track, and its y: where a raid on this line goes.
    pub point: Pos,
    pub y: i32,
    /// Whichever of the two stations is nearer the point and has a district.
    pub near: Option<(Pos, DistrictId)>,
}

impl LineCount {
    pub fn n_at(&self, t: Tick) -> f64 {
        decayed(self.n, self.t_ref, t)
    }

    pub fn add(&mut self, t: Tick) {
        add_leg(&mut self.n, &mut self.t_ref, t);
    }

    pub fn per_day(&self, t: Tick) -> f64 {
        legs_per_day(self.n_at(t))
    }
}

/// One faction's count of one actor's legs through its range.
#[derive(Clone, Debug, PartialEq)]
pub struct RailMeter {
    pub actor: ActorId,
    pub n: f64,
    pub t_ref: Tick,
    /// The rate (legs a day) last pushed to the source, and when.
    pub pushed: f64,
    pub pushed_at: Option<Tick>,
    /// Bumped each time a wake is booked: a wake with another stamp is stale.
    pub stamp: u32,
    /// The latest pass's point and y: the source's position while the meter has no line.
    pub last: Option<(Pos, i32)>,
    /// By station pair, sorted by (a, b).
    pub lines: Vec<LineCount>,
}

impl RailMeter {
    pub fn new(actor: ActorId) -> Self {
        RailMeter {
            actor,
            n: 0.0,
            t_ref: Tick::ZERO,
            pushed: 0.0,
            pushed_at: None,
            stamp: 0,
            last: None,
            lines: Vec::new(),
        }
    }

    pub fn n_at(&self, t: Tick) -> f64 {
        decayed(self.n, self.t_ref, t)
    }

    /// Counts a leg whose pass ended at `t`.
    pub fn add(&mut self, t: Tick) {
        add_leg(&mut self.n, &mut self.t_ref, t);
    }

    pub fn per_day(&self, t: Tick) -> f64 {
        legs_per_day(self.n_at(t))
    }

    /// The first tick at which the rate is at or below `r` legs a day (`t_ref` when it already is,
    /// the end of time for an `r` of 0 or less, which a count never reaches).
    pub fn falls_to(&self, r: f64) -> Tick {
        let n_lo = r * RAIL_TAU / TICKS_PER_DAY as f64;
        if self.n <= n_lo {
            return self.t_ref;
        }
        if n_lo <= 0.0 {
            return BLOCKED;
        }
        later(self.t_ref, (RAIL_TAU * (self.n / n_lo).ln()).ceil() as u64)
    }

    /// Credits one leg ending at `t` to the line between stations `a` and `b` (either order): the
    /// line is made if it is new (the smallest-counted line makes room when there are
    /// [`MAX_LINES`]), and the point, y and nearer station are the pass's unless a later pass has
    /// already set them (MZB-334).
    pub fn credit_line(
        &mut self,
        (a, b): (u64, u64),
        t: Tick,
        point: Pos,
        y: i32,
        near: Option<(Pos, DistrictId)>,
    ) {
        let key = (a.min(b), a.max(b));
        match self.lines.binary_search_by(|l| (l.a, l.b).cmp(&key)) {
            Ok(i) => {
                let l = &mut self.lines[i];
                if t >= l.t_ref {
                    l.point = point;
                    l.y = y;
                    l.near = near;
                }
                l.add(t);
            }
            Err(_) => {
                if self.lines.len() >= MAX_LINES {
                    // The smallest count at `t` (the lower pair on a tie) makes room.
                    let (worst, _) = self
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(j, l)| (j, l.n_at(t)))
                        .min_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)))
                        .expect("lines");
                    self.lines.remove(worst);
                }
                let at = self
                    .lines
                    .binary_search_by(|l| (l.a, l.b).cmp(&key))
                    .unwrap_err();
                self.lines.insert(
                    at,
                    LineCount {
                        a: key.0,
                        b: key.1,
                        n: 1.0,
                        t_ref: t,
                        point,
                        y,
                        near,
                    },
                );
            }
        }
    }

    /// The line with the largest count at `t` (the lower pair on a tie), if any.
    pub fn busiest(&self, t: Tick) -> Option<&LineCount> {
        self.lines
            .iter()
            .map(|l| (l, l.n_at(t)))
            .max_by(|x, y| {
                x.1.total_cmp(&y.1)
                    .then((y.0.a, y.0.b).cmp(&(x.0.a, x.0.b)))
            })
            .map(|(l, _)| l)
    }
}

impl Conflict {
    /// A rail meter's push (MZB-313): faction `f`'s one source for `actor`, `per_day` legs a day at
    /// `pos` (a point on the track) with `pin`. It reaches only `f` (falloff 1), has no district,
    /// chunks or places, and counts against `actor` whoever stands where: so `reattribute` never
    /// moves it and a camp founded later never picks it up. A rate of 0 removes the source and
    /// `f`'s reach of it. The feud's rate is then recomputed (tolerance once from this one rate,
    /// as for any kind).
    #[allow(clippy::too_many_arguments)]
    pub fn set_emission_for(
        &mut self,
        w: &mut World,
        f: u32,
        actor: ActorId,
        per_day: f64,
        pos: Pos,
        pin: Pin,
        at: Tick,
    ) {
        let _ = w;
        self.run_to(at);
        let key = (
            rail_key(self.factions[f as usize].key, actor),
            SignalKind::Rail,
        );
        let rate = per_day.max(0.0) / TICKS_PER_DAY as f64;
        if rate > 0.0 {
            let id = self.sources.insert(
                key,
                Source {
                    pos,
                    district: None,
                    rate,
                    affects: vec![(f, 1.0)],
                    actor,
                    chunks: Vec::new(),
                    pin: Some(Box::new(pin)),
                },
            );
            self.factions.row_mut(f).add_reach(SignalKind::Rail, id);
        } else if let Some((id, _)) = self.sources.remove(&key) {
            self.factions.row_mut(f).remove_reach(SignalKind::Rail, id);
        }
        self.refresh_rates(f, actor, SignalKind::Rail, at);
    }
}
