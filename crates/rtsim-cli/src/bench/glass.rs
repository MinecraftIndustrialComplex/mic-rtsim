// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box's `glass_nodes` sweep (GLASSBOX.md §9.3): compile plus first solve CPU against
//! nodes per district, declared slope 1 with the rounds fixed. Run by `rtsim-cli bench industry`.
//!
//! The synthetic factory repeats one cell of ten nodes and eleven edges: an input store of zinc
//! and copper, a funnel, a brass mixer (two inputs), a belt, a tree of two brass tunnels (split,
//! then prefer nearest) over three presses, and an output store; the mixer and presses are one
//! kinetic domain per cell. The records go through `Engine::ingest` after the district, its
//! inventories and one sweep, as Java sends them, so the time is the store's assembly, the graph
//! build and the first solve (L, the classes and the LP U).

use rtsim_industry::engine::Engine;
use rtsim_industry::glass::records::*;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{DistrictDecl, InvKind, InventoryDecl, Obs, SweepEnd, Timed};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use serde::Serialize;

use crate::sys;

const ZINC: Good = Good(11);
const COPPER: Good = Good(12);
const BRASS: Good = Good(13);
const SHEET: Good = Good(14);
const D: DistrictId = DistrictId(1);
/// Nodes per cell.
pub const CELL: u64 = 10;

/// One point of the sweep.
#[derive(Clone, Debug, Serialize)]
pub struct GlassPoint {
    pub nodes: u64,
    pub edges: u64,
    /// CPU for every record of the compile through `Engine::ingest` (assembly, build, first solve).
    pub cpu_ns: f64,
    /// CPU of the solve alone (`glass::solve::solve` on the built graph, same regime).
    pub solve_cpu_ns: f64,
    pub rounds: u32,
    pub converged: bool,
    pub lp_solves: u32,
    pub lp_skipped: u32,
    pub lp_tight: u32,
    /// The first solve's L for sheets, per tick (a sanity check: cells × the cell's rate).
    pub sheets_l: f64,
    /// The first save after the compile, the save before it made before the compile's records
    /// (`glass_replay::save_cost`, docs/choices-industry-dirty-pages.md): the bytes of the pages
    /// it wrote and its CPU.
    pub save_bytes: u64,
    pub save_cpu_ns: f64,
    pub save_pages: u64,
    /// The save after that, with nothing changed between: the globals page alone.
    pub next_save_bytes: u64,
    pub next_save_cpu_ns: f64,
    pub next_save_pages: u64,
}

fn head(index: u32) -> GraphHead {
    GraphHead {
        district: D,
        compile: 1,
        index,
    }
}

fn entry(recipe: Option<u32>, batch: u32, ticks: u32, flags: u32) -> RateEntry {
    RateEntry {
        level: 0,
        recipe,
        batch,
        ticks_num: ticks,
        ticks_den: 1,
        flags,
    }
}

/// The compile's records for `cells` cells, END last. With `slack`, both tunnels force their
/// split and the slow press takes 64 ticks: L runs at 2 × 2 × 1/64 = 1/16 a cell while U (routing
/// free) runs at the mixer's 2/17, so the LP decides U.
pub fn factory(cells: u32, slack: bool) -> Vec<GlassObs> {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut domains: Vec<GraphDomain> = Vec::new();
    let term = |good, num| Term {
        good,
        flags: 0,
        num,
        den: 1,
    };
    let recipes = vec![
        GraphRecipe {
            head: head(0),
            recipe_type: recipe_type::MIXING,
            heat: heat::NONE,
            inputs: vec![term(ZINC, 1), term(COPPER, 1)],
            outputs: vec![term(BRASS, 2)],
            duration: 17,
            name: 0,
        },
        GraphRecipe {
            head: head(1),
            recipe_type: recipe_type::PRESSING,
            heat: heat::NONE,
            inputs: vec![term(BRASS, 1)],
            outputs: vec![term(SHEET, 1)],
            duration: 10,
            name: 0,
        },
    ];
    let node = |nodes: &mut Vec<GraphNode>,
                kind: NodeKind,
                model: u32,
                flags: u32,
                rates: Vec<RateEntry>|
     -> u32 {
        let i = nodes.len() as u32;
        nodes.push(GraphNode {
            head: head(i),
            kind,
            model,
            domain: None,
            inventory: None,
            flags,
            pos: BlockPos::default(),
            rule: rule::NONE,
            capacity: 0,
            params: [0; 4],
            rates,
        });
        i
    };
    let edge = |edges: &mut Vec<GraphEdge>, kind: EdgeKind, from: u32, to: u32, order: u32| {
        let i = edges.len() as u32;
        edges.push(GraphEdge {
            head: head(i),
            kind,
            from,
            to,
            order,
            flags: 0,
            capacity: None,
            latency: if kind == EdgeKind::Belt { 20 } else { 0 },
            param: 0,
            filter: Vec::new(),
        });
    };
    let blocks = node_flags::ANY_OUTPUT_BLOCKS;
    for c in 0..cells {
        let d = c;
        let s_in = node(
            &mut nodes,
            NodeKind::Store,
            model::STORE_ITEMS,
            0,
            Vec::new(),
        );
        nodes[s_in as usize].inventory = Some(2 * c);
        let funnel = node(
            &mut nodes,
            NodeKind::Mover,
            model::FUNNEL_BRASS,
            0,
            vec![entry(None, 16, 8, rate_flags::SHARED)],
        );
        let mixer = node(
            &mut nodes,
            NodeKind::Machine,
            model::MIXER,
            blocks,
            vec![entry(Some(0), 1, 17, 0)],
        );
        let belt = node(
            &mut nodes,
            NodeKind::Mover,
            model::BELT,
            0,
            vec![entry(None, 1, 4, rate_flags::SHARED)],
        );
        let j1 = node(
            &mut nodes,
            NodeKind::Junction,
            model::TUNNEL_BRASS,
            0,
            Vec::new(),
        );
        nodes[j1 as usize].rule = if slack {
            rule::FORCED_SPLIT
        } else {
            rule::SPLIT
        };
        let j2 = node(
            &mut nodes,
            NodeKind::Junction,
            model::TUNNEL_BRASS,
            0,
            Vec::new(),
        );
        nodes[j2 as usize].rule = if slack {
            rule::FORCED_SPLIT
        } else {
            rule::PREFER_NEAREST
        };
        let slow = if slack { 64 } else { 32 };
        let presses: Vec<u32> = [10, slow, 20]
            .into_iter()
            .map(|t| {
                node(
                    &mut nodes,
                    NodeKind::Machine,
                    model::PRESS,
                    blocks,
                    vec![entry(Some(1), 1, t, 0)],
                )
            })
            .collect();
        let s_out = node(
            &mut nodes,
            NodeKind::Store,
            model::STORE_ITEMS,
            0,
            Vec::new(),
        );
        nodes[s_out as usize].inventory = Some(2 * c + 1);
        for v in [mixer, presses[0], presses[1], presses[2]] {
            nodes[v as usize].domain = Some(d);
        }
        edge(&mut edges, EdgeKind::Adjacency, s_in, funnel, 0);
        edge(&mut edges, EdgeKind::Adjacency, funnel, mixer, 0);
        edge(&mut edges, EdgeKind::Adjacency, mixer, belt, 0);
        edge(&mut edges, EdgeKind::Belt, belt, j1, 0);
        edge(&mut edges, EdgeKind::Adjacency, j1, presses[0], 0);
        edge(&mut edges, EdgeKind::Belt, j1, j2, 1);
        edge(&mut edges, EdgeKind::Adjacency, j2, presses[1], 0);
        edge(&mut edges, EdgeKind::Adjacency, j2, presses[2], 1);
        for &p in &presses {
            edge(&mut edges, EdgeKind::Adjacency, p, s_out, 0);
        }
        let member = |node: u32, impact: f64| Member {
            node: Some(node),
            count: 1,
            ratio_num: 1,
            ratio_den: 1,
            impact,
        };
        domains.push(GraphDomain {
            head: head(d),
            network: u64::from(d) + 1,
            current: Some(0),
            flags: 0,
            live_stress: 0.0,
            live_capacity: 0.0,
            levels: vec![DomainLevel::speed_row(32)],
            sources: vec![DomainSource::fixed(i64::from(d), 32, 4096.0)],
            foreign: 0,
            members: vec![
                member(mixer, 4.0),
                member(presses[0], 8.0),
                member(presses[1], 8.0),
                member(presses[2], 8.0),
            ],
        });
    }
    let (n, e, r, d) = (
        nodes.len() as u32,
        edges.len() as u32,
        recipes.len() as u32,
        domains.len() as u32,
    );
    let mut out: Vec<GlassObs> = Vec::with_capacity((n + e + r + d + 1) as usize);
    out.extend(nodes.into_iter().map(GlassObs::Node));
    out.extend(edges.into_iter().map(GlassObs::Edge));
    out.extend(recipes.into_iter().map(GlassObs::Recipe));
    out.extend(domains.into_iter().map(GlassObs::Domain));
    out.push(GlassObs::End(GraphEnd {
        district: D,
        compile: 1,
        flags: end_flags::COMPLETE,
        window: 0,
        records: n + e + r + d,
        nodes: n,
        edges: e,
        recipes: r,
        domains: d,
        reason: 0,
        fingerprint: 0,
        frame: 0,
        revision: 1,
        compile_ticks: 1,
    }));
    out
}

/// An engine that knows the district, its inventories and one sweep of them.
fn engine(cells: u32) -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    for (g, name) in [
        (ZINC, "zinc"),
        (COPPER, "copper"),
        (BRASS, "brass"),
        (SHEET, "sheet"),
    ] {
        e.define_good(g, GoodInfo::item(name, 64));
    }
    let at = Tick(0);
    let send = |e: &mut Engine, obs: Obs| e.ingest(&Timed { at, obs });
    send(
        &mut e,
        Obs::District(DistrictDecl {
            district: D,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: 2 * cells,
        }),
    );
    for index in 0..2 * cells {
        send(
            &mut e,
            Obs::Inventory(InventoryDecl {
                district: D,
                revision: 1,
                index,
                pos: BlockPos::new(index as i32 * 2, 64, 0),
                kind: InvKind(InvKind::ITEMS),
                item_slots: 1_000,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
        let goods = if index % 2 == 0 {
            GoodVec::from_pairs([(ZINC, 10_000), (COPPER, 10_000)])
        } else {
            GoodVec::new()
        };
        send(
            &mut e,
            Obs::StockSample {
                district: D,
                inventory: index,
                sweep: 1,
                goods,
            },
        );
    }
    send(
        &mut e,
        Obs::SweepEnd(SweepEnd {
            district: D,
            sweep: 1,
            records: 2 * cells,
            inventories: 2 * cells,
            live: false,
            creative: false,
            overstressed: false,
        }),
    );
    e
}

/// Compile and first solve of a factory of about `nodes` nodes.
pub fn point(nodes: u64, slack: bool) -> GlassPoint {
    let cells = (nodes / CELL).max(1) as u32;
    let records = factory(cells, slack);
    let mut e = engine(cells);
    // The pages before the compile: what the save before it wrote.
    e.save_changed();
    let timed: Vec<Timed> = records
        .into_iter()
        .map(|g| Timed {
            at: Tick(0),
            obs: Obs::Glass(g),
        })
        .collect();
    let c0 = sys::thread_cpu_ns().unwrap_or(0);
    for t in &timed {
        e.ingest(t);
    }
    let c1 = sys::thread_cpu_ns().unwrap_or(0);
    let r = e.glass.report(D).expect("the compile is accepted");
    let dg = &e.glass.districts[&D];
    let c = dg.compiled.as_ref().expect("compiled");
    // The solve alone, in the regime the first solve read.
    let s0 = sys::thread_cpu_ns().unwrap_or(0);
    let sol = rtsim_industry::glass::solve::solve(&c.graph, &c.regime, &Tunables::default().glass);
    let s1 = sys::thread_cpu_ns().unwrap_or(0);
    assert_eq!(sol.rounds, r.rounds);
    let sheets_l = r
        .goods
        .iter()
        .find(|g| g.good == SHEET.0)
        .map_or(0.0, |g| g.in_l);
    let save = super::glass_replay::save_cost(&mut e);
    let next = super::glass_replay::save_cost(&mut e);
    GlassPoint {
        nodes: u64::from(r.nodes),
        edges: u64::from(r.edges),
        cpu_ns: (c1 - c0) as f64,
        solve_cpu_ns: (s1 - s0) as f64,
        rounds: r.rounds,
        converged: r.converged,
        lp_solves: sol.lp_solves,
        lp_skipped: sol.lp_skipped,
        lp_tight: sol.lp_tight,
        sheets_l,
        save_bytes: save.bytes,
        save_cpu_ns: save.cpu_ns,
        save_pages: save.pages,
        next_save_bytes: next.bytes,
        next_save_cpu_ns: next.cpu_ns,
        next_save_pages: next.pages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_synthetic_factory_compiles_and_every_cell_makes_sheets() {
        let p = point(100, false);
        assert_eq!(p.nodes, 100);
        assert_eq!(p.edges, 110);
        assert!(p.converged);
        // Each cell: the mixer's 2/17 brass per tick, the belt's 1/4 aside, all pressed (the
        // presses take 1/10 + 1/32 + 1/20).
        assert!(
            (p.sheets_l - 10.0 * 2.0 / 17.0).abs() < 1e-9,
            "{}",
            p.sheets_l
        );
        // No routing slack, but the routing-free linear bound overcounts the tunnel tree for
        // sheets (29/160 a cell): that one LP runs; the inputs' linear bounds meet L.
        assert_eq!((p.lp_solves, p.lp_tight), (1, 5));
        // The forced splits: 1/16 per cell, and the LPs run.
        let p = point(100, true);
        assert!((p.sheets_l - 10.0 / 16.0).abs() < 1e-9, "{}", p.sheets_l);
        assert!(p.lp_solves > 0);
    }

    /// `cargo test --release -p rtsim-cli glass_table -- --ignored --nocapture`: the sweep alone.
    #[test]
    #[ignore]
    fn glass_table() {
        for slack in [false, true] {
            for n in [10, 100, 1_000, 10_000] {
                let p = point(n, slack);
                println!(
                    "slack {slack} nodes {} cpu {:.0} us solve {:.0} us rounds {} lp {}/{}/{}",
                    p.nodes,
                    p.cpu_ns / 1e3,
                    p.solve_cpu_ns / 1e3,
                    p.rounds,
                    p.lp_solves,
                    p.lp_skipped,
                    p.lp_tight
                );
            }
        }
    }
}
