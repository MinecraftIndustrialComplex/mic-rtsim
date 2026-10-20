// SPDX-License-Identifier: AGPL-3.0-or-later
//! Storehouse markets (C13, PLAN.md §8.4, research/economy-2026-10-19.md).
//!
//! **The model.** Each market holds integer stocks and a treasury. Price is a pure function of
//! stock, anchored to the labour cost of the good relative to minting one coin, so nothing
//! integrates and nothing drifts. Production runs at capacity below the target stock, matches
//! outflow at it, and stops above it. Money is a good: a market mints while its treasury is below
//! target and refuses cash above twice the target. Every quantity is an integer number of
//! thousandths ([`units`]), every transfer is two-sided, and every rate is constant between
//! breakpoints, so a market's whole future until its next breakpoint is a closed form.
//!
//! - [`units`]: amounts, prices and the accrual quantum.
//! - [`goods`]: the goods registry; currencies are goods (C9: emeralds for humans, amethyst for
//!   ribbits).
//! - [`labour`]: labour costs `L_g` from an abstract recipe graph, the Hawkins-Simon lint and the
//!   other lints of the research's failure table.
//! - [`price`]: the price curve `p(S)` and the exact player trade integral in `i128`.
//! - [`line`]: one good at one market: the per-quantum law, its closed form and its breakpoints.
//! - [`market`]: a market as a kernel [`rtsim_kernel::lazy::LazyModel`] with no randomness, so the
//!   O5 slot cost does not apply; the economic rules (exportable and absorbable rates, exchange,
//!   cash refusal); player trades through the treasury (C14, C23).
//! - [`tunables`]: `r`, `T`, `k` and the rest, all PLACEHOLDERS (P9, P18) except `T`'s derived
//!   lower bound.
//!
//! Flows between markets, news and caravans are `rtsim-travel`'s; this crate only knows a
//! market's own rates.

pub mod build;
pub mod goods;
pub mod labour;
pub mod line;
pub mod market;
pub mod persist;
pub mod price;
pub mod tunables;
pub mod units;

pub use goods::{GoodDef, GoodId, GoodKind, Goods};
pub use line::{Level, Line, Rates};
pub use market::{Market, MarketKind, Offer, OfferSide, Quote, Refusal};
pub use price::PriceCurve;
pub use tunables::{Rules, Tunables};
pub use units::{Amount, MILLI, PRICE_ONE, Price, Quantum};
