// SPDX-License-Identifier: AGPL-3.0-or-later
//! The detail ladder (SCALE.md §2): every simulated kind exists at one level of detail at a time,
//! and five operations move between levels and answer questions.
//!
//! | Operation | Here |
//! |---|---|
//! | `fold` | [`Fold`]: absorb an instance of the level below, keeping every conserved total |
//! | `sample` | [`Sample::sample`]: derive the instance at a slot, changing nothing; [`Sample::take`] promotes it |
//! | `query` | [`Query`] and [`Answers`]: the only way systems read |
//! | `catch_up` | [`LazyModel::settle`] and [`LazyModel::commit_slot`], driven by [`crate::lazy::Lazy`] |
//! | `next_breakpoint` | [`LazyModel::next_breakpoint`] |
//!
//! **Systems call queries, never storage.** A query type names a question ("sheep within 2 km");
//! every level that can hold the data implements [`Answers`] for it. When data moves between
//! levels, the answering type changes and no system does. [`crate::lazy::LazyTable::query`] is the
//! entry point for lazy regions.
//!
//! [`laws`] holds the invariant checks SCALE.md §2 requires, written once so every kind's property
//! tests call the same code.

pub use crate::lazy::LazyModel;

/// The six levels, finest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// One row with full state: the ewe a player named.
    Individual,
    /// Counts per cohort with their distributions: a herd.
    Group,
    /// Counts per kind per grid cell.
    Field,
    /// Rates on a graph edge.
    Flow,
    /// Totals per region or province.
    Aggregate,
    /// Totals per faction or body.
    Strategic,
}

/// Which level an instance is at now.
pub trait OnLadder {
    fn level(&self) -> Level;
}

/// `fold`: absorbs `lower` into `self`. Every conserved quantity `lower` held is now held by
/// `self`.
pub trait Fold<Lower> {
    fn fold(&mut self, lower: Lower);
}

/// `sample` and its promoting twin `take`.
///
/// Slots index the instances a level holds, `0..population()`. The instance at a slot is derived
/// from the level's seed and state, so the same slot gives the same individual until the state
/// changes: this is why the same ewe with the same markings comes back.
pub trait Sample<Lower> {
    /// How many instances can be sampled.
    fn population(&self) -> u64;

    /// The instance at `slot`, or `None` past the population. Pure.
    fn sample(&self, slot: u64) -> Option<Lower>;

    /// The instance at `slot`, removed from this level: promotion to a row of its own.
    fn take(&mut self, slot: u64) -> Option<Lower>;
}

/// A question. `Answer` is what any level that holds the data returns.
pub trait Query {
    type Answer;
}

/// A level (or a view of one) that can answer `Q`.
pub trait Answers<Q: Query> {
    fn answer(&self, q: &Q) -> Q::Answer;
}

/// The SCALE.md §2 invariants as reusable checks. Each returns a description of the first
/// violation.
pub mod laws {
    use super::{Fold, Sample};
    use crate::conserve::Conserved;

    /// `fold` after `take` changes no total, and leaves `upper` as it was.
    pub fn take_then_fold_is_identity<U, L>(upper: &U, slot: u64) -> Result<(), String>
    where
        U: Sample<L> + Fold<L> + Conserved + Clone + PartialEq + std::fmt::Debug,
    {
        let before = upper.totals();
        let mut u = upper.clone();
        let Some(lower) = u.take(slot) else {
            return if slot >= upper.population() {
                Ok(())
            } else {
                Err(format!(
                    "take({slot}) failed below population {}",
                    upper.population()
                ))
            };
        };
        u.fold(lower);
        let after = u.totals();
        if after != before {
            return Err(format!("totals changed: {before:?} -> {after:?}"));
        }
        if &u != upper {
            return Err(format!("state changed: {upper:?} -> {u:?}"));
        }
        Ok(())
    }

    /// `take` moves exactly the sampled instance's totals out of `upper`.
    pub fn take_conserves<U, L>(upper: &U, slot: u64) -> Result<(), String>
    where
        U: Sample<L> + Conserved + Clone,
        L: Conserved,
    {
        let before = upper.totals();
        let mut u = upper.clone();
        let Some(lower) = u.take(slot) else {
            return Ok(());
        };
        let mut sum = u.totals();
        sum.merge(&lower.totals());
        if sum != before {
            return Err(format!("take({slot}): {before:?} became {sum:?}"));
        }
        Ok(())
    }

    /// `sample` is pure, and agrees with `take` on which instance a slot names.
    pub fn sample_is_stable<U, L>(upper: &U, slot: u64) -> Result<(), String>
    where
        U: Sample<L> + Clone,
        L: PartialEq + std::fmt::Debug,
    {
        let a = upper.sample(slot);
        let b = upper.sample(slot);
        if a != b {
            return Err(format!("sample({slot}) gave {a:?} then {b:?}"));
        }
        let taken = upper.clone().take(slot);
        if taken != a {
            return Err(format!(
                "sample({slot}) = {a:?} but take({slot}) = {taken:?}"
            ));
        }
        Ok(())
    }

    /// Folding any number of lowers adds exactly their totals.
    pub fn fold_adds_totals<U, L>(upper: &U, lowers: Vec<L>) -> Result<(), String>
    where
        U: Fold<L> + Conserved + Clone,
        L: Conserved,
    {
        let mut expect = upper.totals();
        let mut u = upper.clone();
        for l in lowers {
            expect.merge(&l.totals());
            u.fold(l);
        }
        let got = u.totals();
        if got != expect {
            return Err(format!("fold: expected {expect:?}, got {got:?}"));
        }
        Ok(())
    }
}
