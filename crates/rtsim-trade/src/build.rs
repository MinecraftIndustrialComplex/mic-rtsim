// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compiling markets from data: labour costs over the network, anchors, targets and exchange
//! bands (PLAN.md §8.4, economy research §2 and §5 "derived versus tunable").
//!
//! 1. Each market solves its own recipes ([`RecipeBook::solve`]).
//! 2. A good a market cannot make costs what the cheapest neighbour pays plus transport labour
//!    (`min_j L_g(j) + transport labor`): a shortest path per good over the edges, then the local
//!    solve again, since an imported input can make a local recipe cheaper. Repeated until nothing
//!    changes (a few rounds: as deep as the recipe graph).
//! 3. `L_mint` is the cost of one coin at its culture's cheapest mint, the same for every market of
//!    the culture. The anchor is `p*_g = L_g / L_mint`.
//! 4. Targets: `Sbar = T × max(c, y)` with `T` at least twice the market's longest edge, and
//!    `Mbar = k × pop / L_mint`.
//! 5. Border markets (an edge to another culture) get an exchange line for each foreign currency:
//!    anchored at mint parity `L_mint(foreign) / L_mint(home)`, floating within plus or minus the
//!    cost of carrying a coin home over the market's shortest border edge (the gold points).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use rtsim_kernel::clock::TICKS_PER_DAY;
use rtsim_settle::Culture;

use crate::goods::{GoodId, GoodKind, Goods};
use crate::labour::{Costs, Lint, Local, RecipeBook, lint_world};
use crate::line::Line;
use crate::market::{GoodState, Input, Market, MarketKind};
use crate::price::{MAX_ANCHOR, PriceCurve};
use crate::tunables::{Ratio, Tunables};
use crate::units::{Amount, MILLI, PRICE_ONE};

/// One market to build.
#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub key: u64,
    pub kind: MarketKind,
    pub culture: Option<Culture>,
    pub local: Local,
}

/// An edge between two nodes (indices into the node list), with its travel time.
#[derive(Clone, Copy, Debug)]
pub struct LinkSpec {
    pub a: usize,
    pub b: usize,
    pub d_quanta: u64,
}

/// How a built world starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genesis {
    /// Every stock and treasury at its target: a quiet world.
    AtTarget,
    /// Stocks at this share of target, in thousandths (treasuries at target). For scenarios that
    /// should start busy.
    Share(i64),
}

pub struct Built {
    pub markets: Vec<Market>,
    pub costs: Vec<Costs>,
    /// Each culture's cheapest mint cost, worker-days per coin (mint parity uses these).
    pub mint_cost: [f64; 2],
    pub lints: Vec<Lint>,
}

/// Transport labour per item of `g` over `d_quanta`, in worker-days: `τ = upkeep × d / capacity`.
pub fn transport_labour(goods: &Goods, tun: &Tunables, g: GoodId, d_quanta: u64) -> f64 {
    let days = d_quanta as f64 * tun.quantum.len() as f64 / TICKS_PER_DAY as f64;
    let capacity_items = f64::from(tun.caravan_slots) * f64::from(goods.get(g).stack.max(1));
    tun.upkeep_worker_days_per_day * days / capacity_items
}

/// Transport cost per item of `g` over `d_quanta` in coins of a culture whose mint costs `l_mint`
/// worker-days a coin, millionths: `τ_ij = upkeep × d_ij / capacity`, priced like an anchor.
pub fn transport_price(goods: &Goods, tun: &Tunables, g: GoodId, d_quanta: u64, l_mint: f64) -> i64 {
    anchor_of(transport_labour(goods, tun, g, d_quanta), l_mint)
}

/// Integer prices are each rounded once, so a margin built from three of them can be off by up to
/// this many millionths when the true margin is zero. Not a tunable: the rounding's own bound.
pub const MARGIN_SLACK: i64 = 2;

fn anchor_of(labour: f64, l_mint: f64) -> i64 {
    if !labour.is_finite() || !l_mint.is_finite() || l_mint <= 0.0 {
        return 0;
    }
    ((labour / l_mint) * PRICE_ONE as f64)
        .round()
        .clamp(0.0, MAX_ANCHOR as f64) as i64
}

/// The mint recipe's cost ignoring its resource: the fallback `L_mint` for a world with no mint
/// in reach at all (overnight choice E9), so prices stay defined.
fn notional_mint(book: &RecipeBook, cur: GoodId) -> f64 {
    book.recipes
        .iter()
        .filter(|r| r.output == cur && r.inputs.is_empty() && r.yield_per_batch > 0)
        .map(|r| r.labour / (r.yield_per_batch as f64 / MILLI as f64))
        .fold(f64::INFINITY, f64::min)
}

/// Builds every market. `nodes` and `links` describe the network; the result is in node order.
pub fn build_markets(
    goods: &Goods,
    book: &RecipeBook,
    tun: &Tunables,
    nodes: &[NodeSpec],
    links: &[LinkSpec],
    genesis: Genesis,
) -> Built {
    let n = nodes.len();
    let g_n = goods.len();
    let mut adj: Vec<Vec<(usize, u64)>> = vec![Vec::new(); n];
    for l in links {
        adj[l.a].push((l.b, l.d_quanta));
        adj[l.b].push((l.a, l.d_quanta));
    }
    let max_d: Vec<u64> = adj
        .iter()
        .map(|v| v.iter().map(|x| x.1).max().unwrap_or(0))
        .collect();

    // 1 and 2: local solves, then shortest paths per good, until stable.
    let mut imported = vec![vec![f64::INFINITY; g_n]; n];
    let mut costs: Vec<Costs> = Vec::with_capacity(n);
    for _round in 0..16 {
        costs = nodes
            .iter()
            .enumerate()
            .map(|(i, nd)| book.solve(goods, &nd.local, &imported[i], tun.craft_share_milli))
            .collect();
        let mut changed = false;
        for g in goods.ids() {
            let gi = g.index();
            // Multi-source Dijkstra from every market's own cost of g.
            let mut best: Vec<f64> = costs.iter().map(|c| c.labour[gi]).collect();
            let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
            for (i, &b) in best.iter().enumerate() {
                if b.is_finite() {
                    heap.push(Reverse((b.to_bits(), i)));
                }
            }
            while let Some(Reverse((bits, i))) = heap.pop() {
                let c = f64::from_bits(bits);
                if c > best[i] {
                    continue;
                }
                for &(j, d) in &adj[i] {
                    let via = c + transport_labour(goods, tun, g, d);
                    if via < best[j] * (1.0 - 1e-12) {
                        best[j] = via;
                        heap.push(Reverse((via.to_bits(), j)));
                    }
                }
            }
            for i in 0..n {
                let imp = if best[i] < costs[i].labour[gi] {
                    best[i]
                } else {
                    f64::INFINITY
                };
                if (imp - imported[i][gi]).abs() > 1e-12 * imp.abs().max(1.0)
                    && !(imp.is_infinite() && imported[i][gi].is_infinite())
                {
                    imported[i][gi] = imp;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Positive f64s order like their bits, which is why the heap keys on them.

    // 3: mint costs.
    let mut mint_cost = [f64::INFINITY; 2];
    for c in Culture::ALL {
        let Some(cur) = goods.currency(c) else {
            continue;
        };
        let cheapest = costs
            .iter()
            .filter(|k| k.recipe[cur.index()].is_some())
            .map(|k| k.labour[cur.index()])
            .fold(f64::INFINITY, f64::min);
        mint_cost[c.index()] = if cheapest.is_finite() {
            cheapest
        } else {
            notional_mint(book, cur)
        };
    }

    let mut markets = Vec::with_capacity(n);
    for (i, nd) in nodes.iter().enumerate() {
        let border = adj[i]
            .iter()
            .filter(|&&(j, _)| nodes[j].culture.is_some() && nodes[j].culture != nd.culture)
            .map(|&(_, d)| d)
            .min();
        markets.push(make_market(
            goods, book, tun, nd, &costs[i], mint_cost, max_d[i], border, genesis,
        ));
    }

    let needs: Vec<bool> = goods
        .iter()
        .map(|(_, d)| d.need_per_pop_day > 0)
        .collect();
    let locals: Vec<&Local> = nodes.iter().map(|n| &n.local).collect();
    let lints = lint_world(goods, book, &locals, &needs);
    Built {
        markets,
        costs,
        mint_cost,
        lints,
    }
}

/// Costs for a market joining an existing network (C22: villages join as their structure starts
/// generate): its own recipes, with imports priced from its new neighbours' costs plus transport.
/// The neighbours keep their cached costs; a newcomer never reprices the world (overnight choice
/// E15).
pub fn costs_for_newcomer(
    goods: &Goods,
    book: &RecipeBook,
    tun: &Tunables,
    local: &Local,
    neighbours: &[(&Costs, u64)],
) -> Costs {
    let mut imported = vec![f64::INFINITY; goods.len()];
    for g in goods.ids() {
        for &(c, d) in neighbours {
            let via = c.labour[g.index()] + transport_labour(goods, tun, g, d);
            if via < imported[g.index()] {
                imported[g.index()] = via;
            }
        }
    }
    book.solve(goods, local, &imported, tun.craft_share_milli)
}

/// One market from its costs: anchors, targets, lines and the recipe coupling. `max_d` is its
/// longest edge (for `T`'s bound) and `border_d` its shortest edge to another culture, if any (for
/// the exchange band).
#[allow(clippy::too_many_arguments)]
pub fn make_market(
    goods: &Goods,
    book: &RecipeBook,
    tun: &Tunables,
    nd: &NodeSpec,
    costs: &Costs,
    mint_cost: [f64; 2],
    max_d: u64,
    border_d: Option<u64>,
    genesis: Genesis,
) -> Market {
    let qn = tun.quantum;
    let g_n = goods.len();
    {
        let cost = costs;
        let currency = nd.culture.and_then(|c| goods.currency(c));
        // L_mint is the culture's mint cost, the same in every market of the culture: an emerald
        // is the same labour wherever it is, so prices in emeralds compare across markets and a
        // steady flow's margin is exactly zero up to rounding (overnight choice E16).
        let l_mint = match (nd.culture, currency) {
            (Some(c), Some(_)) => mint_cost[c.index()],
            _ => f64::INFINITY,
        };
        let t_quanta = tun.target_quanta(max_d);
        let pop = i64::from(nd.local.pop);
        // Inputs of the chosen recipes, own output netted out of the yield.
        let mut inputs: Vec<Vec<Input>> = vec![Vec::new(); g_n];
        let mut net_cap = vec![0 as Amount; g_n];
        for g in goods.ids() {
            let gi = g.index();
            let Some(r) = cost.recipe[gi] else {
                continue;
            };
            let rec = &book.recipes[r];
            let own: Amount = rec
                .inputs
                .iter()
                .filter(|x| x.0 == g)
                .map(|x| x.1)
                .sum();
            let net_yield = (rec.yield_per_batch - own).max(1);
            net_cap[gi] = qn.per_day(
                (i128::from(cost.capacity_per_day[gi]) * i128::from(net_yield)
                    / i128::from(rec.yield_per_batch)) as Amount,
            );
            inputs[gi] = rec
                .inputs
                .iter()
                .filter(|x| x.0 != g)
                .map(|&(good, per)| Input {
                    good,
                    per,
                    yield_: net_yield,
                })
                .collect();
        }
        let mut used_here = vec![false; g_n];
        let mut input_use = vec![0 as Amount; g_n];
        for gi in 0..g_n {
            for inp in &inputs[gi] {
                used_here[inp.good.index()] = true;
                input_use[inp.good.index()] += (i128::from(net_cap[gi]) * i128::from(inp.per)
                    / i128::from(inp.yield_)) as Amount;
            }
        }
        let mbar = if l_mint.is_finite() && l_mint > 0.0 {
            ((tun.k_days as f64 * pop as f64 / l_mint) * MILLI as f64).round() as Amount
        } else {
            0
        };
        let mut lines = Vec::with_capacity(g_n);
        let mut states = Vec::with_capacity(g_n);
        for g in goods.ids() {
            let gi = g.index();
            let def = goods.get(g);
            let household = qn.per_day(pop * def.need_per_pop_day);
            let (target, curve, cap) = match def.kind {
                GoodKind::Item => {
                    let target = tun.target(t_quanta, household + input_use[gi], net_cap[gi]);
                    let anchor = anchor_of(cost.labour[gi], l_mint);
                    (target, PriceCurve::new(anchor, tun.r, target), net_cap[gi])
                }
                GoodKind::Currency(c) if Some(g) == currency => {
                    // The treasury: one coin is worth one coin; the mint fills it toward Mbar.
                    let _ = c;
                    (mbar, PriceCurve::flat(PRICE_ONE), net_cap[gi])
                }
                GoodKind::Currency(c) => {
                    // A foreign currency: an exchange line at border markets.
                    let parity = if l_mint.is_finite() {
                        mint_cost[c.index()] / l_mint
                    } else {
                        0.0
                    };
                    let anchor = (parity * PRICE_ONE as f64).round().clamp(0.0, MAX_ANCHOR as f64)
                        as i64;
                    match border_d {
                        Some(d) if anchor > 0 => {
                            let tau = transport_labour(goods, tun, g, d) / l_mint;
                            let band = ((tau / parity) * 1024.0).round().clamp(0.0, 1023.0) as i64;
                            let target = (i128::from(mbar) * i128::from(tun.fx_holding_share_milli)
                                / 1000
                                * i128::from(PRICE_ONE)
                                / i128::from(anchor.max(1)))
                                as Amount;
                            (
                                target,
                                PriceCurve::new(anchor, Ratio::new(band, 1024), target),
                                0,
                            )
                        }
                        _ => (0, PriceCurve::new(anchor, Ratio::new(0, 1), 0), 0),
                    }
                }
            };
            let stock = match (genesis, def.kind) {
                (_, GoodKind::Currency(_)) if Some(g) != currency => 0,
                (Genesis::AtTarget, _) => target,
                (Genesis::Share(_), GoodKind::Currency(_)) => target,
                (Genesis::Share(s), GoodKind::Item) => {
                    (i128::from(target) * i128::from(s) / 1000) as Amount
                }
            };
            let mut line = Line::new(stock, target);
            line.rates.cap = cap;
            line.book.genesis = i128::from(stock);
            if nd.kind == MarketKind::Base {
                line = Line::default();
            }
            lines.push(line);
            states.push(GoodState {
                curve,
                household,
                nominal_cap: cap,
                inputs: std::mem::take(&mut inputs[gi]),
                used_here: used_here[gi],
                currency: def.kind != GoodKind::Item,
            });
        }
        let order = recipe_order(&states);
        let mut m = Market {
            key: nd.key,
            kind: nd.kind,
            culture: nd.culture,
            currency,
            quantum: qn,
            t_quanta,
            lines,
            goods: states,
            order,
            base_prices: vec![None; g_n],
            news_resolution_div: tun.news_resolution_div,
        };
        m.recompute();
        m
    }
}

/// Goods ordered so that every good comes before the inputs of its recipe (a depth-first
/// post-order on "is an input of", reversed). Cycles beyond seed grain are cut at the first good
/// met again; [`Market::recompute`] then lags one event for them.
fn recipe_order(states: &[GoodState]) -> Vec<GoodId> {
    let n = states.len();
    let mut seen = vec![false; n];
    let mut post = Vec::with_capacity(n);
    fn visit(g: usize, states: &[GoodState], seen: &mut [bool], post: &mut Vec<GoodId>) {
        if seen[g] {
            return;
        }
        seen[g] = true;
        for inp in &states[g].inputs {
            visit(inp.good.index(), states, seen, post);
        }
        post.push(GoodId(g as u16));
    }
    for g in 0..n {
        visit(g, states, &mut seen, &mut post);
    }
    post.reverse();
    post
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goods::GoodDef;
    use crate::labour::{Recipe, ResourceId};

    #[test]
    fn imports_cost_transport_and_borders_get_exchange_lines() {
        let mut goods = Goods::new();
        let item = |name: &str, need| GoodDef {
            name: name.into(),
            kind: GoodKind::Item,
            need_per_pop_day: need,
            stack: 64,
        };
        let wheat = goods.add(item("wheat", 500));
        let emerald = goods.add(GoodDef {
            name: "emerald".into(),
            kind: GoodKind::Currency(Culture::Human),
            need_per_pop_day: 0,
            stack: 64,
        });
        let amethyst = goods.add(GoodDef {
            name: "amethyst".into(),
            kind: GoodKind::Currency(Culture::Ribbit),
            need_per_pop_day: 0,
            stack: 64,
        });
        let (farm, ore, geode) = (ResourceId(0), ResourceId(1), ResourceId(2));
        let book = RecipeBook {
            recipes: vec![
                Recipe {
                    name: "farm".into(),
                    output: wheat,
                    yield_per_batch: 10 * MILLI,
                    inputs: vec![],
                    labour: 1.0,
                    resource: Some(farm),
                },
                Recipe {
                    name: "mine emerald".into(),
                    output: emerald,
                    yield_per_batch: MILLI,
                    inputs: vec![],
                    labour: 2.0,
                    resource: Some(ore),
                },
                Recipe {
                    name: "harvest amethyst".into(),
                    output: amethyst,
                    yield_per_batch: 4 * MILLI,
                    inputs: vec![],
                    labour: 1.0,
                    resource: Some(geode),
                },
            ],
        };
        let mut a = Local {
            pop: 20,
            ..Local::default()
        };
        a.resources.insert(farm, 50 * MILLI);
        a.resources.insert(ore, 2 * MILLI);
        let b = Local {
            pop: 10,
            ..Local::default()
        };
        let mut r = Local {
            pop: 10,
            ..Local::default()
        };
        r.resources.insert(geode, 8 * MILLI);
        let nodes = vec![
            NodeSpec {
                key: 1,
                kind: MarketKind::Storehouse,
                culture: Some(Culture::Human),
                local: a,
            },
            NodeSpec {
                key: 2,
                kind: MarketKind::Storehouse,
                culture: Some(Culture::Human),
                local: b,
            },
            NodeSpec {
                key: 3,
                kind: MarketKind::Storehouse,
                culture: Some(Culture::Ribbit),
                local: r,
            },
        ];
        let links = [
            LinkSpec {
                a: 0,
                b: 1,
                d_quanta: 6,
            },
            LinkSpec {
                a: 1,
                b: 2,
                d_quanta: 6,
            },
        ];
        let tun = Tunables::default();
        let built = build_markets(&goods, &book, &tun, &nodes, &links, Genesis::AtTarget);
        let [ma, mb, mr] = &built.markets[..] else {
            panic!()
        };
        // B cannot farm: its wheat costs A's plus carrying it 6 quanta.
        let la = built.costs[0].labour[wheat.index()];
        let lb = built.costs[1].labour[wheat.index()];
        assert!((lb - la - transport_labour(&goods, &tun, wheat, 6)).abs() < 1e-12);
        // Anchors in coins: A mints at 2 worker-days a coin, wheat costs 0.1: 1/20 coin. B pays
        // A's price plus transport, in the same emeralds.
        assert_eq!(ma.anchor(wheat), 50_000);
        assert!(mb.anchor(wheat) > ma.anchor(wheat));
        // T is at least twice the longest edge.
        assert!(mb.t_quanta >= 12);
        // Only B and R are border markets; B prices amethyst at mint parity: 0.25 worker-days
        // per shard over 2 per emerald, an eighth of an emerald.
        assert!(mb.line(amethyst).target > 0);
        assert_eq!(ma.line(amethyst).target, 0);
        assert!(mr.line(emerald).target > 0);
        assert_eq!(mb.anchor(amethyst), PRICE_ONE / 8);
        assert_eq!(mr.anchor(emerald), 8 * PRICE_ONE);
        // Treasuries start at Mbar.
        assert!(ma.treasury() > 0 && ma.treasury() == ma.line(emerald).target);
        assert!(built.lints.is_empty(), "{:?}", built.lints);
    }
}
