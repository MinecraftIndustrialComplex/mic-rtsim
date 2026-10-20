// SPDX-License-Identifier: AGPL-3.0-or-later
//! Goods. Currencies are goods (economy research): each culture's money has a [`GoodKind`] that
//! says so, and a market's treasury is simply its stock of its own culture's currency.
//!
//! In game, goods are compiled from data (datapack JSON and the server's `RecipeManager`,
//! PLAN.md §8.4); here they are a plain registry so the engine runs headless.

use rtsim_settle::Culture;

use crate::units::Amount;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GoodId(pub u16);

impl GoodId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GoodKind {
    /// Anything traded, made and eaten.
    Item,
    /// A culture's money (C9).
    Currency(Culture),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoodDef {
    pub name: String,
    pub kind: GoodKind,
    /// What one resident needs per day, in thousandths of an item: consumption `c = pop × need`
    /// (PLAN.md §8.4). Data; PLACEHOLDER values in the scenario until P18's soak.
    pub need_per_pop_day: Amount,
    /// Items per inventory slot (64 for most, 16 for some). A caravan's capacity in items is its
    /// slots times this, so transport cost per item depends on it.
    pub stack: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Goods {
    defs: Vec<GoodDef>,
}

impl Goods {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, def: GoodDef) -> GoodId {
        assert!(
            self.defs.len() < u16::MAX as usize,
            "too many goods for a u16 id"
        );
        if let GoodKind::Currency(c) = def.kind {
            assert!(
                self.currency(c).is_none(),
                "culture {c:?} already has a currency"
            );
        }
        self.defs.push(def);
        GoodId((self.defs.len() - 1) as u16)
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn get(&self, g: GoodId) -> &GoodDef {
        &self.defs[g.index()]
    }

    pub fn ids(&self) -> impl Iterator<Item = GoodId> + use<> {
        (0..self.defs.len() as u16).map(GoodId)
    }

    pub fn iter(&self) -> impl Iterator<Item = (GoodId, &GoodDef)> {
        self.defs
            .iter()
            .enumerate()
            .map(|(i, d)| (GoodId(i as u16), d))
    }

    pub fn find(&self, name: &str) -> Option<GoodId> {
        self.defs
            .iter()
            .position(|d| d.name == name)
            .map(|i| GoodId(i as u16))
    }

    /// The good that is `culture`'s money.
    pub fn currency(&self, culture: Culture) -> Option<GoodId> {
        self.defs
            .iter()
            .position(|d| d.kind == GoodKind::Currency(culture))
            .map(|i| GoodId(i as u16))
    }

    pub fn is_currency(&self, g: GoodId) -> bool {
        matches!(self.get(g).kind, GoodKind::Currency(_))
    }

    pub fn currency_culture(&self, g: GoodId) -> Option<Culture> {
        match self.get(g).kind {
            GoodKind::Currency(c) => Some(c),
            GoodKind::Item => None,
        }
    }
}
