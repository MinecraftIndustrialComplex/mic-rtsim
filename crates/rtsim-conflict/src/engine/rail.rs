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
/// How far from the track point a band bound for a track stands, toward its den (PLACEHOLDER,
/// MOWZIE.md §4.4, MZB-316).
pub const SPOT_BLOCKS: f64 = 5.0;
/// Lines kept per meter; a new one beyond this evicts the one with the smallest count
/// (PLACEHOLDER, MZB-307).
pub const MAX_LINES: usize = 16;
/// Passes a train holds before its oldest is let go (PLACEHOLDER, MZB-308).
pub const HELD_PER_TRAIN: usize = 64;
/// Passes held in all before the oldest is let go (PLACEHOLDER, MZB-308).
pub const HELD_TOTAL: usize = 4_096;
/// How long a pass waits for its train's next arrival: a day (PLACEHOLDER, ledger 1).
pub const HELD_TICKS: u64 = TICKS_PER_DAY;
/// A push is made when the rate has moved this much short of the band's edge: the band's edge is
/// reached by a wake booked at the first tick the closed form crosses it, and a float a hair on
/// the near side would otherwise book the same wake again a tick later (MZB-337).
const BAND_EPS: f64 = 1e-9;

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
            // The track ends with its source: the feud's focus keeps its position but not the
            // track, so a kill in the wild (a lower class) can take the focus (MZB-314, MZB-348).
            if self
                .feud(f, actor)
                .is_some_and(|x| x.focus.as_ref().is_some_and(|fo| fo.track.is_some()))
                && let Some(fo) = self.feud_mut(f, actor).and_then(|x| x.focus.as_mut())
            {
                fo.track = None;
            }
        }
        self.refresh_rates(f, actor, SignalKind::Rail, at);
    }
}

impl Conflict {
    /// Where a band bound for the track point `pos` stands: [`SPOT_BLOCKS`] from it toward `den`,
    /// in whole blocks and canonical (MZB-316). The direction is read in the track's chart, so a
    /// den across a seam is the short way round. A den within a block of the point, or in another
    /// frame, puts the spot toward +x.
    pub fn spot_toward(&self, pos: Pos, den: Pos) -> Pos {
        let t = self.topo(pos.frame);
        let from = (i64::from(pos.x), i64::from(pos.z));
        let (dx, dz) = if den.frame == pos.frame {
            let to = t.toward(from, (i64::from(den.x), i64::from(den.z)));
            ((to.0 - from.0) as f64, (to.1 - from.1) as f64)
        } else {
            (0.0, 0.0)
        };
        let d = dx.hypot(dz);
        let (ox, oz) = if d <= 1.0 {
            (SPOT_BLOCKS, 0.0)
        } else {
            (dx / d * SPOT_BLOCKS, dz / d * SPOT_BLOCKS)
        };
        let (x, z) = t.canonical_i32(
            (f64::from(pos.x) + ox).round() as i32,
            (f64::from(pos.z) + oz).round() as i32,
        );
        Pos::new(pos.frame, x, z)
    }
}

/// A line a pass is counted on: its two stations' ids (either order) and the one nearer the
/// pass that has a district.
type LineAt = ((u64, u64), Option<(Pos, DistrictId)>);

/// A pass waiting for its train's next arrival to name the line it ran (MZB-308).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub faction: u32,
    /// Whom the pass counts against: none for a train nobody owns, counted when its line is named
    /// or when it is let go (MZB-309, MZB-310).
    pub actor: Option<ActorId>,
    pub exit: Tick,
    pub point: Pos,
    pub y: i32,
}

/// One end of a line: a station of the train's last departure and its arrival (MZB-311).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StationEnd {
    pub id: u64,
    pub dim: u32,
    pub x: i32,
    pub z: i32,
    pub district: Option<u64>,
}

/// A train has arrived (MZB-311): `line` is the stations it ran between, `None` when it is not
/// known (an unwatched stop, an arrival with no departure booked) or is a self-edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailCall {
    pub train: u64,
    pub line: Option<(StationEnd, StationEnd)>,
}

impl Conflict {
    /// Whether any pass of `train` is waiting for a line.
    pub fn holds_train(&self, train: u64) -> bool {
        self.held.contains_key(&train)
    }

    /// Passes waiting for a line, in all.
    pub fn held_passes(&self) -> usize {
        self.held_n
    }

    /// A RAIL_PASS (MZB-319): a train's pass through a faction's range. A pass for a faction
    /// unknown or minding no rail is dropped; one with an owner is counted at once, and every
    /// pass is held for its train's next arrival to name the line it ran.
    pub fn rail_pass(&mut self, w: &mut World, rp: &boundary::RailPass, at: Tick) {
        self.let_go_due(w, at);
        let Some(f) = self.faction_of_key(rp.faction) else {
            self.stats.rail_unknown += 1;
            return;
        };
        if self.culture(f).signals[SignalKind::Rail.index()].weight <= 0.0 {
            self.stats.rail_unknown += 1;
            return;
        }
        self.stats.rail_passes += 1;
        let point = self.canon(
            rp.dimension,
            rp.pos.0.floor() as i32,
            rp.pos.2.floor() as i32,
        );
        let y = rp.pos.1.floor() as i32;
        let exit = Tick(rp.exit.max(0) as u64).min(at);
        let actor = (rp.owner != (0, 0)).then(|| self.player(w, rp.owner));
        if let Some(a) = actor {
            self.count_pass(f, a, exit, point, y, None);
            self.rail_gate(w, f, a, at);
        }
        self.hold(
            w,
            rp.train,
            Held {
                faction: f,
                actor,
                exit,
                point,
                y,
            },
            at,
        );
    }

    /// A train arrived (MZB-311): with a line, each of its held passes is credited to it; with
    /// none, they are let go now.
    pub fn rail_call(&mut self, w: &mut World, call: &RailCall, at: Tick) {
        self.let_go_due(w, at);
        self.stats.rail_calls += 1;
        let Some(passes) = self.take_train(call.train) else {
            return;
        };
        let Some((a, b)) = call.line.filter(|(a, b)| a.id != b.id) else {
            for h in passes {
                self.stats.rail_let_go += 1;
                self.let_go(w, h, at);
            }
            return;
        };
        let (pa, pb) = (self.canon(a.dim, a.x, a.z), self.canon(b.dim, b.x, b.z));
        let mut chunks = vec![chunk_of(pa), chunk_of(pb)];
        chunks.sort_unstable();
        chunks.dedup();
        let mut touched: BTreeSet<(ActorId, u32)> = BTreeSet::new();
        for h in passes {
            let owned = h.actor.is_some();
            let Some(actor) = h.actor.or_else(|| self.presence.top_of(&chunks, at)) else {
                // Nobody stood by either station: counted against nobody (MZB-310).
                continue;
            };
            let near = self.near_station(h.point, (pa, &a), (pb, &b));
            let line = Some(((a.id, b.id), near));
            if owned {
                // Counted at ingest; a meter retired since has no line to credit.
                self.credit_only(h.faction, actor, h.exit, h.point, h.y, line);
            } else {
                self.count_pass(h.faction, actor, h.exit, h.point, h.y, line);
            }
            touched.insert((actor, h.faction));
        }
        for (actor, f) in touched {
            self.rail_gate(w, f, actor, at);
        }
    }

    /// A meter's wake fired (MZB-306).
    pub(super) fn rail_wake(
        &mut self,
        w: &mut World,
        f: u32,
        actor: ActorId,
        stamp: u32,
        at: Tick,
    ) {
        self.let_go_due(w, at);
        if self
            .factions
            .row(f)
            .rail_meter(actor)
            .is_none_or(|m| m.stamp != stamp)
        {
            self.stats.stale += 1;
            self.stats.rail_stale += 1;
            return;
        }
        self.stats.rail_wakes += 1;
        self.rail_gate(w, f, actor, at);
    }

    /// The push gate (MZB-305) and the next wake (MZB-306) of `actor`'s meter in `f`, after a
    /// count or at a wake.
    pub(super) fn rail_gate(&mut self, w: &mut World, f: u32, actor: ActorId, at: Tick) {
        let tol = self.culture(f).signals[SignalKind::Rail.index()].tolerance_per_day;
        let Ok(i) = self.factions.row(f).rail_at(actor) else {
            return;
        };
        let (r, p, day_due) = {
            let m = &self.factions.row(f).rail()[i];
            (
                m.per_day(at),
                m.pushed,
                m.pushed_at.is_none_or(|t| at.0 >= t.0 + TICKS_PER_DAY),
            )
        };
        let quiet = r < tol && p < tol;
        if quiet && r <= RETIRE_PER_DAY {
            self.retire_meter(w, f, actor, at);
            return;
        }
        let mut p = p;
        if !quiet && ((r - p).abs() >= (0.1 * p).max(1.0) - BAND_EPS || day_due) {
            let (pos, pin) = self.rail_focus(f, actor, at);
            self.set_emission_for(w, f, actor, r, pos, pin, at);
            self.stats.rail_pushes += 1;
            let m = self
                .factions
                .row_mut(f)
                .rail_meter_mut(actor)
                .expect("the meter just read");
            m.pushed = r;
            m.pushed_at = Some(at);
            p = r;
            if r < tol && r <= RETIRE_PER_DAY {
                self.retire_meter(w, f, actor, at);
                return;
            }
        }
        // The next wake: retirement for a quiet meter, else the earlier of the band's lower edge
        // and a day after the push.
        let m = self
            .factions
            .row_mut(f)
            .rail_meter_mut(actor)
            .expect("the meter just read");
        let wake = if r < tol && p < tol {
            m.falls_to(RETIRE_PER_DAY)
        } else {
            let lo = p - (0.1 * p).max(1.0);
            let by_day = Tick(m.pushed_at.map_or(at.0, |t| t.0) + TICKS_PER_DAY);
            if lo > 0.0 {
                m.falls_to(lo).min(by_day)
            } else {
                by_day
            }
        };
        m.stamp += 1;
        let stamp = m.stamp;
        let wake = wake.max(Tick(at.0 + 1));
        self.schedule(
            wake,
            Ev::Rail {
                faction: f,
                actor,
                stamp,
            },
        );
    }

    /// Where `actor`'s meter in `f` pushes its source: the busiest line's point and pin, else the
    /// latest pass's with no station (MZB-307).
    fn rail_focus(&self, f: u32, actor: ActorId, at: Tick) -> (Pos, Pin) {
        let m = self
            .factions
            .row(f)
            .rail_meter(actor)
            .expect("the meter just read");
        if let Some(l) = m.busiest(at) {
            return (
                l.point,
                Pin {
                    y: l.y,
                    station: l.near,
                },
            );
        }
        match m.last {
            Some((pos, y)) => (pos, Pin { y, station: None }),
            None => (
                self.factions[f as usize].pos,
                Pin {
                    y: 0,
                    station: None,
                },
            ),
        }
    }

    /// A quiet meter that has fallen to nothing: its source is pushed to 0 (if it ever had one)
    /// and the meter goes with its lines (MZB-306).
    fn retire_meter(&mut self, w: &mut World, f: u32, actor: ActorId, at: Tick) {
        let Ok(i) = self.factions.row(f).rail_at(actor) else {
            return;
        };
        if self.factions.row(f).rail()[i].pushed > 0.0 {
            let pos = self.factions[f as usize].pos;
            self.set_emission_for(
                w,
                f,
                actor,
                0.0,
                pos,
                Pin {
                    y: 0,
                    station: None,
                },
                at,
            );
        }
        // A wake already booked for it finds no meter and counts stale.
        let row = self.factions.row_mut(f);
        if let Ok(i) = row.rail_at(actor) {
            row.rail_remove(i);
        }
    }

    /// One leg ending at `exit` counted for `actor` in `f` (the meter made if it is new), with
    /// the line it ran when that is known. The meter's latest point is the pass's if the pass is
    /// the latest.
    fn count_pass(
        &mut self,
        f: u32,
        actor: ActorId,
        exit: Tick,
        point: Pos,
        y: i32,
        line: Option<LineAt>,
    ) {
        let m = self.factions.row_mut(f).rail_entry(actor);
        if m.last.is_none() || exit >= m.t_ref {
            m.last = Some((point, y));
        }
        m.add(exit);
        if let Some((ids, near)) = line {
            m.credit_line(ids, exit, point, y, near);
        }
    }

    /// A pass counted at ingest, now credited to the line it ran.
    fn credit_only(
        &mut self,
        f: u32,
        actor: ActorId,
        exit: Tick,
        point: Pos,
        y: i32,
        line: Option<LineAt>,
    ) {
        if let (Some(m), Some((ids, near))) = (self.factions.row_mut(f).rail_meter_mut(actor), line)
        {
            m.credit_line(ids, exit, point, y, near);
        }
    }

    /// Whichever of the line's two stations is nearer `point` and has a district, in the point's
    /// frame (the lower id on a tie). None when neither qualifies.
    fn near_station(
        &self,
        point: Pos,
        a: (Pos, &StationEnd),
        b: (Pos, &StationEnd),
    ) -> Option<(Pos, DistrictId)> {
        let t = self.topo(point.frame);
        let mut best: Option<(f64, Pos, DistrictId)> = None;
        let mut ends = [a, b];
        ends.sort_by_key(|e| e.1.id);
        for (pos, st) in ends {
            let (Some(d), true) = (st.district, pos.frame == point.frame) else {
                continue;
            };
            let dd = tdist(t, pxz(point), pxz(pos));
            if best.is_none_or(|e| dd < e.0) {
                best = Some((dd, pos, DistrictId(d)));
            }
        }
        best.map(|(_, pos, d)| (pos, d))
    }

    // ---- the held passes --------------------------------------------------------------------

    /// Holds a pass for its train, the train's passes in order of exit. A train over its cap, or
    /// the table over its own, lets its oldest go first (MZB-308).
    fn hold(&mut self, w: &mut World, train: u64, h: Held, at: Tick) {
        if self
            .held
            .get(&train)
            .is_some_and(|v| v.len() >= HELD_PER_TRAIN)
            && let Some(old) = self.pop_held(train)
        {
            self.stats.rail_let_go += 1;
            self.let_go(w, old, at);
        }
        if self.held_n >= HELD_TOTAL
            && let Some(&(_, oldest)) = self.held_oldest.first()
            && let Some(old) = self.pop_held(oldest)
        {
            self.stats.rail_let_go += 1;
            self.let_go(w, old, at);
        }
        let v = self.held.entry(train).or_default();
        if let Some(first) = v.first() {
            self.held_oldest.remove(&(first.exit, train));
        }
        let at_i = v.partition_point(|x| x.exit <= h.exit);
        v.insert(at_i, h);
        self.held_oldest.insert((v[0].exit, train));
        self.held_n += 1;
    }

    /// The oldest pass `train` holds, taken out.
    fn pop_held(&mut self, train: u64) -> Option<Held> {
        let v = self.held.get_mut(&train)?;
        let h = v.remove(0);
        self.held_oldest.remove(&(h.exit, train));
        match v.first() {
            Some(n) => {
                self.held_oldest.insert((n.exit, train));
            }
            None => {
                self.held.remove(&train);
            }
        }
        self.held_n -= 1;
        Some(h)
    }

    /// Every pass `train` holds, taken out.
    fn take_train(&mut self, train: u64) -> Option<Vec<Held>> {
        let v = self.held.remove(&train)?;
        self.held_oldest.remove(&(v[0].exit, train));
        self.held_n -= v.len();
        Some(v)
    }

    /// A pass nobody named a line for (MZB-309): one with an owner was counted at ingest, so it is
    /// only dropped; one with none counts now, at its exit, against the player with the most time
    /// in its point's chunk, or against nobody.
    fn let_go(&mut self, w: &mut World, h: Held, at: Tick) {
        if h.actor.is_some() {
            return;
        }
        if let Some(a) = self.presence.top_of(&[chunk_of(h.point)], at) {
            self.count_pass(h.faction, a, h.exit, h.point, h.y, None);
            self.rail_gate(w, h.faction, a, at);
        }
    }

    /// Lets go every pass held for a day (MZB-308). Lazy: it runs first in `rail_pass`,
    /// `rail_call` and `rail_wake`.
    pub(super) fn let_go_due(&mut self, w: &mut World, at: Tick) {
        while let Some(&(exit, train)) = self.held_oldest.first()
            && exit.0 + HELD_TICKS <= at.0
        {
            let Some(h) = self.pop_held(train) else {
                break;
            };
            self.stats.rail_let_go += 1;
            self.let_go(w, h, at);
        }
    }
}
