// SPDX-License-Identifier: AGPL-3.0-or-later
//! The composition rule for markets (SCALE.md §4): `catch_up(t0 → t2)` equals `catch_up(t1 → t2)`
//! applied after `catch_up(t0 → t1)`, for any `t1`, and reads change nothing. Markets have no
//! randomness, so this is about the closed form: settling across any split of time gives the same
//! state, books included, and the breakpoints booked do not depend on who read when.

use proptest::prelude::*;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::lazy::{Lazy, LazyModel, LazyTable};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_settle::Culture;
use rtsim_trade::line::{Line, Rates};
use rtsim_trade::market::{GoodState, Input, MarketKind};
use rtsim_trade::tunables::Ratio;
use rtsim_trade::{GoodId, MILLI, Market, PRICE_ONE, PriceCurve, Quantum};

/// Three goods: grain (input), bread (made from grain), and the currency.
fn market(stocks: [i64; 3], caps: [i64; 2], cons: [i64; 2], flows: [(i64, i64); 3]) -> Market {
    let r = Ratio::new(3, 4);
    let targets = [
        48 * caps[0].max(cons[0]).max(1),
        48 * caps[1].max(cons[1]).max(1),
        20_000,
    ];
    let mut lines: Vec<Line> = (0..3).map(|i| Line::new(stocks[i], targets[i])).collect();
    for i in 0..3 {
        lines[i].rates = Rates {
            cap: if i < 2 { caps[i] } else { 0 },
            cons: if i < 2 { cons[i] } else { 0 },
            inflow: flows[i].0,
            outflow: flows[i].1,
            slack: 0,
        };
    }
    let gs = |i: usize, inputs: Vec<Input>, used: bool| GoodState {
        curve: if i == 2 {
            PriceCurve::flat(PRICE_ONE)
        } else {
            PriceCurve::new(50_000, r, targets[i])
        },
        household: if i < 2 { cons[i] } else { 0 },
        nominal_cap: if i < 2 { caps[i] } else { 0 },
        inputs,
        used_here: used,
        currency: i == 2,
    };
    let mut m = Market {
        key: 9,
        kind: MarketKind::Storehouse,
        culture: Some(Culture::Human),
        currency: Some(GoodId(2)),
        quantum: Quantum::HOUR,
        t_quanta: 48,
        lines,
        goods: vec![
            gs(0, vec![], true),
            gs(
                1,
                vec![Input {
                    good: GoodId(0),
                    per: 3 * MILLI,
                    yield_: MILLI,
                }],
                false,
            ),
            gs(2, vec![], false),
        ],
        order: vec![GoodId(1), GoodId(0), GoodId(2)],
        base_prices: vec![None; 3],
        rules: rtsim_trade::tunables::Rules::default(),
    };
    m.recompute();
    m
}

fn arb_market() -> impl Strategy<Value = Market> {
    (
        prop::array::uniform3(0i64..200_000),
        prop::array::uniform2(0i64..2_000),
        prop::array::uniform2(0i64..2_000),
        prop::array::uniform3((0i64..1_500, 0i64..1_000)),
    )
        .prop_map(|(s, c, k, f)| market(s, c, k, f))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Settling over [t0, t2] in one go or with a stop anywhere in between gives the same market,
    /// stock and books.
    #[test]
    fn settle_composes(m in arb_market(), t1 in 0u64..500_000, t2 in 0u64..500_000) {
        let (t1, t2) = (t1.min(t2), t1.max(t2));
        let key = StreamKey::new(1, SystemId::named("t"), 9);
        let mut whole = Lazy::new(m.clone(), Tick(0));
        let mut split = Lazy::new(m.clone(), Tick(0));
        whole.apply(Tick(t2), key, |_, _| ());
        split.apply(Tick(t1), key, |_, _| ());
        split.apply(Tick(t2), key, |_, _| ());
        prop_assert_eq!(whole.model(), split.model());
        // A read projects without committing.
        let before = split.model().clone();
        let v = split.read(Tick(t2 + 12_345), key);
        prop_assert_eq!(split.model(), &before);
        let mut later = split.clone();
        later.apply(Tick(t2 + 12_345), key, |_, _| ());
        prop_assert_eq!(&v, later.model());
    }

    /// In a lazy table, the breakpoints a market books and the states they fire at do not depend
    /// on reads in between.
    #[test]
    fn breakpoints_ignore_reads(m in arb_market(), reads in prop::collection::vec(0u64..400_000, 0..12)) {
        let horizon = Tick(400_000);
        let run = |reads: &[u64]| {
            let mut t: LazyTable<Market> = LazyTable::new(1, SystemId::named("trade.market"), Tick(0));
            let h = t.insert(9, m.clone(), Tick(0));
            let mut reads: Vec<u64> = reads.to_vec();
            reads.sort_unstable();
            let mut fired = Vec::new();
            let mut ri = 0;
            loop {
                let next_read = reads.get(ri).copied().map(Tick);
                let limit = next_read.unwrap_or(horizon).min(horizon);
                if let Some((at, _)) = t.next_due(limit) {
                    // Act on it as a world would: recompute.
                    t.apply(h, at, |m, _| m.recompute());
                    fired.push((at, t.get(h).unwrap().cell().model().lines.clone()));
                    continue;
                }
                match next_read {
                    Some(r) if r <= horizon => {
                        let _ = t.read(h, r);
                        ri += 1;
                    }
                    _ => break,
                }
            }
            (fired, t.get(h).unwrap().cell().view_without_catch_up(horizon))
        };
        let quiet = run(&[]);
        let busy = run(&reads);
        prop_assert_eq!(quiet.0, busy.0);
        prop_assert_eq!(quiet.1, busy.1);
    }
}

#[test]
fn a_view_is_the_model_settled() {
    let m = market(
        [1_000, 0, 5_000],
        [500, 100],
        [200, 50],
        [(0, 0), (0, 0), (0, 0)],
    );
    let v = m.view(Tick(0), Tick(10_000));
    let mut s = m.clone();
    s.settle(Tick(0), Tick(10_000));
    assert_eq!(v, s);
}
