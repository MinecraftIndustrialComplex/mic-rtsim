// SPDX-License-Identifier: AGPL-3.0-or-later
//! Labour costs from an abstract recipe graph (PLAN.md §8.4, economy research §2).
//!
//! `L_g = min over recipes r making g of (l_r + Σ a_ri L_i) / y_r`, where `l_r` is the labour of
//! one batch in worker-days, `a_ri` its inputs and `y_r` its yield. A recipe that needs a local
//! resource (farmland, ore, a geode) is available only where that resource flows (`F_r > 0`), and
//! its capacity is capped by the flow, so local resources enter only through `F_r` and hence
//! through `L_g` and capacity (C9: prices follow local resource availability). Goods a market
//! cannot make are priced from its neighbours plus transport labour (`rtsim-world` relaxes that
//! over the edges).
//!
//! Cycles (seed grain that needs grain) are solved by value iteration, then exactly as
//! `L = (I - A^T)^-1 l` for the chosen recipes. The Hawkins-Simon lint checks that the chosen
//! input matrix is productive, so a cycle cannot cost less than nothing (Veloren's price-cost loop,
//! economy research §1).
//!
//! In game, recipes come from the server's `RecipeManager` (PLAN.md §6.3); here they are data.
//! Floats are used only here, once, when data is compiled; prices become integers after it.

use std::collections::BTreeMap;

use crate::goods::{GoodId, Goods};
use crate::units::{Amount, MILLI};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u16);

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub name: String,
    pub output: GoodId,
    /// Output per batch, in thousandths.
    pub yield_per_batch: Amount,
    /// Inputs per batch, in thousandths.
    pub inputs: Vec<(GoodId, Amount)>,
    /// Labour per batch, in worker-days.
    pub labour: f64,
    /// The local resource the recipe needs, if any.
    pub resource: Option<ResourceId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeBook {
    pub recipes: Vec<Recipe>,
}

/// What a market has locally: its population and the flow of each resource, as the most output
/// per day the resource supports, in thousandths (crops on observed farmland, ore, a geode's
/// regrowth).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub pop: u32,
    pub resources: BTreeMap<ResourceId, Amount>,
}

/// A market's labour costs and what it can make.
#[derive(Clone, Debug, PartialEq)]
pub struct Costs {
    /// Worker-days per item; infinite when the market can neither make nor import the good.
    pub labour: Vec<f64>,
    /// The recipe the market makes each good with, if it makes it.
    pub recipe: Vec<Option<usize>>,
    /// Production capacity per day, thousandths, from the chosen recipe.
    pub capacity_per_day: Vec<Amount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lint {
    /// The chosen recipes' input matrix fails Hawkins-Simon: some cycle eats more than it makes.
    NotProductive { goods: Vec<GoodId> },
    /// Someone needs the good and nobody anywhere can make it, so its price is undefined.
    NoProducer(GoodId),
    /// Someone makes the good and nobody needs it or uses it: its price sits at the floor
    /// (economy research failure "prices pinned at floor").
    NoSink(GoodId),
    /// A needed good that no market can make from inputs it makes itself: supply depends on
    /// trade alone and can deadlock (research failure "deadlock").
    NoRouteWithoutTrade(GoodId),
}

impl RecipeBook {
    pub fn available(&self, r: usize, local: &Local) -> bool {
        match self.recipes[r].resource {
            Some(res) => local.resources.get(&res).is_some_and(|&f| f > 0),
            None => true,
        }
    }

    /// Output capacity per day, thousandths: the resource's flow, or for a recipe with no
    /// resource the share of the village's working days one craft may use.
    pub fn capacity_per_day(&self, r: usize, local: &Local, craft_share_milli: i64) -> Amount {
        let rec = &self.recipes[r];
        match rec.resource {
            Some(res) => local.resources.get(&res).copied().unwrap_or(0).max(0),
            None => {
                if rec.labour <= 0.0 {
                    return 0;
                }
                let workers = f64::from(local.pop) * craft_share_milli as f64 / 1000.0;
                (workers / rec.labour * rec.yield_per_batch as f64).floor() as Amount
            }
        }
    }

    /// A recipe's cost per item given input costs. An input that is the recipe's own output (seed
    /// grain) is solved in closed form, so a farm needs no outside wheat to price its wheat, and a
    /// recipe that eats at least as much of its output as it makes never has a finite cost.
    fn cost(&self, r: usize, l: &[f64]) -> f64 {
        let rec = &self.recipes[r];
        let mut c = rec.labour;
        let mut own = 0.0;
        for &(i, a) in &rec.inputs {
            if i == rec.output {
                own += a as f64;
            } else {
                c += a as f64 / MILLI as f64 * l[i.index()];
            }
        }
        let net = rec.yield_per_batch as f64 - own;
        if net <= 0.0 {
            return f64::INFINITY;
        }
        c / (net / MILLI as f64)
    }

    /// Labour costs at a market. `imported[g]` is the cost of getting `g` from elsewhere (infinite
    /// when unknown); local recipes beat it when cheaper.
    pub fn solve(
        &self,
        goods: &Goods,
        local: &Local,
        imported: &[f64],
        craft_share_milli: i64,
    ) -> Costs {
        let n = goods.len();
        assert_eq!(imported.len(), n);
        let mut l = imported.to_vec();
        let mut chosen: Vec<Option<usize>> = vec![None; n];
        let avail: Vec<usize> = (0..self.recipes.len())
            .filter(|&r| self.available(r, local) && self.recipes[r].yield_per_batch > 0)
            .collect();
        // Value iteration: costs only fall, and a productive system converges geometrically.
        for _ in 0..10_000 {
            let mut changed = false;
            for &r in &avail {
                let out = self.recipes[r].output.index();
                let c = self.cost(r, &l);
                if c.is_finite() && c < l[out] * (1.0 - 1e-12) {
                    l[out] = c;
                    chosen[out] = Some(r);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // The exact fixed point for the chosen recipes, where they form cycles.
        if let Some(exact) = self.exact(n, &chosen, &l) {
            for g in 0..n {
                if chosen[g].is_some() && exact[g].is_finite() && exact[g] > 0.0 {
                    l[g] = exact[g];
                }
            }
        }
        let capacity_per_day = (0..n)
            .map(|g| {
                chosen[g].map_or(0, |r| self.capacity_per_day(r, local, craft_share_milli))
            })
            .collect();
        Costs {
            labour: l,
            recipe: chosen,
            capacity_per_day,
        }
    }

    /// Input coefficients of the chosen recipes: `a[i][j]` items of `i` per item of `j`.
    fn matrix(&self, n: usize, chosen: &[Option<usize>]) -> Vec<Vec<f64>> {
        let mut a = vec![vec![0.0; n]; n];
        for (j, r) in chosen.iter().enumerate() {
            if let Some(r) = *r {
                let rec = &self.recipes[r];
                for &(i, amt) in &rec.inputs {
                    a[i.index()][j] += amt as f64 / rec.yield_per_batch as f64;
                }
            }
        }
        a
    }

    /// Solves `L_j = l_j + Σ_i a_ij L_i` for the chosen goods, with the others' costs fixed.
    fn exact(&self, n: usize, chosen: &[Option<usize>], l: &[f64]) -> Option<Vec<f64>> {
        let idx: Vec<usize> = (0..n).filter(|&g| chosen[g].is_some()).collect();
        if idx.is_empty() {
            return None;
        }
        let a = self.matrix(n, chosen);
        let m = idx.len();
        // (I - A^T) restricted to the chosen goods, and the right side l plus fixed inputs.
        let mut mat = vec![vec![0.0; m + 1]; m];
        for (row, &j) in idx.iter().enumerate() {
            let rec = &self.recipes[chosen[j].unwrap()];
            let mut rhs = rec.labour / (rec.yield_per_batch as f64 / MILLI as f64);
            for (col, &i) in idx.iter().enumerate() {
                mat[row][col] = if row == col { 1.0 } else { 0.0 } - a[i][j];
            }
            for i in 0..n {
                if chosen[i].is_none() && a[i][j] != 0.0 {
                    rhs += a[i][j] * l[i];
                }
            }
            mat[row][m] = rhs;
        }
        let sol = gauss(mat)?;
        let mut out = l.to_vec();
        for (k, &g) in idx.iter().enumerate() {
            out[g] = sol[k];
        }
        Some(out)
    }

    /// Hawkins-Simon: every leading principal minor of `I - A` is positive, for the recipes a
    /// market chose. Elimination without pivoting gives the minors as running products of the
    /// pivots, so a non-positive pivot names where it fails.
    pub fn hawkins_simon(&self, n: usize, chosen: &[Option<usize>]) -> Result<(), Lint> {
        let idx: Vec<usize> = (0..n).filter(|&g| chosen[g].is_some()).collect();
        let a = self.matrix(n, chosen);
        let m = idx.len();
        let mut mat: Vec<Vec<f64>> = idx
            .iter()
            .enumerate()
            .map(|(r, &i)| {
                idx.iter()
                    .enumerate()
                    .map(|(c, &j)| if r == c { 1.0 } else { 0.0 } - a[i][j])
                    .collect()
            })
            .collect();
        for k in 0..m {
            let p = mat[k][k];
            if p <= 1e-12 {
                return Err(Lint::NotProductive {
                    goods: idx[..=k].iter().map(|&g| GoodId(g as u16)).collect(),
                });
            }
            for r in k + 1..m {
                let f = mat[r][k] / p;
                for c in k..m {
                    mat[r][c] -= f * mat[k][c];
                }
            }
        }
        Ok(())
    }

    /// Whether `local` can make `g` from inputs it makes itself, recursively.
    pub fn self_sufficient(&self, g: GoodId, local: &Local) -> bool {
        let mut memo = BTreeMap::new();
        self.self_sufficient_inner(g, local, &mut memo)
    }

    fn self_sufficient_inner(
        &self,
        g: GoodId,
        local: &Local,
        memo: &mut BTreeMap<GoodId, bool>,
    ) -> bool {
        if let Some(&v) = memo.get(&g) {
            return v;
        }
        // Assume not while exploring, so a cycle with no outside entry does not count.
        memo.insert(g, false);
        let ok = (0..self.recipes.len()).any(|r| {
            let rec = &self.recipes[r];
            rec.output == g
                && self.available(r, local)
                && rec
                    .inputs
                    .iter()
                    .all(|&(i, _)| i == g || self.self_sufficient_inner(i, local, memo))
        });
        memo.insert(g, ok);
        ok
    }
}

/// Gaussian elimination with partial pivoting on an augmented matrix.
fn gauss(mut m: Vec<Vec<f64>>) -> Option<Vec<f64>> {
    let n = m.len();
    for k in 0..n {
        let piv = (k..n).max_by(|&a, &b| m[a][k].abs().total_cmp(&m[b][k].abs()))?;
        if m[piv][k].abs() < 1e-15 {
            return None;
        }
        m.swap(k, piv);
        for r in 0..n {
            if r != k {
                let f = m[r][k] / m[k][k];
                if f != 0.0 {
                    for c in k..=n {
                        m[r][c] -= f * m[k][c];
                    }
                }
            }
        }
    }
    Some((0..n).map(|k| m[k][n] / m[k][k]).collect())
}

/// The world-level lints of the economy research's failure table: who needs what, who can make
/// it, and whether any market can make each needed good on its own.
pub fn lint_world(goods: &Goods, book: &RecipeBook, locals: &[&Local], needs: &[bool]) -> Vec<Lint> {
    let mut out = Vec::new();
    let n = goods.len();
    let mut used_as_input = vec![false; n];
    for r in &book.recipes {
        for &(i, _) in &r.inputs {
            used_as_input[i.index()] = true;
        }
    }
    for g in goods.ids() {
        let i = g.index();
        let made_somewhere = locals.iter().any(|l| {
            (0..book.recipes.len()).any(|r| book.recipes[r].output == g && book.available(r, l))
        });
        if needs[i] && !made_somewhere {
            out.push(Lint::NoProducer(g));
        }
        if made_somewhere && !needs[i] && !used_as_input[i] && !goods.is_currency(g) {
            out.push(Lint::NoSink(g));
        }
        if needs[i] && made_somewhere && !locals.iter().any(|l| book.self_sufficient(g, l)) {
            out.push(Lint::NoRouteWithoutTrade(g));
        }
    }
    for l in locals {
        let costs = book.solve(goods, l, &vec![f64::INFINITY; n], 100);
        if let Err(e) = book.hawkins_simon(n, &costs.recipe)
            && !out.contains(&e)
        {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goods::{GoodDef, GoodKind};

    fn goods() -> (Goods, GoodId, GoodId, GoodId) {
        let mut g = Goods::new();
        let def = |name: &str| GoodDef {
            name: name.into(),
            kind: GoodKind::Item,
            need_per_pop_day: 0,
            stack: 64,
        };
        let wheat = g.add(def("wheat"));
        let bread = g.add(def("bread"));
        let iron = g.add(def("iron"));
        (g, wheat, bread, iron)
    }

    #[test]
    fn costs_follow_recipes_and_resources() {
        let (g, wheat, bread, iron) = goods();
        let farmland = ResourceId(0);
        let ore = ResourceId(1);
        let book = RecipeBook {
            recipes: vec![
                // A batch of 10 wheat takes 1 worker-day and 1 wheat as seed: a cycle.
                Recipe {
                    name: "farm".into(),
                    output: wheat,
                    yield_per_batch: 10 * MILLI,
                    inputs: vec![(wheat, MILLI)],
                    labour: 1.0,
                    resource: Some(farmland),
                },
                Recipe {
                    name: "bake".into(),
                    output: bread,
                    yield_per_batch: MILLI,
                    inputs: vec![(wheat, 3 * MILLI)],
                    labour: 0.1,
                    resource: None,
                },
                Recipe {
                    name: "mine".into(),
                    output: iron,
                    yield_per_batch: MILLI,
                    inputs: vec![],
                    labour: 2.0,
                    resource: Some(ore),
                },
            ],
        };
        let mut farm = Local {
            pop: 20,
            ..Local::default()
        };
        farm.resources.insert(farmland, 100 * MILLI);
        let c = book.solve(&g, &farm, &[f64::INFINITY; 3], 100);
        // L_wheat = (1 + 0.1 L_wheat) / 1 per 10 → L = 1 / (10 - 1) = 1/9.
        assert!((c.labour[wheat.index()] - 1.0 / 9.0).abs() < 1e-12);
        assert!((c.labour[bread.index()] - (0.1 + 3.0 / 9.0)).abs() < 1e-12);
        assert!(c.labour[iron.index()].is_infinite());
        assert_eq!(c.capacity_per_day[wheat.index()], 100 * MILLI);
        // 20 × 0.1 workers / 0.1 worker-days per loaf = 20 loaves a day.
        assert_eq!(c.capacity_per_day[bread.index()], 20 * MILLI);
        assert!(book.hawkins_simon(3, &c.recipe).is_ok());
        // Importing iron at 5 worker-days makes it known.
        let c2 = book.solve(&g, &farm, &[f64::INFINITY, f64::INFINITY, 5.0], 100);
        assert_eq!(c2.labour[iron.index()], 5.0);
        assert_eq!(c2.recipe[iron.index()], None);
    }

    #[test]
    fn an_unproductive_cycle_is_linted() {
        let (g, wheat, _, _) = goods();
        let book = RecipeBook {
            recipes: vec![Recipe {
                name: "magic".into(),
                output: wheat,
                yield_per_batch: MILLI,
                // Eats two to make one.
                inputs: vec![(wheat, 2 * MILLI)],
                labour: 1.0,
                resource: None,
            }],
        };
        // Forced choice, to show the lint itself.
        assert!(matches!(
            book.hawkins_simon(g.len(), &[Some(0), None, None]),
            Err(Lint::NotProductive { .. })
        ));
        // Value iteration never chooses it: its cost is never finite from infinity.
        let c = book.solve(
            &g,
            &Local {
                pop: 10,
                ..Local::default()
            },
            &[f64::INFINITY; 3],
            100,
        );
        assert_eq!(c.recipe[wheat.index()], None);
    }

    #[test]
    fn world_lints() {
        let (g, wheat, bread, iron) = goods();
        let book = RecipeBook {
            recipes: vec![Recipe {
                name: "bake".into(),
                output: bread,
                yield_per_batch: MILLI,
                inputs: vec![(wheat, MILLI)],
                labour: 1.0,
                resource: None,
            }],
        };
        let l = Local {
            pop: 5,
            ..Local::default()
        };
        let lints = lint_world(&g, &book, &[&l], &[false, true, true]);
        assert!(lints.contains(&Lint::NoProducer(iron)));
        assert!(lints.contains(&Lint::NoRouteWithoutTrade(bread)));
    }
}
