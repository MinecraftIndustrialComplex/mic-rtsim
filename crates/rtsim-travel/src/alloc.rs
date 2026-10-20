// SPDX-License-Identifier: AGPL-3.0-or-later
//! Allocation by news margin (PLAN.md §8.4 "Flows"): on an event, a market gives its exportable
//! rate to the edges whose news margin `p_j - p_i - τ_ij` is not negative, best margin first, each
//! up to what the destination advertised it can absorb.
//!
//! A margin of exactly zero keeps a flow: at steady state an importer that cannot make a good sits
//! at its target with an anchor of exactly the exporter's plus transport, so a strict test would
//! stop the flow, let the importer drain, and restart it, forever (overnight choice E14).
//!
//! What a destination wants is its advertised absorbable rate plus what this exporter was already
//! landing there when the news was written: the advertisement is net of all inbound flows,
//! including this one.

use rtsim_trade::{Amount, Price};

use crate::edge::DirRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub dir: DirRef,
    /// The destination's price converted to the exporter's currency, minus the exporter's own
    /// price and the transport cost, millionths of a coin per item.
    pub margin: Price,
    /// Thousandths per quantum the destination would take from this exporter.
    pub want: Amount,
    /// The price agreed for the flow, in the payer's currency.
    pub quote: Price,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grant {
    pub dir: DirRef,
    pub rate: Amount,
    pub quote: Price,
}

/// Grants `exportable` to the candidates, best margin first (ties by direction, so the result
/// never depends on input order). Candidates with a negative margin or no want get zero, and are
/// still listed so their flows stop.
pub fn allocate(exportable: Amount, cands: Vec<Candidate>) -> Vec<Grant> {
    allocate_with(exportable, cands, false)
}

/// As [`allocate`], but with `ration` a shortage is shared in proportion to what each
/// destination wants instead of best margin first: when two importers compete for one exporter's
/// spare goods, best-first hands everything to whichever is shorter by the last news, which then
/// fills and loses it to the other, back and forth (overnight choice E49). Destinations with a
/// negative margin still get nothing.
pub fn allocate_with(exportable: Amount, mut cands: Vec<Candidate>, ration: bool) -> Vec<Grant> {
    cands.sort_by(|a, b| b.margin.cmp(&a.margin).then(a.dir.cmp(&b.dir)));
    let x = exportable.max(0);
    let ok = |c: &Candidate| c.margin >= 0 && c.want > 0;
    let wanted: i128 = cands
        .iter()
        .filter(|c| ok(c))
        .map(|c| i128::from(c.want))
        .sum();
    if ration && wanted > i128::from(x) {
        let mut grants: Vec<Grant> = cands
            .iter()
            .map(|c| Grant {
                dir: c.dir,
                rate: if ok(c) {
                    (i128::from(x) * i128::from(c.want) / wanted) as Amount
                } else {
                    0
                },
                quote: c.quote,
            })
            .collect();
        // What floor rounding leaves goes to the one that wants most (the first among equals),
        // so exactly `x` is granted and holding a total never ratchets it down a thousandth at a
        // time.
        let left = x - grants.iter().map(|g| g.rate).sum::<Amount>();
        if left > 0
            && let Some(k) = (0..cands.len())
                .filter(|&k| ok(&cands[k]))
                .max_by_key(|&k| (cands[k].want, std::cmp::Reverse(k)))
        {
            grants[k].rate += left;
        }
        return grants;
    }
    let mut left = x;
    cands
        .into_iter()
        .map(|c| {
            let rate = if c.margin >= 0 && c.want > 0 {
                c.want.min(left)
            } else {
                0
            };
            left -= rate;
            Grant {
                dir: c.dir,
                rate,
                quote: c.quote,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(edge: u32, margin: i64, want: i64) -> Candidate {
        Candidate {
            dir: DirRef { edge, dir: 0 },
            margin,
            want,
            quote: 1,
        }
    }

    #[test]
    fn rationing_grants_exactly_what_there_is() {
        let g = allocate_with(
            100,
            vec![c(1, 5, 70), c(2, 9, 70), c(3, 0, 70), c(4, -1, 70)],
            true,
        );
        let rate = |e: u32| g.iter().find(|x| x.dir.edge == e).unwrap().rate;
        assert_eq!(rate(1) + rate(2) + rate(3), 100);
        assert_eq!(rate(4), 0);
        assert!((rate(1) - rate(2)).abs() <= 1 && (rate(2) - rate(3)).abs() <= 1);
    }

    #[test]
    fn best_margin_first_up_to_want() {
        let g = allocate(
            100,
            vec![c(1, 5, 80), c(2, 9, 50), c(3, -1, 500), c(4, 0, 500)],
        );
        let rate = |e: u32| g.iter().find(|x| x.dir.edge == e).unwrap().rate;
        assert_eq!(rate(2), 50);
        assert_eq!(rate(1), 50);
        assert_eq!(rate(4), 0);
        assert_eq!(rate(3), 0);
    }
}
