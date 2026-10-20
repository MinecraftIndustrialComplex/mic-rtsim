// SPDX-License-Identifier: AGPL-3.0-or-later
//! No money pump (economy research failure table: "Money pump | Property test: any closed trade
//! cycle ends with no more money"; PLAN.md §8.5: "No loop of trades makes money").
//!
//! A player trades with one market in any sequence of buys and sells, with markups that change
//! between trades as the market's feelings change (never better than fair), and the sequence
//! returns the goods: the player ends with no more coins than it started with.

use proptest::prelude::*;
use rtsim_settle::Culture;
use rtsim_trade::line::Line;
use rtsim_trade::market::{FAIR, GoodState, MarketKind, markup_for};
use rtsim_trade::tunables::Ratio;
use rtsim_trade::{GoodId, MILLI, Market, PRICE_ONE, PriceCurve, Quantum};

fn market(anchor: i64, target_items: i64, stock_items: i64, treasury: i64, r: Ratio) -> Market {
    let target = target_items * MILLI;
    Market {
        key: 1,
        kind: MarketKind::Storehouse,
        culture: Some(Culture::Human),
        currency: Some(GoodId(1)),
        quantum: Quantum::HOUR,
        t_quanta: 48,
        lines: vec![
            Line::new(stock_items * MILLI, target),
            Line::new(treasury * MILLI, 1_000 * MILLI),
        ],
        goods: vec![
            GoodState {
                curve: PriceCurve::new(anchor, r, target),
                household: 0,
                nominal_cap: 0,
                inputs: vec![],
                used_here: false,
                currency: false,
            },
            GoodState {
                curve: PriceCurve::flat(PRICE_ONE),
                household: 0,
                nominal_cap: 0,
                inputs: vec![],
                used_here: false,
                currency: true,
            },
        ],
        order: vec![GoodId(0), GoodId(1)],
        base_prices: vec![None, None],
        news_resolution_div: 8,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn no_trade_cycle_makes_money(
        anchor in 1_000i64..20_000_000,
        target in 1i64..2_000,
        stock in 0i64..4_000,
        r_num in 0i64..4,
        trades in prop::collection::vec((any::<bool>(), 1i64..200, -128i8..=127), 1..30),
    ) {
        let r = Ratio::new(r_num, 4);
        // A treasury big enough that money never runs out mid-cycle (a short treasury only
        // refuses trades, which cannot make a pump).
        let mut m = market(anchor, target, stock, 1_000_000_000, r);
        let g = GoodId(0);
        let mut coins: i64 = 0; // the player's coins, relative to the start
        let mut items: i64 = 0; // the player's items of g, relative to the start
        let worst = Ratio::new(1, 2);
        let step = |m: &mut Market, buy: bool, n: i64, feeling: i8, coins: &mut i64, items: &mut i64| {
            let (up, down) = markup_for(feeling, worst);
            if buy {
                if let Ok(q) = m.quote_buy(g, n, up) {
                    m.apply_buy(g, q);
                    *coins -= q.coins;
                    *items += q.items;
                }
            } else {
                if let Ok(q) = m.quote_sell(g, n, down) {
                    m.apply_sell(g, q);
                    *coins += q.coins;
                    *items -= q.items;
                }
            }
        };
        for (buy, n, feeling) in trades {
            step(&mut m, buy, n, feeling, &mut coins, &mut items);
        }
        // Close the cycle: return every item, or buy back every item sold, at a fair price.
        if items > 0 {
            let q = m.quote_sell(g, items, FAIR);
            if let Ok(q) = q {
                m.apply_sell(g, q);
                coins += q.coins;
                items = 0;
            }
        } else if items < 0 {
            let q = m.quote_buy(g, -items, FAIR);
            if let Ok(q) = q {
                m.apply_buy(g, q);
                coins -= q.coins;
                items = 0;
            }
        }
        // A cycle the market refused to close (stock past twice its target) is not a cycle.
        prop_assume!(items == 0);
        prop_assert!(coins <= 0, "the player made {coins} coins from a closed cycle");
        prop_assert_eq!(m.stock(g), stock * MILLI);
    }

    /// The same holds with barter at anchor ratios: a loop of barters returns at most what went
    /// in.
    #[test]
    fn barter_loops_never_pay(anchor_a in 1_000i64..10_000_000, anchor_b in 1_000i64..10_000_000, give in 1i64..1_000) {
        let mut m = market(anchor_a, 100, 100, 1_000, Ratio::new(3, 4));
        // Second good: reuse the currency slot as "the other good" priced at anchor_b.
        m.goods[1].curve = PriceCurve::new(anchor_b, Ratio::new(3, 4), 1_000 * MILLI);
        m.lines[1].stock = 1_000_000 * MILLI;
        m.lines[0].stock = 1_000_000 * MILLI;
        let (a, b) = (GoodId(0), GoodId(1));
        let got_b = m.quote_barter(a, give, b);
        let back_a = m.quote_barter(b, got_b, a);
        prop_assert!(back_a <= give);
    }
}
