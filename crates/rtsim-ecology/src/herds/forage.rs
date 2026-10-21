// SPDX-License-Identifier: AGPL-3.0-or-later
//! The forage field: what herds eat, on a coarse grid (SCALE.md §2's field level, §4's closed
//! forms).
//!
//! **Dynamics.** In a cell, forage F follows F' = g·F − h·F² within a sub-season, with
//! g = growth − decay and h = growth / capacity (logistic growth with decay). In u = 1/F that is
//! linear, u' = −g·u + h, so a sub-season of length τ is an affine map u ↦ a·u + b with
//! a = e^(−gτ) and b = h(1 − a)/g. Affine maps compose into affine maps, so:
//! - a whole year is one affine map (twelve compositions), and n years are its n-th power in
//!   closed form (a^n, b(1 − a^n)/(1 − a));
//! - catching a cell up over any elapsed time costs at most two partial years of sub-seasons plus
//!   one power: flat in elapsed time (SCALE.md §4, §10).
//!
//! **Lazy cells.** A cell no herd has grazed has no row: it sits on the climate's periodic steady
//! state (the annual map's fixed point carried to the time of year). Grazing writes a row anchored
//! at that moment; reads evolve it to now. The yearly sweep drops rows that are back within 1% of
//! the steady state, so memory follows recent grazing, not the map (SCALE.md §6).
//!
//! **Floor.** Grazing never takes a cell below a floor share of its best capacity (roots and seed
//! bank), so a grazed cell always regrows.

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::codec::{self, Decode, Encode, Reader, Writer};
use rtsim_kernel::rows::{PagedRows, cell_key, cell_of_key};

use super::climate::{Climate, ForageRates, SUB_SEASONS, Subs};

/// Forage cell edge, blocks. PLACEHOLDER 128 (a quarter region; SCALE.md §2 sketches 64 for
/// density fields; herds cover hundreds of blocks, so coarser cells keep a decision's work small).
pub const CELL_BLOCKS: i32 = 128;

/// Grazing never leaves less than this share of a cell's best capacity. PLACEHOLDER 3%.
pub const FLOOR_SHARE: f64 = 0.03;

/// A row drops back to the steady state when within this share of it at the yearly sweep.
/// PLACEHOLDER 1%.
pub const CONVERGED_SHARE: f64 = 0.01;

/// The cell holding a block position.
pub fn cell_of(x: i64, z: i64) -> (i32, i32) {
    (
        x.div_euclid(i64::from(CELL_BLOCKS)) as i32,
        z.div_euclid(i64::from(CELL_BLOCKS)) as i32,
    )
}

/// A cell's centre in blocks.
pub fn centre_of(c: (i32, i32)) -> (i32, i32) {
    (
        c.0 * CELL_BLOCKS + CELL_BLOCKS / 2,
        c.1 * CELL_BLOCKS + CELL_BLOCKS / 2,
    )
}

/// u ↦ a·u + b, u = 1/F.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
}

impl Affine {
    pub const ID: Affine = Affine { a: 1.0, b: 0.0 };

    /// `days` of the dynamics `r`.
    pub fn piece(r: ForageRates, days: f64) -> Affine {
        if days <= 0.0 {
            return Affine::ID;
        }
        let h = if r.capacity > 0.0 && r.growth_per_day > 0.0 {
            r.growth_per_day / r.capacity
        } else {
            0.0
        };
        let g = r.growth_per_day - r.decay_per_day;
        if g.abs() < 1e-12 {
            return Affine {
                a: 1.0,
                b: h * days,
            };
        }
        let a = (-g * days).exp();
        Affine {
            a,
            b: h * (-(-g * days).exp_m1()) / g,
        }
    }

    /// `self`, then `next`.
    pub fn then(self, next: Affine) -> Affine {
        Affine {
            a: next.a * self.a,
            b: next.a * self.b + next.b,
        }
    }

    /// `self` applied `n` times, in closed form.
    pub fn pow(self, n: u64) -> Affine {
        if n == 0 {
            return Affine::ID;
        }
        let an = self.a.powf(n as f64);
        let b = if (1.0 - self.a).abs() < 1e-12 {
            self.b * n as f64
        } else {
            self.b * (1.0 - an) / (1.0 - self.a)
        };
        Affine { a: an, b }
    }

    pub fn apply(self, u: f64) -> f64 {
        self.a * u + self.b
    }
}

/// A grazed cell: its forage at its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    pub anchor: Tick,
    pub forage: f64,
}

impl Encode for Row {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.anchor.0);
        w.f64(self.forage);
    }
}

impl Decode for Row {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        Ok(Row {
            anchor: Tick(r.u64()?),
            forage: r.f64()?,
        })
    }
}

impl rtsim_kernel::rows::Row for Row {}

/// Cell key: (frame, cell x, cell z).
pub type CellKey = (u16, i32, i32);

/// The page table of the grazed cells (docs/choices-paging-modules.md).
pub const TABLE_FORAGE: u16 = 0x4811;
/// Cells per page: 8 x 8 cells of [`CELL_BLOCKS`], 1,024 blocks square. PLACEHOLDER.
pub const FORAGE_PER_PAGE: u64 = 64;

/// A cell's row key: neighbouring cells share pages.
pub fn row_key(k: CellKey) -> u64 {
    cell_key(k.0, i64::from(k.1), i64::from(k.2))
}

/// The cell of a row key.
pub fn cell_of_row(key: u64) -> CellKey {
    let (f, x, z) = cell_of_key(key);
    (f, x as i32, z as i32)
}

/// The field: rows for grazed cells only, in pages (A16).
#[derive(Debug)]
pub struct Forage {
    pub rows: PagedRows<Row>,
    pub stats: ForageStats,
}

impl Default for Forage {
    fn default() -> Self {
        Forage {
            rows: PagedRows::new(TABLE_FORAGE, FORAGE_PER_PAGE),
            stats: ForageStats::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForageStats {
    /// Cells read (each costs O(sub-seasons a year), never O(elapsed)).
    pub reads: u64,
    pub grazed: u64,
    pub folded: u64,
}

fn u_of(f: f64, floor: f64) -> f64 {
    1.0 / f.max(floor).max(1e-9)
}

impl Forage {
    pub fn new() -> Self {
        Forage::default()
    }

    /// The map over `[t0, t1)` for one cell: sub-season pieces to the next year boundary, whole
    /// years by power, then pieces to `t1`.
    fn map(climate: &dyn Climate, subs: &Subs, k: CellKey, t0: Tick, t1: Tick) -> Affine {
        if t1 <= t0 {
            return Affine::ID;
        }
        let rates = |n: i64| climate.forage(k.0, k.1, k.2, Subs::in_year(n));
        let day = TICKS_PER_DAY as f64;
        let mut m = Affine::ID;
        let mut t = t0;
        let mut n = subs.index(t);
        // Pieces until a year boundary or t1.
        while t < t1 {
            if Subs::in_year(n) == 0 && subs.start(n) == t && t1.0 - t.0 >= subs.year_ticks() {
                break;
            }
            let end = subs.start(n + 1).min(t1);
            m = m.then(Affine::piece(rates(n), (end.0 - t.0) as f64 / day));
            t = end;
            n += 1;
        }
        if t >= t1 {
            return m;
        }
        // Whole years.
        let years = (t1.0 - t.0) / subs.year_ticks();
        if years > 0 {
            let mut y = Affine::ID;
            for s in 0..i64::from(SUB_SEASONS) {
                y = y.then(Affine::piece(rates(n + s), subs.len as f64 / day));
            }
            m = m.then(y.pow(years));
            t += years * subs.year_ticks();
            n += years as i64 * i64::from(SUB_SEASONS);
        }
        while t < t1 {
            let end = subs.start(n + 1).min(t1);
            m = m.then(Affine::piece(rates(n), (end.0 - t.0) as f64 / day));
            t = end;
            n += 1;
        }
        m
    }

    /// The best capacity a cell has in any sub-season: the floor's base.
    pub fn best_capacity(climate: &dyn Climate, k: CellKey) -> f64 {
        (0..SUB_SEASONS)
            .map(|s| climate.forage(k.0, k.1, k.2, s).capacity)
            .fold(0.0, f64::max)
    }

    fn floor(climate: &dyn Climate, k: CellKey) -> f64 {
        Self::best_capacity(climate, k) * FLOOR_SHARE
    }

    /// The climate's periodic steady state at `t`: the annual map's fixed point at the start of
    /// `t`'s year, carried to `t`.
    pub fn steady(climate: &dyn Climate, subs: &Subs, k: CellKey, t: Tick) -> f64 {
        let n = subs.index(t);
        let y0 = Subs::year(n) * i64::from(SUB_SEASONS);
        let start = subs.start(y0);
        let day = TICKS_PER_DAY as f64;
        let mut y = Affine::ID;
        for s in 0..i64::from(SUB_SEASONS) {
            y = y.then(Affine::piece(
                climate.forage(k.0, k.1, k.2, Subs::in_year(y0 + s)),
                subs.len as f64 / day,
            ));
        }
        let floor = Self::floor(climate, k);
        let u0 = if y.a < 1.0 - 1e-12 {
            y.b / (1.0 - y.a)
        } else {
            u_of(floor, floor)
        };
        let u = Self::map(climate, subs, k, start, t.max(start)).apply(u0);
        if u.is_finite() && u > 0.0 {
            (1.0 / u).max(floor)
        } else {
            floor
        }
    }

    /// Forage in a cell at `t` (forage units). Pure: O(sub-seasons a year) whatever the time since
    /// the cell was grazed.
    pub fn at(&self, climate: &dyn Climate, subs: &Subs, k: CellKey, t: Tick) -> f64 {
        match self.rows.get(row_key(k)) {
            Some(r) if r.anchor <= t => {
                let floor = Self::floor(climate, k);
                let u = Self::map(climate, subs, k, r.anchor, t).apply(u_of(r.forage, floor));
                if u.is_finite() && u > 0.0 {
                    (1.0 / u).max(floor)
                } else {
                    floor
                }
            }
            Some(r) => r.forage,
            None => Self::steady(climate, subs, k, t),
        }
    }

    /// Takes up to `want` from a cell at `t` (never below the floor); returns what was taken. The
    /// cell is re-anchored at `t`.
    pub fn graze(
        &mut self,
        climate: &dyn Climate,
        subs: &Subs,
        k: CellKey,
        t: Tick,
        want: f64,
    ) -> f64 {
        self.stats.grazed += 1;
        let now = self.at(climate, subs, k, t);
        let floor = Self::floor(climate, k);
        let take = want.clamp(0.0, (now - floor).max(0.0));
        self.rows.insert(
            row_key(k),
            Row {
                anchor: t,
                forage: now - take,
            },
        );
        take
    }

    /// Drops rows back within [`CONVERGED_SHARE`] of the steady state at `t` (the yearly sweep:
    /// a deterministic moment, so the state never depends on when anything was read).
    ///
    /// A whole-field pass once a year: it loads every grazed cell's page (paged, the budget is
    /// kept page by page).
    pub fn sweep(&mut self, climate: &dyn Climate, subs: &Subs, t: Tick) {
        let mut gone = Vec::new();
        let rows = &mut self.rows;
        let pass = rows.for_each_bounded(|key, _| gone.push(key));
        pass.unwrap_or_else(|e| panic!("forage sweep: page load failed: {e}"));
        let keys = std::mem::take(&mut gone);
        for key in keys {
            let k = cell_of_row(key);
            let f = self.at(climate, subs, k, t);
            let s = Self::steady(climate, subs, k, t);
            if (f - s).abs() <= CONVERGED_SHARE * s.max(1e-9) {
                self.rows.remove(key);
                self.stats.folded += 1;
            }
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.rows.heap_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::Calendar;
    use proptest::prelude::*;

    /// Seasonal grass: grows in spring and summer, rests in autumn, decays in winter.
    struct Seasonal;
    impl Climate for Seasonal {
        fn forage(&self, _: u16, cx: i32, _: i32, sub: u32) -> ForageRates {
            let cap = 1_000.0 + 100.0 * f64::from(cx.rem_euclid(5));
            match sub {
                0..=5 => ForageRates {
                    growth_per_day: 0.15,
                    capacity: cap,
                    decay_per_day: 0.0,
                },
                6..=8 => ForageRates {
                    growth_per_day: 0.02,
                    capacity: cap,
                    decay_per_day: 0.01,
                },
                _ => ForageRates {
                    growth_per_day: 0.0,
                    capacity: cap,
                    decay_per_day: 0.03,
                },
            }
        }
        fn temperature(&self, _: u16, _: i32, _: i32, _: u32) -> f32 {
            10.0
        }
        fn terrain(&self, _: u16, _: i32, _: i32) -> f32 {
            1.0
        }
    }

    /// Steps the logistic ODE numerically, for comparison.
    fn euler(c: &dyn Climate, subs: &Subs, k: CellKey, f0: f64, t0: Tick, t1: Tick) -> f64 {
        let mut f = f0;
        let step = 50u64;
        let mut t = t0.0;
        while t < t1.0 {
            let dt = step.min(t1.0 - t);
            let r = c.forage(k.0, k.1, k.2, Subs::in_year(subs.index(Tick(t))));
            let days = dt as f64 / TICKS_PER_DAY as f64;
            let h = if r.capacity > 0.0 && r.growth_per_day > 0.0 {
                r.growth_per_day / r.capacity
            } else {
                0.0
            };
            // RK2 on F' = gF - hF^2.
            let g = r.growth_per_day - r.decay_per_day;
            let d = |f: f64| g * f - h * f * f;
            let mid = f + 0.5 * days * d(f);
            f += days * d(mid);
            t += dt;
        }
        f
    }

    #[test]
    fn closed_form_matches_the_ode() {
        let subs = Subs::of(&Calendar::default());
        let c = Seasonal;
        let k = (0, 2, 0);
        for (f0, d0, d1) in [(50.0, 0u64, 30u64), (800.0, 5, 200), (10.0, 40, 41)] {
            let (t0, t1) = (Tick::from_days(d0), Tick::from_days(d1));
            let closed = Forage::map(&c, &subs, k, t0, t1).apply(1.0 / f0);
            let num = euler(&c, &subs, k, f0, t0, t1);
            assert!(
                ((1.0 / closed) - num).abs() / num < 1e-3,
                "{f0} {d0}..{d1}: {} vs {num}",
                1.0 / closed
            );
        }
    }

    #[test]
    fn steady_state_is_periodic_and_fixed() {
        let subs = Subs::of(&Calendar::default());
        let c = Seasonal;
        let k = (0, 1, 3);
        for d in [0u64, 13, 50, 95] {
            let a = Forage::steady(&c, &subs, k, Tick::from_days(d));
            let b = Forage::steady(&c, &subs, k, Tick::from_days(d + 96 * 7));
            assert!((a - b).abs() < 1e-6 * a, "{a} vs {b}");
        }
        // A row at the steady state stays on it.
        let mut f = Forage::new();
        let t0 = Tick::from_days(10);
        let s0 = Forage::steady(&c, &subs, k, t0);
        f.rows.insert(
            row_key(k),
            Row {
                anchor: t0,
                forage: s0,
            },
        );
        let later = Tick::from_days(500);
        let x = f.at(&c, &subs, k, later);
        let s = Forage::steady(&c, &subs, k, later);
        assert!((x - s).abs() < 1e-6 * s, "{x} vs {s}");
    }

    proptest! {
        /// Catching up in one read equals catching up in two (the composition rule), and grazing
        /// then regrowing converges back to the steady state.
        #[test]
        fn composes(f0 in 40.0f64..1200.0, a in 0u64..2000, b in 0u64..2000, c2 in 0u64..20_000) {
            let subs = Subs::of(&Calendar::default());
            let c = Seasonal;
            let k = (0, 4, -2);
            let (t0, t1, t2) = (Tick(a * 997), Tick(a * 997 + b * 1013), Tick(a * 997 + b * 1013 + c2 * 991));
            let whole = Forage::map(&c, &subs, k, t0, t2).apply(1.0 / f0);
            let mid = Forage::map(&c, &subs, k, t0, t1).apply(1.0 / f0);
            let two = Forage::map(&c, &subs, k, t1, t2).apply(mid);
            prop_assert!((whole - two).abs() <= 1e-9 * whole.abs().max(1e-9), "{} vs {}", whole, two);
        }
    }

    #[test]
    fn grazing_floors_and_recovers() {
        let subs = Subs::of(&Calendar::default());
        let c = Seasonal;
        let k = (0, 0, 0);
        let mut f = Forage::new();
        let t = Tick::from_days(20);
        let before = f.at(&c, &subs, k, t);
        let took = f.graze(&c, &subs, k, t, 1e9);
        assert!((before - took - 30.0).abs() < 1e-6, "floor is 3% of 1000");
        assert_eq!(f.rows.len(), 1);
        // Years later it is back on the steady state, and the sweep folds it.
        f.sweep(&c, &subs, Tick::from_days(20 + 96 * 20));
        assert!(f.rows.is_empty());
    }
}
