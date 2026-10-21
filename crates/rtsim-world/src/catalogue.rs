// SPDX-License-Identifier: AGPL-3.0-or-later
//! The goods and recipes a live phase 1 world trades: the one list the native link builds its
//! world with and anything reading its pages back (rtsim-view) decodes them with, so the two
//! cannot drift apart. A snapshot records only how many goods and recipes it had.
//!
//! PLACEHOLDER (choice V5): today the synthetic catalogue (`crate::synth`); P9 and P18 replace it
//! with measured ones, here.

use rtsim_trade::Goods;
use rtsim_trade::labour::RecipeBook;

use crate::synth;

/// The phase 1 world's goods and recipes.
pub fn catalogue() -> (Goods, RecipeBook) {
    let (goods, ids) = synth::goods();
    let book = synth::recipes(&ids);
    (goods, book)
}
