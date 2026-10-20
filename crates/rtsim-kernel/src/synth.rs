// SPDX-License-Identifier: AGPL-3.0-or-later
//! Synthetic kinds for the kernel's tests and the scale bench. Scaffolding, not content: they have
//! the shapes real modules will have, with made-up parameters, and get deleted once real modules
//! can stand in for them.
//!
//! - [`Herd`]: a group whose members are latent (counts per cohort, no rows), with random births,
//!   ageing and deaths committed once per day slot. Implements fold and sample against [`Sheep`].
//! - [`Sheep`]: an individual whose identity is derived from (herd seed, slot).
//! - [`Field`]: counts per grid cell; herds fold into it.
//! - [`Stock`]: a deterministic piecewise-linear stock with a threshold breakpoint.
//! - [`SynthRegion`]: a stock and a herd in one lazy region.
//!
//! Every rate below is a PLACEHOLDER chosen only to keep populations bounded in a bench run.

use crate::clock::{SlotPeriod, Tick};
use crate::codec::{self, Decode, Encode, Reader, Writer};
use crate::conserve::{Conserved, Diffuser, QuantityKind, Totals};
use crate::ladder::{Answers, Fold, Level, OnLadder, Query, Sample};
use crate::lazy::LazyModel;
use crate::rng::{SlotRng, philox4x64_10};

pub const SHEEP: QuantityKind = QuantityKind(1);
pub const GRAIN: QuantityKind = QuantityKind(2);

/// Per-day-slot probabilities. PLACEHOLDER values, not ecology.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HerdParams {
    /// Births per adult per day at zero crowding.
    pub birth: f64,
    /// Young becoming adult, per day.
    pub mature: f64,
    /// Adults becoming old, per day.
    pub senesce: f64,
    /// Deaths per day for young, adult, old.
    pub death: [f64; 3],
    /// Crowding: births scale by (1 - total / capacity).
    pub capacity: u64,
}

/// PLACEHOLDER parameters for tests and the bench.
pub const HERD_PARAMS: HerdParams = HerdParams {
    birth: 0.01,
    mature: 0.02,
    senesce: 0.005,
    death: [0.004, 0.001, 0.02],
    capacity: u64::MAX,
};

pub const YOUNG: u8 = 0;
pub const ADULT: u8 = 1;
pub const OLD: u8 = 2;

/// A herd at the group level: counts per cohort. Members are latent and cost no bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Herd {
    pub seed: u64,
    pub cohorts: [u64; 3],
    /// Births and deaths so far, so a test can check the head count is conserved.
    pub born: u64,
    pub died: u64,
    /// Crowding capacity for this herd.
    pub capacity: u64,
}

impl Herd {
    pub fn new(seed: u64, cohorts: [u64; 3]) -> Self {
        let total: u64 = cohorts.iter().sum();
        Self {
            seed,
            cohorts,
            born: 0,
            died: 0,
            capacity: total.saturating_mul(2).clamp(16, HERD_PARAMS.capacity),
        }
    }

    pub fn total(&self) -> u64 {
        self.cohorts.iter().sum()
    }

    /// One day slot: deaths, then ageing, then births, each one binomial draw per cohort, so the
    /// cost does not depend on the herd's size.
    pub fn commit_day(&mut self, rng: &mut SlotRng, p: &HerdParams) {
        let [mut y, mut a, mut o] = self.cohorts;
        let total = y + a + o;
        let room = if total >= self.capacity {
            0.0
        } else {
            1.0 - total as f64 / self.capacity as f64
        };
        let dy = rng.binomial(y, p.death[0]);
        let da = rng.binomial(a, p.death[1]);
        let d_o = rng.binomial(o, p.death[2]);
        y -= dy;
        a -= da;
        o -= d_o;
        let mature = rng.binomial(y, p.mature);
        let senesce = rng.binomial(a, p.senesce);
        let births = rng.binomial(a, (p.birth * room).clamp(0.0, 1.0));
        y = y - mature + births;
        a = a - senesce + mature;
        o += senesce;
        self.cohorts = [y, a, o];
        self.born += births;
        self.died += dy + da + d_o;
    }
}

impl OnLadder for Herd {
    fn level(&self) -> Level {
        Level::Group
    }
}

impl Conserved for Herd {
    fn add_totals(&self, out: &mut Totals) {
        out.add(SHEEP, i128::from(self.total()));
    }
}

/// An individual, derived from its herd's seed and its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Sheep {
    pub identity: u64,
    pub cohort: u8,
    /// Looks, from the identity: what a player would recognise when the same ewe comes back.
    pub fleece: u8,
}

impl OnLadder for Sheep {
    fn level(&self) -> Level {
        Level::Individual
    }
}

impl Conserved for Sheep {
    fn add_totals(&self, out: &mut Totals) {
        out.add(SHEEP, 1);
    }
}

impl Herd {
    /// The cohort and the index within it that `slot` names, or `None` past the population.
    fn locate(&self, slot: u64) -> Option<(u8, u64)> {
        let mut s = slot;
        for (c, &n) in self.cohorts.iter().enumerate() {
            if s < n {
                return Some((c as u8, s));
            }
            s -= n;
        }
        None
    }

    fn derive(&self, cohort: u8, within: u64) -> Sheep {
        let bits = philox4x64_10([within, u64::from(cohort), 0, 0], [self.seed, 0x5eed]);
        Sheep {
            identity: bits[0],
            cohort,
            fleece: (bits[1] % 16) as u8,
        }
    }
}

impl Sample<Sheep> for Herd {
    fn population(&self) -> u64 {
        self.total()
    }

    fn sample(&self, slot: u64) -> Option<Sheep> {
        let (c, within) = self.locate(slot)?;
        Some(self.derive(c, within))
    }

    /// Takes the member at `slot`. The members after it in its cohort shift down by one, which is
    /// "the state changed": slots name different sheep afterwards.
    fn take(&mut self, slot: u64) -> Option<Sheep> {
        let (c, within) = self.locate(slot)?;
        let sheep = self.derive(c, within);
        self.cohorts[c as usize] -= 1;
        Some(sheep)
    }
}

impl Fold<Sheep> for Herd {
    fn fold(&mut self, s: Sheep) {
        self.cohorts[s.cohort as usize] += 1;
    }
}

/// Counts per grid cell: the level below aggregates, above groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub cells: Vec<u64>,
}

impl Field {
    pub fn new(cells: usize) -> Self {
        Self {
            cells: vec![0; cells],
        }
    }

    pub fn cell_of(&self, herd: &Herd) -> usize {
        (herd.seed % self.cells.len() as u64) as usize
    }
}

impl OnLadder for Field {
    fn level(&self) -> Level {
        Level::Field
    }
}

impl Conserved for Field {
    fn add_totals(&self, out: &mut Totals) {
        out.add(SHEEP, self.cells.iter().map(|&c| i128::from(c)).sum());
    }
}

impl Fold<Herd> for Field {
    fn fold(&mut self, h: Herd) {
        let c = self.cell_of(&h);
        self.cells[c] += h.total();
    }
}

/// A deterministic stock: piecewise-linear production between breakpoints, with error diffusion.
/// Its breakpoint is the moment it reaches `threshold` ("grain surplus crosses the caravan
/// threshold at day X", SCALE.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stock {
    pub amount: i64,
    pub flow: Diffuser,
    pub threshold: i64,
    /// Produced so far, for the conservation ledger.
    pub produced: i64,
}

impl Stock {
    pub fn new(amount: i64, flow: Diffuser, threshold: i64) -> Self {
        Self {
            amount,
            flow,
            threshold,
            produced: 0,
        }
    }

    pub fn settle(&mut self, dt: u64) {
        let d = self.flow.emit(dt);
        self.amount += d;
        self.produced += d;
    }

    pub fn at(&self, dt: u64) -> i64 {
        self.amount + self.flow.peek(dt)
    }

    pub fn breakpoint(&self, anchor: Tick) -> Option<Tick> {
        if self.amount >= self.threshold {
            return None;
        }
        let dt = self.flow.ticks_until(self.threshold - self.amount)?;
        Some(anchor + dt.max(1))
    }
}

impl LazyModel for Stock {
    type View = i64;

    fn slot_period(&self) -> Option<SlotPeriod> {
        None
    }

    fn settle(&mut self, from: Tick, to: Tick) {
        Stock::settle(self, to - from);
    }

    fn commit_slot(&mut self, _slot: u64, _rng: &mut SlotRng) {}

    fn view(&self, anchor: Tick, at: Tick) -> i64 {
        self.at(at - anchor)
    }

    fn next_breakpoint(&self, anchor: Tick) -> Option<Tick> {
        self.breakpoint(anchor)
    }
}

/// A region with one deterministic process (a grain stock) and one random one (a herd).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SynthRegion {
    pub stock: Stock,
    pub herd: Herd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionView {
    pub grain: i64,
    pub herd: [u64; 3],
}

impl LazyModel for SynthRegion {
    type View = RegionView;

    fn slot_period(&self) -> Option<SlotPeriod> {
        Some(SlotPeriod::DAY)
    }

    fn settle(&mut self, from: Tick, to: Tick) {
        self.stock.settle(to - from);
    }

    fn commit_slot(&mut self, _slot: u64, rng: &mut SlotRng) {
        self.herd.commit_day(rng, &HERD_PARAMS);
    }

    fn view(&self, anchor: Tick, at: Tick) -> RegionView {
        RegionView {
            grain: self.stock.at(at - anchor),
            herd: self.herd.cohorts,
        }
    }

    fn next_breakpoint(&self, anchor: Tick) -> Option<Tick> {
        self.stock.breakpoint(anchor)
    }
}

impl Conserved for SynthRegion {
    fn add_totals(&self, out: &mut Totals) {
        self.herd.add_totals(out);
        out.add(GRAIN, i128::from(self.stock.amount));
    }
}

/// "How many sheep are here?"
pub struct HeadCount;
impl Query for HeadCount {
    type Answer = u64;
}

/// "How much grain is here?"
pub struct GrainHeld;
impl Query for GrainHeld {
    type Answer = i64;
}

impl Answers<HeadCount> for RegionView {
    fn answer(&self, _: &HeadCount) -> u64 {
        self.herd.iter().sum()
    }
}

impl Answers<GrainHeld> for RegionView {
    fn answer(&self, _: &GrainHeld) -> i64 {
        self.grain
    }
}

impl Answers<HeadCount> for Herd {
    fn answer(&self, _: &HeadCount) -> u64 {
        self.total()
    }
}

impl Answers<HeadCount> for Field {
    fn answer(&self, _: &HeadCount) -> u64 {
        self.cells.iter().sum()
    }
}

impl Encode for Stock {
    fn encode(&self, w: &mut Writer) {
        w.var_i64(self.amount);
        w.i64(self.flow.rate_fixed());
        w.u32(self.flow.carry());
        w.var_i64(self.threshold);
        w.var_i64(self.produced);
    }
}

impl Decode for Stock {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let amount = r.var_i64()?;
        let rate = r.i64()?;
        let carry = r.u32()?;
        Ok(Stock {
            amount,
            flow: Diffuser::with_carry(rate, carry),
            threshold: r.var_i64()?,
            produced: r.var_i64()?,
        })
    }
}

impl Encode for SynthRegion {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.herd.seed);
        for c in self.herd.cohorts {
            w.var_u64(c);
        }
        w.var_u64(self.herd.born);
        w.var_u64(self.herd.died);
        w.var_u64(self.herd.capacity);
        self.stock.encode(w);
    }
}

impl Decode for SynthRegion {
    fn decode(r: &mut Reader<'_>) -> codec::Result<Self> {
        let herd = Herd {
            seed: r.u64()?,
            cohorts: [r.var_u64()?, r.var_u64()?, r.var_u64()?],
            born: r.var_u64()?,
            died: r.var_u64()?,
            capacity: r.var_u64()?,
        };
        Ok(SynthRegion {
            stock: Stock::decode(r)?,
            herd,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ladder::laws;
    use crate::rng::{StreamKey, SystemId};
    use proptest::prelude::*;

    fn herd() -> impl Strategy<Value = Herd> {
        (any::<u64>(), 0u64..50, 0u64..50, 0u64..50)
            .prop_map(|(s, y, a, o)| Herd::new(s, [y, a, o]))
    }

    proptest! {
        #[test]
        fn herd_obeys_the_ladder_laws(h in herd(), slot in 0u64..200) {
            laws::take_then_fold_is_identity::<Herd, Sheep>(&h, slot).map_err(TestCaseError::fail)?;
            laws::take_conserves::<Herd, Sheep>(&h, slot).map_err(TestCaseError::fail)?;
            laws::sample_is_stable::<Herd, Sheep>(&h, slot).map_err(TestCaseError::fail)?;
        }

        #[test]
        fn herds_fold_into_fields(hs in prop::collection::vec(herd(), 0..20), cells in 1usize..64) {
            let f = Field::new(cells);
            laws::fold_adds_totals::<Field, Herd>(&f, hs.clone()).map_err(TestCaseError::fail)?;
            // The query answer does not depend on which level holds the sheep.
            let mut field = Field::new(cells);
            let total: u64 = hs.iter().map(|h| h.answer(&HeadCount)).sum();
            for h in hs { field.fold(h); }
            prop_assert_eq!(field.answer(&HeadCount), total);
        }

        #[test]
        fn herd_head_count_is_accounted(h in herd(), days in 0u64..200) {
            let start = h.total();
            let mut h = h;
            let key = StreamKey::new(1, SystemId::named("herd"), 0);
            for d in 0..days {
                h.commit_day(&mut key.slot(d), &HERD_PARAMS);
            }
            prop_assert_eq!(h.total() + h.died, start + h.born);
        }

        #[test]
        fn region_round_trips(h in herd(), amount in -1000i64..1000, rate in 0i64..(1 << 36), carry in any::<u32>()) {
            let r = SynthRegion {
                stock: Stock::new(amount, Diffuser::with_carry(rate, carry), 500),
                herd: h,
            };
            prop_assert_eq!(SynthRegion::from_bytes(&r.to_bytes()).unwrap(), r);
        }
    }

    #[test]
    fn stock_breakpoint_is_when_it_crosses() {
        let s = Stock::new(0, Diffuser::per(10, 24_000), 25);
        let bp = s.breakpoint(Tick(100)).unwrap();
        assert!(s.at(bp - Tick(100)) >= 25);
        assert!(s.at(bp - Tick(100) - 1) < 25);
    }

    #[test]
    fn same_slot_same_sheep() {
        let h = Herd::new(77, [3, 4, 5]);
        assert_eq!(h.sample(5), h.sample(5));
        assert_ne!(h.sample(5), h.sample(6));
        assert_eq!(h.sample(12), None);
        assert_eq!(h.sample(5).unwrap().cohort, ADULT);
    }
}
