// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box's tests: GLASSBOX.md's worked examples, the solver's rules (docs/industry-boundary.md
//! §9.3), the checksum (§5.3) and the engine path.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use rtsim_kernel::clock::Tick;

use super::check::{Compiled, DistrictGlass, GlassEvent, POISSON_SLOPE_FACTOR, View};
use super::graph::Graph;
use super::q::Q;
use super::records::*;
use super::report::{class, verdict, why};
use super::solve::{self, GoodBound, Regime, Solution};
use super::{Glass, GlassStats};
use crate::district::Inventory;
use crate::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec, VecMap};
use crate::obs::{InvKind, InventoryDecl};
use crate::tunables::GlassTunables;

const D: DistrictId = DistrictId(7);
const ZINC: Good = Good(1);
const COPPER: Good = Good(2);
const BRASS: Good = Good(3);
const SHEET: Good = Good(4);
const COAL: Good = Good(5);
const WATER: Good = Good(6);
const WOOL: Good = Good(7);
const ORE: Good = Good(8);
const CRUSHED: Good = Good(9);
const XP: Good = Good(10);
const MARKER: Good = Good(11);

/// Ticks per minute: the worked examples are per minute.
const MIN: f64 = 1200.0;

fn goods() -> BTreeMap<Good, GoodInfo> {
    let mut m = BTreeMap::new();
    for (g, name) in [
        (ZINC, "zinc"),
        (COPPER, "copper"),
        (BRASS, "brass"),
        (SHEET, "sheet"),
        (COAL, "coal"),
        (WOOL, "wool"),
        (ORE, "ore"),
        (CRUSHED, "crushed"),
        (XP, "xp"),
    ] {
        m.insert(g, GoodInfo::item(name, 64));
    }
    // Small stacks: a controller's observed store holds its markers at every level the tests use.
    m.insert(MARKER, GoodInfo::item("marker", 16));
    m.insert(WATER, GoodInfo::fluid("water"));
    m
}

/// A graph builder.
#[derive(Default, Clone)]
struct B {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    recipes: Vec<GraphRecipe>,
    domains: Vec<GraphDomain>,
    window: u32,
}

fn head(index: u32) -> GraphHead {
    GraphHead {
        district: D,
        compile: 1,
        index,
    }
}

fn entry(recipe: Option<u32>, batch: u32, ticks: u32) -> RateEntry {
    RateEntry {
        level: 0,
        recipe,
        batch,
        ticks_num: ticks,
        ticks_den: 1,
        flags: 0,
    }
}

impl B {
    fn node(&mut self, kind: NodeKind, model: u32, flags: u32, rates: Vec<RateEntry>) -> u32 {
        let i = self.nodes.len() as u32;
        self.nodes.push(GraphNode {
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
    }

    fn store(&mut self, inv: u32) -> u32 {
        let v = self.node(NodeKind::Store, model::STORE_ITEMS, 0, Vec::new());
        self.nodes[v as usize].inventory = Some(inv);
        v
    }

    fn recipe(&mut self, ins: &[(Good, i64)], outs: &[(Good, i64)]) -> u32 {
        let i = self.recipes.len() as u32;
        let term = |&(good, num): &(Good, i64)| Term {
            good,
            flags: 0,
            num,
            den: 1,
        };
        self.recipes.push(GraphRecipe {
            head: head(i),
            recipe_type: recipe_type::OTHER,
            heat: heat::NONE,
            inputs: ins.iter().map(term).collect(),
            outputs: outs.iter().map(term).collect(),
            duration: 0,
            name: 0,
        });
        i
    }

    fn machine(&mut self, model: u32, recipe: u32, batch: u32, ticks: u32) -> u32 {
        self.node(
            NodeKind::Machine,
            model,
            node_flags::ANY_OUTPUT_BLOCKS,
            vec![entry(Some(recipe), batch, ticks)],
        )
    }

    fn source(&mut self, recipe: u32, batch: u32, ticks: u32) -> u32 {
        self.node(
            NodeKind::Source,
            model::UNKNOWN,
            0,
            vec![entry(Some(recipe), batch, ticks)],
        )
    }

    fn junction(&mut self, r: u32) -> u32 {
        let v = self.node(NodeKind::Junction, model::TUNNEL_BRASS, 0, Vec::new());
        self.nodes[v as usize].rule = r;
        v
    }

    fn edge_k(&mut self, kind: EdgeKind, from: u32, to: u32, filter: &[Good]) -> u32 {
        let i = self.edges.len() as u32;
        let order = self.edges.iter().filter(|e| e.from == from).count() as u32;
        self.edges.push(GraphEdge {
            head: head(i),
            kind,
            from,
            to,
            order,
            flags: 0,
            capacity: None,
            latency: 0,
            param: 0,
            filter: filter.to_vec(),
        });
        i
    }

    fn edge(&mut self, from: u32, to: u32) -> u32 {
        self.edge_k(EdgeKind::Adjacency, from, to, &[])
    }

    fn end(&self, compile: u32) -> GraphEnd {
        let (n, e, r, d) = (
            self.nodes.len() as u32,
            self.edges.len() as u32,
            self.recipes.len() as u32,
            self.domains.len() as u32,
        );
        GraphEnd {
            district: D,
            compile,
            flags: end_flags::COMPLETE,
            window: self.window,
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
        }
    }

    fn graph(&self) -> Graph {
        Graph::build(
            self.end(1),
            self.nodes.clone(),
            self.edges.clone(),
            self.recipes.clone(),
            self.domains.clone(),
        )
        .expect("valid")
    }

    /// The compile's records, END last.
    fn records(&self, compile: u32) -> Vec<GlassObs> {
        let mut out = Vec::new();
        for n in &self.nodes {
            let mut n = n.clone();
            n.head.compile = compile;
            out.push(GlassObs::Node(n));
        }
        for e in &self.edges {
            let mut e = e.clone();
            e.head.compile = compile;
            out.push(GlassObs::Edge(e));
        }
        for r in &self.recipes {
            let mut r = r.clone();
            r.head.compile = compile;
            out.push(GlassObs::Recipe(r));
        }
        for d in &self.domains {
            let mut d = d.clone();
            d.head.compile = compile;
            out.push(GlassObs::Domain(d));
        }
        out.push(GlassObs::End(self.end(compile)));
        out
    }
}

/// Every store read, with room; `held` lists what each store holds.
fn regime(g: &Graph, held: &[(u32, &[Good])]) -> Regime {
    let mut rg = Regime::new(g.nodes.len());
    for (v, n) in g.nodes.iter().enumerate() {
        rg.read[v] = n.kind == NodeKind::Store && n.inventory.is_some();
    }
    for &(v, gs) in held {
        let mut gs = gs.to_vec();
        gs.sort_unstable();
        rg.held[v as usize] = gs;
    }
    rg
}

fn run(g: &Graph, rg: &Regime) -> Solution {
    solve::solve(g, rg, &GlassTunables::default())
}

fn bound(s: &Solution, g: Good) -> &GoodBound {
    s.goods
        .iter()
        .find(|b| b.good == g)
        .unwrap_or_else(|| panic!("good {g:?} not on the stores: {:?}", s.goods))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

// ---- the worked examples (GLASSBOX.md §3.3) ------------------------------------------------------

/// A mixer making 2 brass per 17 ticks (2400/17 ≈ 141.18 per minute) feeding a brass tunnel over
/// press A (1 per 10 ticks, 120 per minute) and press B (1 per 32 ticks, 37.5 per minute).
struct Line {
    g: Graph,
    rg: Regime,
    to_a: u32,
    to_b: u32,
}

fn brass_line(r: u32) -> Line {
    let mut b = B::default();
    let input = b.store(0);
    let output = b.store(1);
    let mix = b.recipe(&[(ZINC, 1), (COPPER, 1)], &[(BRASS, 2)]);
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let mixer = b.machine(model::MIXER, mix, 1, 17);
    let j = b.junction(r);
    let pa = b.machine(model::PRESS, press, 1, 10);
    let pb = b.machine(model::PRESS, press, 1, 32);
    b.edge(input, mixer);
    b.edge(mixer, j);
    let to_a = b.edge(j, pa);
    let to_b = b.edge(j, pb);
    b.edge(pa, output);
    b.edge(pb, output);
    let g = b.graph();
    let rg = regime(&g, &[(input, &[ZINC, COPPER])]);
    Line { g, rg, to_a, to_b }
}

#[test]
fn forced_split_runs_at_twice_the_slow_press_while_u_is_the_mixer() {
    let l = brass_line(rule::FORCED_SPLIT);
    let s = run(&l.g, &l.rg);
    let sheet = bound(&s, SHEET);
    assert_eq!(sheet.in_l, Q::new(1, 16));
    assert!(close(sheet.in_l.to_f64() * MIN, 75.0));
    // U: routing free, the mixer limits (the presses could take 1/10 + 1/32).
    let u = sheet.in_u.to_f64();
    assert!(u >= 2.0 / 17.0, "U {u} below the optimum");
    assert!(u <= 2.0 / 17.0 * (1.0 + 1e-5), "U {u} too loose");
    assert!(close(u * MIN, 2400.0 / 17.0) || (u * MIN - 2400.0 / 17.0).abs() < 1e-3);
    assert_eq!(sheet.class, class::EXACT);
    // The LP ran and found the mixer's rate.
    let lp = sheet.in_u_lp.expect("the LP ran");
    assert!((lp - 2.0 / 17.0).abs() < 1e-9, "LP {lp}");
    // The mixer runs at the forced split's pace: 1/32 crafts per tick.
    assert_eq!(bound(&s, ZINC).out_l, Q::new(1, 32));
    assert!(s.converged);
}

#[test]
fn prefer_nearest_fills_the_near_press_first() {
    let l = brass_line(rule::PREFER_NEAREST);
    let s = run(&l.g, &l.rg);
    let a = s.edge_flow[l.to_a as usize];
    let b = s.edge_flow[l.to_b as usize];
    assert!(close(a * MIN, 120.0), "a {}", a * MIN);
    assert!(close(b, 3.0 / 170.0), "b {}", b * MIN);
    assert!((b * MIN - 21.18).abs() < 0.01);
    assert_eq!(bound(&s, SHEET).in_l, Q::new(2, 17));
}

#[test]
fn split_shares_evenly_and_the_slow_press_caps_its_share() {
    let l = brass_line(rule::SPLIT);
    let s = run(&l.g, &l.rg);
    let a = s.edge_flow[l.to_a as usize];
    let b = s.edge_flow[l.to_b as usize];
    assert!(close(b * MIN, 37.5));
    assert!(close(a, 2.0 / 17.0 - 1.0 / 32.0));
    assert_eq!(bound(&s, SHEET).in_l, Q::new(2, 17));
    assert_eq!(bound(&s, SHEET).class, class::EXACT);
}

#[test]
fn a_randomized_tunnel_is_bounded_between_l_and_u() {
    let l = brass_line(rule::RANDOMIZE);
    let s = run(&l.g, &l.rg);
    let sheet = bound(&s, SHEET);
    assert_eq!(sheet.class, class::BOUNDED);
    assert!(sheet.in_u >= sheet.in_l);
}

// ---- coupling (§3.2 step 4) ----------------------------------------------------------------------

#[test]
fn a_mixer_short_of_zinc_backs_its_copper_up_and_the_rounds_converge() {
    let mut b = B::default();
    let out = b.store(0);
    let rz = b.recipe(&[], &[(ZINC, 1)]);
    let rc = b.recipe(&[], &[(COPPER, 1)]);
    let mix = b.recipe(&[(ZINC, 1), (COPPER, 1)], &[(BRASS, 2)]);
    let zinc = b.source(rz, 1, 20);
    let copper = b.source(rc, 1, 5);
    let mixer = b.machine(model::MIXER, mix, 1, 17);
    b.edge(zinc, mixer);
    let ec = b.edge(copper, mixer);
    b.edge(mixer, out);
    let g = b.graph();
    let s = run(&g, &regime(&g, &[]));
    assert_eq!(bound(&s, BRASS).in_l, Q::new(1, 10));
    // Copper backs up to the zinc's pace.
    assert!(close(s.edge_flow[ec as usize], 1.0 / 20.0));
    assert!(s.converged);
    assert!(s.rounds >= 2, "rounds {}", s.rounds);
    assert!(s.rounds <= 2 * g.multi_input + 2);
}

// ---- machines ------------------------------------------------------------------------------------

fn crusher(flags: u32) -> (Graph, Regime) {
    let mut b = B::default();
    let input = b.store(0);
    let a = b.store(1);
    let full = b.store(2);
    let r = b.recipe(&[(ORE, 1)], &[(CRUSHED, 1), (XP, 1)]);
    let m = b.machine(model::CRUSHER, r, 1, 10);
    b.nodes[m as usize].flags = flags;
    b.edge(input, m);
    b.edge_k(EdgeKind::Adjacency, m, a, &[CRUSHED]);
    b.edge_k(EdgeKind::Adjacency, m, full, &[XP]);
    let g = b.graph();
    let mut rg = regime(&g, &[(input, &[ORE])]);
    rg.room[full as usize] = false;
    (g, rg)
}

#[test]
fn any_output_blocks_stops_the_machine_when_one_output_is_full() {
    let (g, rg) = crusher(node_flags::ANY_OUTPUT_BLOCKS);
    let s = run(&g, &rg);
    assert_eq!(bound(&s, CRUSHED).in_l, Q::ZERO);
    assert_eq!(bound(&s, ORE).out_l, Q::ZERO);
    // Without the flag the blocked output is lost and the rest runs.
    let (g, rg) = crusher(0);
    let s = run(&g, &rg);
    assert_eq!(bound(&s, CRUSHED).in_l, Q::new(1, 10));
}

/// A heated mixer; `fuel_ticks` is the fuel edge's ticks per coal (None: no heater).
fn heated_mixer(fuel_ticks: Option<u32>) -> (Graph, Regime) {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let mix = b.recipe(&[(ZINC, 1), (COPPER, 1)], &[(BRASS, 2)]);
    b.recipes[mix as usize].heat = heat::NEEDS_HEATED;
    let mixer = b.machine(model::MIXER, mix, 1, 17);
    b.edge_k(EdgeKind::Adjacency, input, mixer, &[ZINC, COPPER]);
    b.edge(mixer, out);
    let mut held: Vec<Good> = vec![ZINC, COPPER];
    if let Some(t) = fuel_ticks {
        let h = b.node(NodeKind::Heater, model::BLAZE_BURNER, 0, Vec::new());
        b.nodes[h as usize].params = [0, i64::from(heat::KINDLED), 1600, 0];
        let e = b.edge_k(EdgeKind::Adjacency, input, h, &[COAL]);
        b.edges[e as usize].capacity = Some((1, t));
        b.edge_k(EdgeKind::Heat, h, mixer, &[]);
        held.push(COAL);
    }
    let g = b.graph();
    let rg = regime(&g, &[(input, &held)]);
    (g, rg)
}

#[test]
fn a_heater_short_of_fuel_runs_its_recipe_at_its_duty() {
    // One coal per 3,200 ticks burning 1,600: duty 1/2.
    let (g, rg) = heated_mixer(Some(3200));
    let s = run(&g, &rg);
    assert_eq!(s.duty[g.nodes.len() - 1], Q::new(1, 2));
    assert_eq!(bound(&s, BRASS).in_l, Q::new(1, 17));
    assert_eq!(bound(&s, COAL).out_l, Q::new(1, 3200));
    assert!(bound(&s, BRASS).in_u >= Q::new(1, 17));
    // Fed fully, the heater is lit all the time.
    let (g, rg) = heated_mixer(Some(800));
    let s = run(&g, &rg);
    assert_eq!(bound(&s, BRASS).in_l, Q::new(2, 17));
    assert_eq!(bound(&s, COAL).out_l, Q::new(1, 1600));
}

#[test]
fn a_heated_recipe_without_heat_does_not_run() {
    let (g, rg) = heated_mixer(None);
    let s = run(&g, &rg);
    assert_eq!(bound(&s, BRASS).in_l, Q::ZERO);
}

#[test]
fn a_press_on_a_belt_passes_what_it_does_not_press() {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let belt = b.node(
        NodeKind::Mover,
        model::BELT,
        0,
        vec![RateEntry {
            flags: rate_flags::SHARED,
            ..entry(None, 1, 4)
        }],
    );
    let p = b.machine(model::PRESS, press, 1, 10);
    b.nodes[p as usize].flags |= node_flags::PASS_THROUGH;
    b.edge(input, belt);
    b.edge(belt, p);
    b.edge(p, out);
    let g = b.graph();
    let s = run(&g, &regime(&g, &[(input, &[BRASS, ZINC])]));
    let sheet = bound(&s, SHEET).in_l;
    let zinc = bound(&s, ZINC).in_l;
    assert!(sheet.is_pos() && sheet <= Q::new(1, 10));
    assert!(zinc.is_pos(), "zinc passes the press");
    // The belt's 1/4 per tick is shared by both.
    assert!(sheet.add(zinc) <= Q::new(1, 4));
}

// ---- boilers and kinetics (§2.5, §9.3) -----------------------------------------------------------

/// A boiler fed by a pump of `pump` mB per tick drives a domain whose press makes sheets: level 4
/// selects the fast level, level 2 the slow one, which is overstressed.
fn steam(pump: u32, flags: u32) -> (Graph, Regime, u32) {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let water = b.node(
        NodeKind::Source,
        model::WATER_SOURCE,
        node_flags::FLUID,
        Vec::new(),
    );
    let p = b.node(
        NodeKind::Mover,
        model::PUMP,
        node_flags::FLUID,
        vec![RateEntry {
            flags: rate_flags::SHARED,
            ..entry(None, pump, 1)
        }],
    );
    let boiler = b.node(
        NodeKind::Boiler,
        model::BOILER,
        node_flags::FLUID,
        Vec::new(),
    );
    b.nodes[boiler as usize].params = [16, 1, 0, 0];
    let h = b.node(NodeKind::Heater, model::BLAZE_BURNER, 0, Vec::new());
    b.nodes[h as usize].params = [0, i64::from(heat::KINDLED), 0, 0];
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let m = b.node(
        NodeKind::Machine,
        model::PRESS,
        node_flags::ANY_OUTPUT_BLOCKS,
        vec![
            RateEntry {
                level: 1,
                ..entry(Some(press), 1, 20)
            },
            RateEntry {
                level: 2,
                ..entry(Some(press), 1, 10)
            },
        ],
    );
    b.nodes[m as usize].domain = Some(0);
    b.edge_k(EdgeKind::Pipe, water, p, &[WATER]);
    b.edge_k(EdgeKind::Pipe, p, boiler, &[WATER]);
    let he = b.edge_k(EdgeKind::Heat, h, boiler, &[]);
    b.edges[he as usize].param = 4;
    b.edge(input, m);
    b.edge(m, out);
    let level = |selector: u32, speed: i32, capacity: f64| DomainLevel {
        selector,
        speed_num: speed,
        speed_den: 1,
        capacity,
    };
    b.domains.push(GraphDomain {
        head: head(0),
        network: 1,
        current: 2,
        driver: Some(boiler),
        flags,
        live_stress: 0.0,
        live_capacity: 0.0,
        levels: vec![level(0, 0, 0.0), level(2, 16, 1000.0), level(4, 32, 4096.0)],
        members: vec![
            Member {
                node: Some(m),
                count: 1,
                ratio_num: 1,
                ratio_den: 1,
                impact: 8.0,
            },
            Member {
                node: None,
                count: 4,
                ratio_num: 1,
                ratio_den: 1,
                impact: 16.0,
            },
        ],
    });
    let g = b.graph();
    let rg = regime(&g, &[(input, &[BRASS])]);
    (g, rg, p)
}

#[test]
fn the_boiler_level_selects_the_domain_level() {
    let (g, rg, _) = steam(40, 0);
    let s = run(&g, &rg);
    assert_eq!(s.dom_level, vec![Some(2)]);
    assert_eq!(bound(&s, SHEET).in_l, Q::new(1, 10));
}

#[test]
fn less_water_drops_the_level_into_overstress_and_the_members_stop() {
    // 20 mB per tick: boiler level 2, the slow level, whose capacity the load exceeds.
    let (g, rg, _) = steam(20, 0);
    let s = run(&g, &rg);
    assert_eq!(s.dom_level, vec![None]);
    assert_eq!(bound(&s, SHEET).in_l, Q::ZERO);
    assert!(s.converged);
    // Water stopped altogether: level 0, where the press has no entry.
    let (g, mut rg, pump) = steam(40, 0);
    rg.gate_off[pump as usize] = true;
    let s = run(&g, &rg);
    assert_eq!(s.dom_level, vec![Some(0)]);
    assert_eq!(bound(&s, SHEET).in_l, Q::ZERO);
}

#[test]
fn stress_disabled_domains_never_overstress() {
    let (g, rg, _) = steam(20, domain_flags::STRESS_DISABLED);
    let s = run(&g, &rg);
    assert_eq!(s.dom_level, vec![Some(1)]);
    assert_eq!(bound(&s, SHEET).in_l, Q::new(1, 20));
}

#[test]
fn an_overstressed_domain_without_a_driver_stops_its_members() {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let m = b.machine(model::PRESS, press, 1, 10);
    b.nodes[m as usize].domain = Some(0);
    b.edge(input, m);
    b.edge(m, out);
    let dom = |capacity: f64, flags: u32| GraphDomain {
        head: head(0),
        network: 1,
        current: 0,
        driver: None,
        flags,
        live_stress: 0.0,
        live_capacity: 0.0,
        levels: vec![DomainLevel {
            selector: 0,
            speed_num: 64,
            speed_den: 1,
            capacity,
        }],
        members: vec![Member {
            node: Some(m),
            count: 1,
            ratio_num: 1,
            ratio_den: 1,
            impact: 8.0,
        }],
    };
    // 64 × 8 = 512 SU: 511 is overstressed, 512 is not (Create's `capacity < stress`).
    for (cap, flags, want) in [
        (511.0, 0, Q::ZERO),
        (512.0, 0, Q::new(1, 10)),
        (511.0, domain_flags::STRESS_DISABLED, Q::new(1, 10)),
    ] {
        let mut b = b.clone();
        b.domains = vec![dom(cap, flags)];
        let g = b.graph();
        let s = run(&g, &regime(&g, &[(input, &[BRASS])]));
        assert_eq!(bound(&s, SHEET).in_l, want, "capacity {cap} flags {flags}");
    }
}

// ---- graph shapes ----------------------------------------------------------------------------------

#[test]
fn a_return_loop_is_one_component_and_its_goods_are_bounded() {
    // input -> j1 -> press -> j2 -> output (sheets), j2 -> j1 (everything else back).
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let j1 = b.junction(rule::NONE);
    let p = b.machine(model::PRESS, press, 1, 10);
    b.nodes[p as usize].flags |= node_flags::PASS_THROUGH;
    let j2 = b.junction(rule::PREFER_FIRST);
    b.edge(input, j1);
    b.edge(j1, p);
    b.edge(p, j2);
    b.edge_k(EdgeKind::Adjacency, j2, out, &[SHEET]);
    b.edge(j2, j1);
    let g = b.graph();
    assert_eq!(g.cycles, 1);
    assert!(g.cyclic[j1 as usize] && g.cyclic[p as usize] && g.cyclic[j2 as usize]);
    let s = run(&g, &regime(&g, &[(input, &[BRASS])]));
    let sheet = bound(&s, SHEET);
    assert_eq!(sheet.in_l, Q::new(1, 10));
    assert_eq!(sheet.class, class::BOUNDED);
    assert!(sheet.in_u >= sheet.in_l);
}

#[test]
fn a_sequenced_assembly_is_one_node_at_its_loop_period() {
    // §9.3: the unrolled recipe on one SEQUENCED node; 5 loops of a 3-step sequence, each step
    // 20 ticks, is one product per 300 ticks.
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let r = b.recipe(&[(BRASS, 1), (ZINC, 5), (COPPER, 5)], &[(SHEET, 1)]);
    b.recipes[r as usize].recipe_type = recipe_type::SEQUENCED;
    let m = b.machine(model::SEQUENCED, r, 1, 300);
    b.edge(input, m);
    b.edge(m, out);
    let g = b.graph();
    assert_eq!(g.cycles, 0);
    let s = run(&g, &regime(&g, &[(input, &[BRASS, ZINC, COPPER])]));
    assert_eq!(bound(&s, SHEET).in_l, Q::new(1, 300));
    assert_eq!(bound(&s, ZINC).out_l, Q::new(5, 300));
    assert_eq!(bound(&s, SHEET).class, class::EXACT);
}

#[test]
fn a_black_box_makes_its_goods_unbounded() {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let bb = b.node(NodeKind::BlackBox, model::BLACK_BOX, 0, Vec::new());
    b.edge(input, bb);
    b.edge_k(EdgeKind::Adjacency, bb, out, &[SHEET]);
    let g = b.graph();
    let s = run(&g, &regime(&g, &[(input, &[BRASS])]));
    let sheet = bound(&s, SHEET);
    assert_eq!(sheet.class, class::UNBOUNDED);
    assert_eq!(sheet.in_l, Q::ZERO);
    assert!(sheet.in_u.is_inf());
    assert!(s.has_unbounded);
}

// ---- the stochastic source (G-B11) and σ -----------------------------------------------------------

fn sheep() -> Graph {
    let mut b = B::default();
    let out = b.store(0);
    let r = b.recipe(&[], &[(WOOL, 2)]);
    let s = b.source(r, 1, 600);
    b.nodes[s as usize].model = model::ANIMAL_SHEEP;
    b.nodes[s as usize].flags = node_flags::STOCHASTIC;
    // One shearing gives 1 + nextInt(3) wool: E[X] = 2, E[X²] = 14/3.
    b.nodes[s as usize].params = [4, 4, 14, 3];
    b.edge(s, out);
    b.window = 6000;
    b.graph()
}

#[test]
fn a_sheep_source_carries_its_compound_poisson_variance() {
    let g = sheep();
    let s = run(&g, &regime(&g, &[]));
    let wool = bound(&s, WOOL);
    assert_eq!(wool.in_l, Q::new(1, 300));
    assert!(close(wool.var_rate, 14.0 / 3.0 / 600.0));
}

/// splitmix64.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn the_least_squares_slope_of_a_compound_poisson_count_has_variance_six_fifths_lambda_ex2_over_w() {
    // λ = 1/60 per tick, X uniform on {1, 2, 3}: λ E[X²] = 14/180. W = 6,000 sampled every 20.
    let (lambda, w, step, windows) = (1.0 / 60.0, 6000u64, 20u64, 3000);
    let mut rng = Rng(42);
    let mut slopes = Vec::with_capacity(windows);
    for _ in 0..windows {
        let mut events: Vec<(f64, f64)> = Vec::new();
        let mut t = 0.0;
        loop {
            t += -(1.0 - rng.unit()).ln() / lambda;
            if t > w as f64 {
                break;
            }
            events.push((t, (1 + rng.next() % 3) as f64));
        }
        let (mut n, mut st, mut stt, mut sy, mut sty) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut k = 0;
        let mut y = 0.0;
        for s in (0..=w).step_by(step as usize) {
            while k < events.len() && events[k].0 <= s as f64 {
                y += events[k].1;
                k += 1;
            }
            let x = s as f64;
            n += 1.0;
            st += x;
            stt += x * x;
            sy += y;
            sty += x * y;
        }
        slopes.push((n * sty - st * sy) / (n * stt - st * st));
    }
    let mean = slopes.iter().sum::<f64>() / windows as f64;
    let var = slopes.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (windows - 1) as f64;
    let model = POISSON_SLOPE_FACTOR * lambda * (14.0 / 3.0) / w as f64;
    assert!(
        (var / model - 1.0).abs() < 0.1,
        "measured {var:e} vs model {model:e} (ratio {})",
        var / model
    );
}

/// A press (1 per 10 ticks) making a sheet and two chance outputs from brass: an XP nugget at 3/4,
/// and copper at 3/2 (2 units at 3/4). `split`: the nuggets go through a split junction, half to a
/// void and half to the store.
fn chance_press(split: bool) -> (Graph, Regime) {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let r = b.recipe(&[(BRASS, 1)], &[(SHEET, 1), (XP, 1), (COPPER, 1)]);
    let chance = |good, num, den| Term {
        good,
        flags: term_flags::CHANCE,
        num,
        den,
    };
    b.recipes[r as usize].outputs[1] = chance(XP, 3, 4);
    b.recipes[r as usize].outputs[2] = chance(COPPER, 3, 2);
    let p = b.machine(model::PRESS, r, 1, 10);
    b.edge(input, p);
    if split {
        let j = b.junction(rule::SPLIT);
        let void = b.node(NodeKind::Sink, model::SINK_VOID, 0, Vec::new());
        b.edge_k(EdgeKind::Adjacency, p, out, &[SHEET, COPPER]);
        b.edge_k(EdgeKind::Adjacency, p, j, &[XP]);
        b.edge(j, void);
        b.edge(j, out);
    } else {
        b.edge(p, out);
    }
    let g = b.graph();
    let rg = regime(&g, &[(input, &[BRASS])]);
    (g, rg)
}

#[test]
fn chance_outputs_add_their_roll_variance_to_the_checksum() {
    assert!(close(solve::chance_variance(0.75), 0.1875));
    assert!(close(solve::chance_variance(1.5), 0.375));
    assert_eq!(solve::chance_variance(0.0), 0.0);
    assert_eq!(solve::chance_variance(1.0), 0.0);
    let (g, rg) = chance_press(false);
    let s = run(&g, &rg);
    assert!(close(bound(&s, XP).var_rate, 0.1 * 0.1875));
    assert!(close(bound(&s, COPPER).var_rate, 0.1 * 0.375));
    assert_eq!(bound(&s, SHEET).var_rate, 0.0);
    // Half the nuggets are voided: half the roll variance reaches the store.
    let (g, rg) = chance_press(true);
    let s = run(&g, &rg);
    assert_eq!(bound(&s, XP).in_l, Q::new(3, 80));
    assert!(close(bound(&s, XP).var_rate, 0.5 * 0.1 * 0.1875));
}

#[test]
fn the_least_squares_slope_of_a_rolled_output_has_variance_six_fifths_c_var_over_w() {
    // A craft every 40 ticks rolling an output at 4/5 (the precision mechanism): a random walk
    // with variance 0.16 a craft. W = 6,000 sampled every 20, as GB20's check.
    let (period, e, w, step, windows) = (40u64, 0.8, 6000u64, 20u64, 3000);
    let mut rng = Rng(7);
    let mut slopes = Vec::with_capacity(windows);
    for _ in 0..windows {
        let (mut n, mut st, mut stt, mut sy, mut sty) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut y = 0.0;
        for t in (0..=w).step_by(step as usize) {
            if t > 0 && t % period == 0 && rng.unit() < e {
                y += 1.0;
            }
            let x = t as f64;
            n += 1.0;
            st += x;
            stt += x * x;
            sy += y;
            sty += x * y;
        }
        slopes.push((n * sty - st * sy) / (n * stt - st * st));
    }
    let mean = slopes.iter().sum::<f64>() / windows as f64;
    let var = slopes.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (windows - 1) as f64;
    let rate = solve::chance_variance(e) / period as f64;
    let model = POISSON_SLOPE_FACTOR * rate / w as f64;
    assert!(
        (var / model - 1.0).abs() < 0.1,
        "measured {var:e} vs model {model:e} (ratio {})",
        var / model
    );
}

// ---- the checksum (§5.3, G-B14) ------------------------------------------------------------------

fn inventory(index: u32, contents: &[(Good, i64)]) -> Inventory {
    Inventory {
        decl: InventoryDecl {
            district: D,
            revision: 1,
            index,
            pos: BlockPos::default(),
            kind: InvKind(InvKind::ITEMS),
            item_slots: 10_000,
            fluid_capacity: 0,
            energy_capacity: 0,
        },
        chunk: BlockPos::default().chunk(0),
        known: GoodVec::from_pairs(contents.iter().copied()),
        known_at: Tick::ZERO,
        pending: GoodVec::new(),
        held: GoodVec::new(),
        received: BTreeSet::new(),
        doomed: false,
        destroyed: false,
    }
}

/// A store of brass feeding a press (1 per 10 ticks) into a store of sheets, a 2,000-tick window.
fn press_line() -> (B, u32) {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let p = b.machine(model::PRESS, press, 1, 10);
    b.edge(input, p);
    b.edge(p, out);
    b.window = 2000;
    (b, p)
}

/// Drives a compile with sweeps every `step` ticks from 0 to `to`; `made(t)` is the sheets made
/// by tick t. Returns the probe sets sent, by tick.
fn drive(
    c: &mut Compiled,
    step: u64,
    to: u64,
    made: impl Fn(u64) -> i64,
    tun: &GlassTunables,
) -> Vec<(u64, Vec<u32>)> {
    let goods = goods();
    let ports = GoodVec::new();
    let mut probes = Vec::new();
    let mut t = 0;
    while t <= to {
        let m = made(t);
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(BRASS, 100_000 - m)]));
        invs.insert(1, inventory(1, &[(SHEET, m)]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if t == 0 {
            c.start(&view, &goods, tun);
        }
        let (p, _) = c.sweep(Tick(t), &view, &goods, tun);
        if let Some(p) = p {
            probes.push((t, p));
        }
        t += step;
    }
    probes
}

#[test]
fn a_matching_window_passes_after_the_warm_up() {
    let (b, _) = press_line();
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    let probes = drive(&mut c, 100, 3200, |t| (t / 10) as i64, &tun);
    assert!(probes.is_empty());
    let r = c.last.clone().expect("a window completed");
    assert_eq!(r.start, Tick(tun.warmup_ticks));
    assert_eq!(r.ticks, 2000);
    assert_eq!(r.verdict, verdict::PASS);
    let sheet = r.goods.iter().find(|g| g.good == SHEET).expect("sheet");
    assert!(close(sheet.measured, 0.1) && close(sheet.predicted, 0.1));
    assert_eq!(sheet.samples, 21);
    assert!(close(sheet.q, 2.0 / 2000.0));
    let rep = c.report(&DistrictGlass::default());
    assert_eq!(rep.windows_completed, 1);
    assert_eq!(rep.verdict, verdict::PASS);
    assert_eq!(rep.window_ticks_target, 2000);
    // Sheets reach a stack at tick 640: the output store holds them, a new regime key.
    assert_eq!(rep.solves, 2);
}

#[test]
fn a_slow_press_fails_the_window_and_is_derated_and_probed() {
    let (b, p) = press_line();
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    // Half the predicted rate.
    let probes = drive(&mut c, 100, 3200, |t| (t / 20) as i64, &tun);
    let r = c.last.clone().expect("a window completed");
    assert_eq!(r.verdict, verdict::FAIL);
    assert_eq!(c.strikes, 1);
    assert_eq!(c.regime.derate.get(&p), Some(&Q::new(1, 2)));
    assert_eq!(c.strikes, 1);
    assert_eq!(probes, vec![(3200, vec![p])]);
    assert!(
        c.events
            .iter()
            .any(|e| matches!(e, GlassEvent::Derated { node, .. } if *node == p))
    );
    // Re-solved at the derated rate: the next window passes and the strikes reset.
    assert_eq!(bound(&c.sol, SHEET).in_l, Q::new(1, 20));
    let mut c2 = c.clone();
    let tun2 = tun.clone();
    let mut t = 3300;
    let goods = goods();
    let ports = GoodVec::new();
    while t <= 5200 {
        let m = (t / 20) as i64;
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(BRASS, 100_000 - m)]));
        invs.insert(1, inventory(1, &[(SHEET, m)]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        c2.sweep(Tick(t), &view, &goods, &tun2);
        t += 100;
    }
    assert_eq!(c2.windows_completed, 2);
    assert_eq!(c2.last.as_ref().map(|r| r.verdict), Some(verdict::PASS));
    assert_eq!(c2.strikes, 0);
    // A probe sample of the probed press is kept; another node's is not.
    c2.probe_sample(3, p, &GoodVec::from_pairs([(BRASS, 2)]));
    c2.probe_sample(4, p, &GoodVec::from_pairs([(BRASS, 5)]));
    c2.probe_sample(4, 0, &GoodVec::from_pairs([(BRASS, 5)]));
    assert_eq!(c2.probes.len(), 1);
    assert_eq!(c2.probes[&p].first, 2);
    assert_eq!(c2.probes[&p].last, 5);
}

#[test]
fn a_kind_is_demoted_after_the_strikes() {
    let (b, p) = press_line();
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    // The press keeps slowing: every window measures half the (derated) prediction.
    let made = |t: u64| -> i64 {
        let seg = |a: u64, b: u64, per: u64| (t.clamp(a, b) - a) / per;
        (seg(0, 3200, 20) + seg(3200, 5200, 40) + seg(5200, 7200, 80)) as i64
    };
    let probes = drive(&mut c, 400, 7200, made, &tun);
    assert_eq!(c.windows_completed, 3);
    // The probe set is sent once: it did not change.
    assert_eq!(probes.len(), 1);
    assert!(c.regime.demoted.contains(&model::PRESS));
    assert_eq!(c.strikes, 0);
    assert!(c.events.iter().any(|e| matches!(
        e,
        GlassEvent::Demoted { model: m, node } if *m == model::PRESS && *node == p
    )));
    // Measured from now on: the press is a black box and its goods are not judged.
    assert_eq!(bound(&c.sol, SHEET).class, class::UNBOUNDED);
}

/// A controller reading a store of markers gates the press: it runs only while the switch is on
/// (on above 100, off below 20).
fn gated_line() -> (B, u32, u32) {
    let (mut b, p) = press_line();
    let markers = b.store(2);
    let ctl = b.node(NodeKind::Controller, model::THRESHOLD_SWITCH, 0, Vec::new());
    b.nodes[ctl as usize].params = [100, 20, 0, 50];
    b.edge_k(EdgeKind::Observe, markers, ctl, &[]);
    let e = b.edge_k(EdgeKind::Control, ctl, p, &[]);
    b.edges[e as usize].param = CONTROL_RUN_WHEN_ON;
    (b, p, ctl)
}

#[test]
fn a_controller_switches_with_hysteresis_at_each_sweep() {
    let (b, p, ctl) = gated_line();
    let tun = GlassTunables::default();
    let goods = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    assert!(
        !c.ctrl_on[ctl as usize],
        "50 is between: off, as at compile"
    );
    let mut gate = Vec::new();
    for (i, level) in [50, 120, 50, 21, 20, 50, 100].into_iter().enumerate() {
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(BRASS, 1000)]));
        invs.insert(1, inventory(1, &[]));
        invs.insert(2, inventory(2, &[(MARKER, level)]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if i == 0 {
            c.start(&view, &goods, &tun);
        }
        c.sweep(Tick(100 * i as u64), &view, &goods, &tun);
        gate.push(c.regime.gate_off[p as usize]);
    }
    assert_eq!(gate, vec![true, false, false, false, true, true, false]);
    // Two regimes, each solved once; the rest came from the cache.
    assert_eq!(c.solves, 2);
    assert!(c.cache_hits >= 2);
}

#[test]
fn a_window_spanning_two_regimes_predicts_their_time_average() {
    let (b, _, _) = gated_line();
    let tun = GlassTunables::default();
    let goods = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    // The switch is on from 1,200 to 2,200 and off after: the press runs 1,000 of the window's
    // 2,000 ticks.
    let on = |t: u64| (1200..2200).contains(&t);
    let mut made = 0i64;
    let mut t = 0;
    while t <= 3200 {
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(BRASS, 100_000 - made)]));
        invs.insert(1, inventory(1, &[(SHEET, made)]));
        invs.insert(2, inventory(2, &[(MARKER, if on(t) { 150 } else { 10 })]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if t == 0 {
            c.start(&view, &goods, &tun);
        }
        c.sweep(Tick(t), &view, &goods, &tun);
        // The regime read at t holds until the next sweep.
        if on(t) {
            made += 10;
        }
        t += 100;
    }
    let r = c.last.clone().expect("a window");
    let sheet = r.goods.iter().find(|g| g.good == SHEET).expect("sheet");
    let brass = r.goods.iter().find(|g| g.good == BRASS).expect("brass");
    assert_eq!(sheet.verdict, verdict::PASS, "{r:?}");
    assert_eq!(brass.verdict, verdict::PASS, "{r:?}");
    assert!(sheet.predicted > 0.01 && sheet.predicted < 0.1);
    assert!(close(sheet.predicted, sheet.measured));
    // The markers moved with no edge to move them: the model has no flow for them.
    let marker = r.goods.iter().find(|g| g.good == MARKER).expect("marker");
    assert_eq!(marker.predicted, 0.0);
}

// ---- heaters lit at the compile, holding, ties, the report (integration follow-ups) ---------------

/// A heated mixer (1 per 17 ticks, 2 brass each) whose blaze burner has `params` (remaining burn,
/// level, burn ticks per fuel, insertion threshold); `fuel`: coal reaches it from the input store.
/// The heater is the last node.
fn burning_mixer(params: [i64; 4], fuel: bool) -> B {
    let mut b = B::default();
    let input = b.store(0);
    let out = b.store(1);
    let mix = b.recipe(&[(ZINC, 1), (COPPER, 1)], &[(BRASS, 2)]);
    b.recipes[mix as usize].heat = heat::NEEDS_HEATED;
    let mixer = b.machine(model::MIXER, mix, 1, 17);
    b.edge_k(EdgeKind::Adjacency, input, mixer, &[ZINC, COPPER]);
    b.edge(mixer, out);
    let h = b.node(NodeKind::Heater, model::BLAZE_BURNER, 0, Vec::new());
    b.nodes[h as usize].params = params;
    if fuel {
        b.edge_k(EdgeKind::Adjacency, input, h, &[COAL]);
    }
    b.edge_k(EdgeKind::Heat, h, mixer, &[]);
    b.window = 2000;
    b
}

/// The least-squares slope of `p` over sample times `ts`.
fn ls_slope(ts: &[f64], p: impl Fn(f64) -> f64) -> f64 {
    let n = ts.len() as f64;
    let tm = ts.iter().sum::<f64>() / n;
    let pm = ts.iter().map(|&t| p(t)).sum::<f64>() / n;
    let sxy: f64 = ts.iter().map(|&t| (t - tm) * (p(t) - pm)).sum();
    let sxx: f64 = ts.iter().map(|&t| (t - tm) * (t - tm)).sum();
    sxy / sxx
}

#[test]
fn a_heater_lit_at_the_compile_runs_on_its_burn_then_goes_out() {
    // The corpus's unfed burners: 9,984 ticks left, no fuel edge (burn ticks 1). Lit and refusing
    // fuel to 9,484, lit and taking fuel (none comes) to 9,984, then out.
    let b = burning_mixer([9984, i64::from(heat::KINDLED), 1, 500], false);
    let h = b.nodes.len() - 1;
    let tun = GlassTunables::default();
    let goods = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    assert_eq!(c.regime.burning[h], solve::burn::REFUSING);
    assert_eq!(c.heaters_burning(), 1);
    assert_eq!(c.next_breakpoint(), Some(9484));
    let crafts = |t: u64| (t.min(9984) / 17) as i64;
    let mut seen = Vec::new();
    let mut t = 0;
    while t <= 11_200 {
        let k = crafts(t);
        let mut invs = VecMap::new();
        invs.insert(
            0,
            inventory(0, &[(ZINC, 100_000 - k), (COPPER, 100_000 - k)]),
        );
        invs.insert(1, inventory(1, &[(BRASS, 2 * k)]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if t == 0 {
            c.start(&view, &goods, &tun);
            assert_eq!(bound(&c.sol, BRASS).in_l, Q::new(2, 17));
        }
        c.sweep(Tick(t), &view, &goods, &tun);
        if matches!(t, 9400 | 9500 | 10_000) {
            seen.push((t, c.regime.burning[h], bound(&c.sol, BRASS).in_l));
        }
        t += 100;
    }
    assert_eq!(
        seen,
        vec![
            (9400, solve::burn::REFUSING, Q::new(2, 17)),
            (9500, solve::burn::TAKING, Q::new(2, 17)),
            (10_000, solve::burn::OUT, Q::ZERO),
        ]
    );
    assert_eq!(c.heaters_burning(), 0);
    assert_eq!(c.next_breakpoint(), None);
    // Five windows passed; the last, [9,200, 11,200], predicts the burner going out at 9,984 (the
    // sweep interval [9,900, 10,000] is cut there).
    assert_eq!(c.windows_completed, 5);
    let r = c.last.clone().expect("windows");
    assert_eq!((r.start, r.verdict), (Tick(9200), verdict::PASS));
    let ts: Vec<f64> = (0..=20).map(|i| 9200.0 + 100.0 * f64::from(i)).collect();
    let expect = ls_slope(&ts, |t| 2.0 / 17.0 * (t.min(9984.0) - 9200.0));
    let brass = r.goods.iter().find(|g| g.good == BRASS).expect("brass");
    assert!(
        close(brass.predicted, expect),
        "{} vs {expect}",
        brass.predicted
    );
    assert_eq!(brass.verdict, verdict::PASS);
}

#[test]
fn a_heater_takes_fuel_once_its_burn_falls_to_the_threshold() {
    // 1,593 ticks left, 1,600 per coal, threshold 500: it refuses coal until 1,093, then takes one
    // per 1,600 ticks and stays lit. The input store holds 16 coal: nothing brings coal in, so it
    // supplies (GB100).
    let b = burning_mixer([1593, i64::from(heat::KINDLED), 1600, 500], true);
    let h = b.nodes.len() - 1;
    let tun = GlassTunables::default();
    let goods = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    let mut seen = Vec::new();
    for t in [0u64, 1000, 1100, 3000] {
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(ZINC, 1000), (COPPER, 1000), (COAL, 16)]));
        invs.insert(1, inventory(1, &[]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if t == 0 {
            c.start(&view, &goods, &tun);
        }
        c.sweep(Tick(t), &view, &goods, &tun);
        seen.push((
            t,
            c.regime.burning[h],
            bound(&c.sol, COAL).out_l,
            bound(&c.sol, BRASS).in_l,
            c.next_breakpoint(),
        ));
    }
    let lit = Q::new(2, 17);
    assert_eq!(
        seen,
        vec![
            (0, solve::burn::REFUSING, Q::ZERO, lit, Some(1093)),
            (1000, solve::burn::REFUSING, Q::ZERO, lit, Some(1093)),
            (1100, solve::burn::TAKING, Q::new(1, 1600), lit, None),
            (3000, solve::burn::TAKING, Q::new(1, 1600), lit, None),
        ]
    );
}

/// Cornflowers at 1/7 per tick into a round-robin brass tunnel with a millstone (1 per 8 ticks, 2
/// dye each) at its side and a belt on to a store straight on: the corpus's brass_tunnels.
fn tunnel_mill(side_filtered: bool) -> Solution {
    let mut b = B::default();
    let input = b.store(0);
    let dye = b.store(1);
    let rest = b.store(2);
    let mill_r = b.recipe(&[(ORE, 1)], &[(CRUSHED, 2)]);
    let mill = b.machine(model::MILLSTONE, mill_r, 1, 8);
    let t = b.junction(rule::ROUND_ROBIN);
    let feed = b.edge_k(EdgeKind::Chute, input, t, &[ORE]);
    b.edges[feed as usize].capacity = Some((1, 7));
    let side = b.edge(t, mill);
    if side_filtered {
        b.edges[side as usize].flags |= edge_flags::FILTERED_SIDE;
    }
    b.edge_k(EdgeKind::Belt, t, rest, &[]);
    b.edge(mill, dye);
    let g = b.graph();
    run(&g, &regime(&g, &[(input, &[ORE])]))
}

#[test]
fn a_brass_tunnel_shares_unfiltered_sides_and_serves_filtered_ones_first() {
    // As Create (`BrassTunnelBlockEntity`): a side whose flap filter is empty is in the second
    // list with the straight-on output, and round robin halves the flow: what the corpus measured
    // (85.7 cornflowers and 171.4 dye per minute).
    let s = tunnel_mill(false);
    assert_eq!(bound(&s, CRUSHED).in_l, Q::new(1, 7));
    assert_eq!(bound(&s, ORE).in_l, Q::new(1, 14));
    assert!(close(
        (bound(&s, ORE).in_l.to_f64() - bound(&s, ORE).out_l.to_f64()) * MIN,
        -1200.0 / 14.0
    ));
    // A filtered side (FILTERED_SIDE) is served first: the millstone runs flat out.
    let s = tunnel_mill(true);
    assert_eq!(bound(&s, CRUSHED).in_l, Q::new(1, 4));
    assert_eq!(bound(&s, ORE).in_l, Q::new(1, 56));
}

#[test]
fn the_report_gives_the_window_it_judged_after_a_derate() {
    // The slow press fails its window and is derated: the solve now predicts half, but the report
    // is the window's, so its bounds contain its prediction (U >= L, lo <= predicted <= hi).
    let (b, p) = press_line();
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    drive(&mut c, 100, 3200, |t| (t / 20) as i64, &tun);
    assert!(c.regime.derate.contains_key(&p));
    assert_eq!(bound(&c.sol, SHEET).in_l, Q::new(1, 20));
    let rep = c.report(&DistrictGlass::default());
    assert_eq!(rep.verdict, verdict::FAIL);
    for g in &rep.goods {
        assert!(g.in_u >= g.in_l && g.out_u >= g.out_l, "{g:?}");
        assert!(
            g.lo <= g.predicted + 1e-12 && g.predicted <= g.hi + 1e-12,
            "{g:?}"
        );
        assert!(close(g.predicted, g.in_l - g.out_l), "{g:?}");
    }
    let sheet = rep.goods.iter().find(|g| g.good == SHEET.0).expect("sheet");
    assert!(close(sheet.in_l, 0.1) && close(sheet.predicted, 0.1));
    assert!(close(sheet.measured, 0.05));
    assert_eq!(sheet.verdict, verdict::FAIL);
}

/// The corpus's arm_burners_depots, cut down: a store of 16 coal, a smart chute (1 per 7 ticks)
/// an arm takes from, two blaze burners the arm feeds (1,582 and 782 ticks left, 1,600 per coal).
/// `drop`: the chute also drops onto the floor (a sink), at the order of the arm's pull, as Java
/// encoded it.
fn arm_burners(drop: bool) -> (B, u32) {
    let mut b = B::default();
    let coal = b.store(0);
    let shared = |batch, ticks| RateEntry {
        flags: rate_flags::SHARED,
        ..entry(None, batch, ticks)
    };
    let chute = b.node(NodeKind::Mover, model::SMART_CHUTE, 0, vec![shared(1, 7)]);
    let arm = b.node(NodeKind::Mover, model::ARM, 0, vec![shared(1, 16)]);
    b.nodes[arm as usize].rule = rule::PREFER_FIRST;
    let mut heaters = Vec::new();
    for left in [1582, 782] {
        let h = b.node(NodeKind::Heater, model::BLAZE_BURNER, 0, Vec::new());
        b.nodes[h as usize].params = [left, i64::from(heat::KINDLED), 1600, 500];
        heaters.push(h);
    }
    b.edge_k(EdgeKind::Chute, coal, chute, &[]);
    let pull = b.edge_k(EdgeKind::ArmReach, chute, arm, &[]);
    for h in heaters {
        b.edge_k(EdgeKind::ArmReach, arm, h, &[]);
    }
    if drop {
        let sink = b.node(NodeKind::Sink, model::SINK_VOID, 0, Vec::new());
        let d = b.edge_k(EdgeKind::Drop, chute, sink, &[]);
        b.edges[d as usize].order = b.edges[pull as usize].order;
    }
    b.window = 2000;
    (b, chute)
}

fn coal_view_sweep(c: &mut Compiled, t: u64, coal: i64, tun: &GlassTunables) {
    let goods = goods();
    let ports = GoodVec::new();
    let mut invs = VecMap::new();
    invs.insert(0, inventory(0, &[(COAL, coal)]));
    let view = View {
        revision: 1,
        inventories: &invs,
        ports: &ports,
    };
    if t == 0 {
        c.start(&view, &goods, tun);
    }
    c.sweep(Tick(t), &view, &goods, tun);
}

#[test]
fn burners_fed_by_an_arm_take_coal_as_their_burns_reach_the_threshold() {
    let (b, _) = arm_burners(false);
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    // What Create does: a burner takes a coal when its burn falls to 500, then every 1,600 ticks
    // (the 782 one at 282, 1,882, 3,482; the 1,582 one at 1,082, 2,682).
    let taken = |t: u64| {
        [282u64, 1082, 1882, 2682, 3482]
            .iter()
            .filter(|&&a| a <= t)
            .count() as i64
    };
    let mut out = Vec::new();
    let mut t = 0;
    while t <= 3200 {
        coal_view_sweep(&mut c, t, 16 - taken(t), &tun);
        if matches!(t, 200 | 300 | 1100) {
            out.push((t, bound(&c.sol, COAL).out_l));
        }
        t += 100;
    }
    assert_eq!(
        out,
        vec![
            (200, Q::ZERO),
            (300, Q::new(1, 1600)),
            (1100, Q::new(2, 1600)),
        ]
    );
    let coal = bound(&c.sol, COAL);
    assert_eq!(coal.class, class::EXACT);
    let r = c.last.clone().expect("a window");
    let g = r.goods.iter().find(|g| g.good == COAL).expect("coal");
    assert!(close(g.predicted, -2.0 / 1600.0));
    assert_eq!(g.verdict, verdict::PASS, "{g:?}");
}

#[test]
fn a_drop_to_the_floor_is_exact_but_a_tie_with_the_arm_is_bounded() {
    let (b, chute) = arm_burners(true);
    let tun = GlassTunables::default();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    coal_view_sweep(&mut c, 0, 16, &tun);
    let coal = bound(&c.sol, COAL);
    // Bounded, not unbounded: the drop onto the floor is lost at the chute's rate; which of the
    // drop and the arm's pull goes first is the edge index, not Create (GB102, GB103).
    assert_eq!(coal.class, class::BOUNDED);
    assert_eq!(coal.why, (why::PRIORITY_TIE, chute));
    // Both burners refuse fuel: the chute drops everything (Java must not send a drop where the
    // chute has a floor below it: `ChuteBlockEntity.handleDownwardOutput`).
    assert_eq!(coal.out_l, Q::new(1, 7));
    // Without the tie the drop is exact.
    let (mut b, _) = arm_burners(true);
    let n = b.edges.len();
    b.edges[n - 1].order = 7;
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    coal_view_sweep(&mut c, 0, 16, &tun);
    assert_eq!(bound(&c.sol, COAL).class, class::EXACT);
}

#[test]
fn the_inverted_flag_is_display_only() {
    // GB51: the Control param already includes the inversion; the engine reads "on" before it.
    let (mut b, p, ctl) = gated_line();
    b.nodes[ctl as usize].params[2] |= super::check::CONTROLLER_INVERTED;
    let tun = GlassTunables::default();
    let goods = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    let mut gate = Vec::new();
    for (i, level) in [50, 120, 20].into_iter().enumerate() {
        let mut invs = VecMap::new();
        invs.insert(0, inventory(0, &[(BRASS, 1000)]));
        invs.insert(1, inventory(1, &[]));
        invs.insert(2, inventory(2, &[(MARKER, level)]));
        let view = View {
            revision: 1,
            inventories: &invs,
            ports: &ports,
        };
        if i == 0 {
            c.start(&view, &goods, &tun);
        }
        c.sweep(Tick(100 * i as u64), &view, &goods, &tun);
        gate.push(c.regime.gate_off[p as usize]);
    }
    // RUN_WHEN_ON: stopped while off, running while on, as without the flag.
    assert_eq!(gate, vec![true, false, true]);
}

/// The corpus's precision_mechanisms after Java's sequenced-assembly merge: a SEQUENCED host (n5,
/// one product per 30 ticks) on a belt loop (n5, n6, n7 deployers; n14 the prefer-first exit to the
/// output junction n0; n16, n18, n17, n15 back to n5), cogs, large cogs and nuggets by chute, and
/// golden sheets from n11 through n1 and n12 (one per 64 ticks) onto the loop at n18: the only way a
/// sheet reaches the host is round the loop. `perm[id]` is node `id`'s index in the records.
fn precision_mechanisms(perm: &[u32; 19]) -> (Graph, [u32; 5]) {
    precision_mechanisms_fed(perm, 9, 64)
}

/// Corpus 6's compile with the andesite funnel moving a sheet per `funnel` ticks onto a feeder belt
/// that takes an entry per `feeder` ticks (64 at 8 RPM, 8 at 64 RPM).
fn precision_mechanisms_fed(perm: &[u32; 19], funnel: u32, feeder: u32) -> (Graph, [u32; 5]) {
    let (b, stores) = precision_builder(perm, funnel, feeder);
    (b.graph(), stores)
}

/// [`precision_mechanisms_fed`]'s builder: the stores are inventories 0 (cogs, zinc here), 1
/// (golden sheets), 2 (nuggets, coal), 3 (large cogs, copper) and 4 (the output).
fn precision_builder(perm: &[u32; 19], funnel: u32, feeder: u32) -> (B, [u32; 5]) {
    let mut b = B::default();
    let r = b.recipe(
        &[(SHEET, 1), (COAL, 5), (ZINC, 5), (COPPER, 5)],
        &[(BRASS, 1), (SHEET, 1), (ZINC, 1)],
    );
    b.recipes[r as usize].recipe_type = recipe_type::SEQUENCED;
    let chance = |good, num, den| Term {
        good,
        flags: term_flags::CHANCE,
        num,
        den,
    };
    b.recipes[r as usize].outputs = vec![
        chance(BRASS, 4, 5),
        chance(SHEET, 4, 75),
        chance(ZINC, 1, 30),
    ];
    let shared = |batch: u32, ticks: u32| RateEntry {
        flags: rate_flags::SHARED,
        ..entry(None, batch, ticks)
    };
    let mut by_index = [0u32; 19];
    for (id, &i) in perm.iter().enumerate() {
        by_index[i as usize] = id as u32;
    }
    for &id in &by_index {
        let v = match id {
            0 => b.junction(rule::NONE),
            1 => b.node(
                NodeKind::Mover,
                model::FUNNEL_ANDESITE,
                0,
                vec![shared(1, funnel)],
            ),
            2..=4 => b.node(NodeKind::Mover, model::CHUTE, 0, vec![shared(16, 7)]),
            5 => b.node(
                NodeKind::Machine,
                model::SEQUENCED,
                node_flags::PASS_THROUGH,
                vec![entry(Some(r), 1, 30)],
            ),
            6 | 7 => b.node(
                NodeKind::Machine,
                model::DEPLOYER,
                node_flags::PASS_THROUGH,
                Vec::new(),
            ),
            8 => b.store(4),
            9 => b.store(0),
            10 => b.store(3),
            11 => b.store(1),
            12 => b.node(NodeKind::Mover, model::BELT, 0, vec![shared(1, feeder)]),
            13 => b.store(2),
            _ => b.node(NodeKind::Mover, model::BELT, 0, vec![shared(64, 2)]),
        };
        assert_eq!(v, perm[id as usize]);
        if id == 14 {
            b.nodes[v as usize].rule = rule::PREFER_FIRST;
        }
    }
    let n = |id: usize| perm[id];
    b.edge_k(EdgeKind::Adjacency, n(0), n(8), &[BRASS, SHEET, ZINC]);
    b.edge_k(EdgeKind::Chute, n(9), n(2), &[]);
    b.edge_k(EdgeKind::Chute, n(2), n(5), &[]);
    b.edge_k(EdgeKind::Chute, n(10), n(4), &[]);
    b.edge_k(EdgeKind::Chute, n(4), n(5), &[]);
    b.edge(n(11), n(1));
    b.edge(n(1), n(12));
    b.edge_k(EdgeKind::Chute, n(13), n(3), &[]);
    b.edge_k(EdgeKind::Chute, n(3), n(5), &[]);
    // A belt's own links are THROUGH (the chain along belt A, and C's from its start to the
    // feeder's segment); a belt's end into the next belt, and the feeder's, are insertions.
    let through = |b: &mut B, from: u32, to: u32| {
        let e = b.edge_k(EdgeKind::Belt, from, to, &[]);
        b.edges[e as usize].flags |= edge_flags::THROUGH;
    };
    through(&mut b, n(15), n(5));
    through(&mut b, n(5), n(6));
    through(&mut b, n(6), n(7));
    b.edge_k(EdgeKind::Belt, n(7), n(14), &[]);
    b.edge_k(EdgeKind::Belt, n(17), n(15), &[]);
    b.edge(n(14), n(0));
    b.edge_k(EdgeKind::Belt, n(14), n(16), &[]);
    through(&mut b, n(16), n(18));
    b.edge_k(EdgeKind::Belt, n(18), n(17), &[]);
    b.edge_k(EdgeKind::Belt, n(12), n(18), &[]);
    (b, [n(11), n(9), n(10), n(13), n(8)])
}

#[test]
fn a_loop_carries_its_back_edge_from_round_to_round_in_any_node_order() {
    // The exit run's order put the back edge on n15 -> n5, the host's only sheet supply; with back
    // edges carrying nothing in L the host never got a sheet (brass in [0, 2^-24]). Carried over
    // between rounds, every order gives a sheet per 64 ticks: 4/5 x 1/64 = 1/80 brass per tick
    // (GB110).
    let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
    let reversed: [u32; 19] = std::array::from_fn(|i| 18 - i as u32);
    let rotated: [u32; 19] = std::array::from_fn(|i| ((i + 7) % 19) as u32);
    for perm in [identity, reversed, rotated] {
        let (g, [sheets, cogs, large, nug, _]) = precision_mechanisms(&perm);
        assert_eq!(g.cycles, 1);
        let s = run(
            &g,
            &regime(
                &g,
                &[
                    (sheets, &[SHEET]),
                    (cogs, &[ZINC]),
                    (large, &[COPPER]),
                    (nug, &[COAL]),
                ],
            ),
        );
        assert!(s.converged, "{perm:?}: {} rounds", s.rounds);
        let brass = bound(&s, BRASS);
        assert_eq!(brass.in_l, Q::new(1, 80), "{perm:?}");
        assert_eq!(brass.class, class::BOUNDED);
        assert!(brass.in_u >= brass.in_l);
        assert_eq!(bound(&s, SHEET).out_l, Q::new(1, 64), "{perm:?}");
        // A sequence a 64 ticks, the mechanism rolled at 4/5 (GB111): over the exit run's
        // 2,400-tick window 3 sigma is about 4 a minute, so its 12.6 against 15 passes.
        assert!(close(brass.var_rate, 0.16 / 64.0), "{}", brass.var_rate);
        let sigma = (POISSON_SLOPE_FACTOR * brass.var_rate * 2400.0).sqrt() / 2400.0 * MIN;
        assert!((1.3..1.4).contains(&sigma), "{sigma}");
    }
}

/// Corpus 6's held goods: sheets, the three parts (zinc, copper, coal stand in for them).
fn precision_regime(g: &Graph, [sheets, cogs, large, nug, _]: [u32; 5]) -> Regime {
    regime(
        g,
        &[
            (sheets, &[SHEET]),
            (cogs, &[ZINC]),
            (large, &[COPPER]),
            (nug, &[COAL]),
        ],
    )
}

#[test]
fn an_overfed_sequenced_loop_stalls() {
    // The feeder at 64 RPM (an entry per 8 ticks) under the andesite funnel's sheet per 9 ticks:
    // f = 1/9 a tick against the host's mu = 1/30 (GB72). Every belt slot fills and nothing
    // leaves: L is 0 for every good of the loop, U still the capacity.
    let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
    let reversed: [u32; 19] = std::array::from_fn(|i| 18 - i as u32);
    let rotated: [u32; 19] = std::array::from_fn(|i| ((i + 7) % 19) as u32);
    for perm in [identity, reversed, rotated] {
        let (g, stores) = precision_mechanisms_fed(&perm, 9, 8);
        let rg = precision_regime(&g, stores);
        let s = run(&g, &rg);
        assert!(s.converged, "{perm:?}: {} rounds", s.rounds);
        for good in [BRASS, SHEET, ZINC, COPPER, COAL] {
            let b = bound(&s, good);
            assert_eq!((b.in_l, b.out_l), (Q::ZERO, Q::ZERO), "{perm:?} {good:?}");
            assert_eq!(b.class, class::BOUNDED, "{perm:?} {good:?}");
        }
        // U keeps the loop's capacity, 4/5 of a product per 30 ticks: the pass-through deployers
        // carry it in the LP too (GB72).
        let brass = bound(&s, BRASS);
        assert!(brass.in_u >= Q::new(2, 75), "{perm:?}: U {:?}", brass.in_u);
        assert_eq!(brass.var_rate, 0.0);
        // Switched off (the old section 3.4): the loop settles at the host's capacity, a
        // sheet per 30 ticks, the funnel held back to it.
        let old = solve::solve_with(&g, &rg, &GlassTunables::default(), true, false);
        assert!(old.converged);
        assert_eq!(bound(&old, BRASS).in_l, Q::new(2, 75), "{perm:?}");
        assert_eq!(bound(&old, SHEET).out_l, Q::new(1, 30), "{perm:?}");
    }
}

#[test]
fn a_sequenced_loop_fed_at_its_machines_rate_runs() {
    // f = mu = 1/30 (a funnel a sheet per 30 ticks): the population does not grow, the loop runs
    // at the feed (GB72, f > mu only stalls). 1/31 runs below it, as corpus 6's 1/64 does.
    let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
    for (funnel, brass) in [(30, Q::new(2, 75)), (31, Q::new(4, 155))] {
        let (g, stores) = precision_mechanisms_fed(&identity, funnel, 8);
        let s = run(&g, &precision_regime(&g, stores));
        assert!(s.converged);
        assert_eq!(bound(&s, BRASS).in_l, brass, "funnel {funnel}");
        assert_eq!(bound(&s, SHEET).out_l, Q::new(1, i64::from(funnel)));
    }
}

/// A belt segment M2 where the through traffic from M1 (zinc, `zinc` an entry a tick into M1)
/// meets a side insertion (copper, `copper` a tick): M2 takes an entry per 4 ticks.
fn belt_merge(zinc: u32, copper: u32) -> (Graph, Regime) {
    let mut b = B::default();
    let zs = b.store(0);
    let cs = b.store(2);
    let out = b.store(1);
    let shared = |batch: u32, ticks: u32| RateEntry {
        flags: rate_flags::SHARED,
        ..entry(None, batch, ticks)
    };
    let fz = b.node(
        NodeKind::Mover,
        model::FUNNEL_BRASS,
        0,
        vec![shared(1, zinc)],
    );
    let fc = b.node(
        NodeKind::Mover,
        model::FUNNEL_BRASS,
        0,
        vec![shared(1, copper)],
    );
    let m1 = b.node(NodeKind::Mover, model::BELT, 0, vec![shared(1, 4)]);
    let m2 = b.node(NodeKind::Mover, model::BELT, 0, vec![shared(1, 4)]);
    b.edge(zs, fz);
    b.edge(fz, m1);
    // The belt's own link comes first by index: the rule, not the index, puts it last.
    let link = b.edge_k(EdgeKind::Belt, m1, m2, &[]);
    b.edges[link as usize].flags |= edge_flags::THROUGH;
    b.edge(cs, fc);
    b.edge(fc, m2);
    b.edge(m2, out);
    let g = b.graph();
    let rg = regime(&g, &[(zs, &[ZINC]), (cs, &[COPPER])]);
    (g, rg)
}

#[test]
fn a_side_insertion_goes_before_the_through_traffic() {
    // Saturated: the insertion brings 1/8 and takes it, the through traffic gets the 1/8 left of
    // the segment's 1/4 (canInsertAtFromSide never waits for passing items). Which gaps each gets
    // at saturation is timing: bounded (GB72).
    let (g, rg) = belt_merge(2, 8);
    let s = run(&g, &rg);
    assert!(s.converged);
    let (zinc, copper) = (bound(&s, ZINC), bound(&s, COPPER));
    assert_eq!((zinc.in_l, copper.in_l), (Q::new(1, 8), Q::new(1, 8)));
    assert_eq!((zinc.class, copper.class), (class::BOUNDED, class::BOUNDED));
    assert_eq!(zinc.why.0, why::SHARED_CAPACITY);
    assert!(zinc.in_u >= Q::new(1, 4) && copper.in_u >= Q::new(1, 8));
    // Switched off (the old section 3.3): the main line first, nothing left for the side.
    let old = solve::solve_with(&g, &rg, &GlassTunables::default(), false, true);
    assert_eq!(bound(&old, ZINC).in_l, Q::new(1, 4));
    assert_eq!(bound(&old, COPPER).in_l, Q::ZERO);
}

#[test]
fn a_belt_merge_below_its_capacity_is_exact() {
    // 1/16 through and 1/8 inserted fit in 1/4: both flow in full, in either order.
    let (g, rg) = belt_merge(16, 8);
    for first in [true, false] {
        let s = solve::solve_with(&g, &rg, &GlassTunables::default(), first, true);
        let (zinc, copper) = (bound(&s, ZINC), bound(&s, COPPER));
        assert_eq!((zinc.in_l, copper.in_l), (Q::new(1, 16), Q::new(1, 8)));
        assert_eq!((zinc.class, copper.class), (class::EXACT, class::EXACT));
    }
}

// ---- a large base (the OmegaSrv rehearsal, docs/deploy-rehearsal-omegasrv.md 6.7) ------------------

/// A base shaped like the rehearsal's main one (1,258 nodes, 1,790 edges, 138 multi-input
/// machines): `cells` overflow loops, each a store feeding a belt loop through a funnel, a tunnel
/// on the loop that gives a press what it takes and sends the rest round again, and the press's
/// output funnelled into its own store. Each cell has its own goods, and its feed runs a little
/// above its press, so the loop fills a little each round (`feed` ticks per item against the
/// press's 10).
fn base_shaped(cells: u32, feed: u32) -> B {
    let mut b = B::default();
    let shared = |batch: u32, ticks: u32| RateEntry {
        flags: rate_flags::SHARED,
        ..entry(None, batch, ticks)
    };
    for c in 0..cells {
        let (a, o) = (Good(100 + 2 * c), Good(101 + 2 * c));
        let input = b.store(2 * c);
        let out = b.store(2 * c + 1);
        let r = b.recipe(&[(a, 1)], &[(o, 1)]);
        let f_in = b.node(
            NodeKind::Mover,
            model::FUNNEL_ANDESITE,
            0,
            vec![shared(1, feed)],
        );
        let l1 = b.node(NodeKind::Mover, model::BELT, 0, vec![shared(64, 2)]);
        let j = b.junction(rule::PREFER_FIRST);
        let l2 = b.node(NodeKind::Mover, model::BELT, 0, vec![shared(64, 2)]);
        let p = b.machine(model::PRESS, r, 1, 10);
        let f_out = b.node(
            NodeKind::Mover,
            model::FUNNEL_ANDESITE,
            0,
            vec![shared(1, 2)],
        );
        b.edge(input, f_in);
        b.edge(f_in, l1);
        b.edge(l1, j);
        b.edge(j, p);
        b.edge(j, l2);
        b.edge(l2, l1);
        b.edge(p, f_out);
        b.edge(f_out, out);
    }
    b
}

fn base_regime(b: &B, g: &Graph, cells: u32) -> Regime {
    let held: Vec<(u32, Vec<Good>)> = (0..cells)
        .map(|c| {
            let v = b
                .nodes
                .iter()
                .position(|n| n.inventory == Some(2 * c))
                .expect("store") as u32;
            (v, vec![Good(100 + 2 * c)])
        })
        .collect();
    let held: Vec<(u32, &[Good])> = held.iter().map(|(v, g)| (*v, g.as_slice())).collect();
    regime(g, &held)
}

/// A plain line beside the base, its own part: a store of good 900 (inventory 900), a funnel an item
/// per 20 ticks, a press, a funnel, a store (inventory 901) for good 901. Returns the input store.
fn add_line(b: &mut B) -> u32 {
    let shared = |batch: u32, ticks: u32| RateEntry {
        flags: rate_flags::SHARED,
        ..entry(None, batch, ticks)
    };
    let input = b.store(900);
    let out = b.store(901);
    let r = b.recipe(&[(Good(900), 1)], &[(Good(901), 1)]);
    let f_in = b.node(
        NodeKind::Mover,
        model::FUNNEL_ANDESITE,
        0,
        vec![shared(1, 20)],
    );
    let p = b.machine(model::PRESS, r, 1, 10);
    let f_out = b.node(
        NodeKind::Mover,
        model::FUNNEL_ANDESITE,
        0,
        vec![shared(1, 2)],
    );
    b.edge(input, f_in);
    b.edge(f_in, p);
    b.edge(p, f_out);
    b.edge(f_out, out);
    input
}

#[test]
fn a_base_sized_solve_stops_within_its_work() {
    // The rehearsal's main base ran every solve to its 278-round cap, 2.4 to 5.4 s inside the
    // engine's step (docs/deploy-rehearsal-omegasrv.md 6.7, DR9). Its shape here: 138 over-fed
    // loops, each needing 8,614 rounds to settle (its belt fills at 1/90 an item a round, then
    // the funnel is squeezed back to the press). The solve stops at `glass.solveWorkMax`, a count
    // and not a clock (choice G2-58), and every part, still moving, has L = 0 and is bounded
    // (choice G2-60): at or below the 1/10 a tick each press runs at.
    let cells = 138;
    let b = base_shaped(cells, 9);
    let g = b.graph();
    let rg = base_regime(&b, &g, cells);
    let tun = GlassTunables::default();
    let per = solve::Solver::new(&g, &rg, &tun).round_work();
    let s = solve::solve(&g, &rg, &tun);
    assert_eq!(g.multi_input, cells);
    assert!(!s.converged && s.starved);
    assert_eq!(s.work, u64::from(s.rounds) * per);
    assert!(s.work <= tun.solve_work_max && s.work + per > tun.solve_work_max);
    assert!(s.rounds < tun.rounds_per_multi_input * cells + tun.rounds_base);
    assert!(s.lp_work <= tun.lp_work_max);
    for c in 0..cells {
        let (a, o) = (bound(&s, Good(100 + 2 * c)), bound(&s, Good(101 + 2 * c)));
        assert_eq!((a.class, o.class), (class::BOUNDED, class::BOUNDED));
        assert_eq!((a.out_l, o.in_l), (Q::ZERO, Q::ZERO), "cell {c}");
        assert!(
            a.out_u >= Q::new(1, 10) && o.in_u >= Q::new(1, 10),
            "cell {c}"
        );
    }
    // One cell, given the rounds: the fixed point the bounds hold.
    let one = base_shaped(1, 9);
    let g1 = one.graph();
    let rg1 = base_regime(&one, &g1, 1);
    let all = GlassTunables {
        rounds_base: 20_000,
        solve_work_max: u64::MAX / 4,
        ..GlassTunables::default()
    };
    let full = solve::solve(&g1, &rg1, &all);
    assert!(full.converged && !full.starved, "{} rounds", full.rounds);
    assert_eq!(full.rounds, 8_614);
    assert_eq!(bound(&full, Good(100)).out_l, Q::new(1, 10));
    assert_eq!(bound(&full, Good(101)).in_l, Q::new(1, 10));
}

#[test]
fn a_base_with_more_lps_than_its_work_keeps_the_linear_bound_for_the_rest() {
    // 400 cells: 1,600 goods and directions to bound, each a small LP that costs its set-up
    // (`LP_WORK_FIXED`, choice G2-62). The LP work stops them part way; the goods past it keep
    // the linear bound, which is above the LP's and never below L.
    let cells = 400;
    let b = base_shaped(cells, 9);
    let g = b.graph();
    let rg = base_regime(&b, &g, cells);
    let tun = GlassTunables::default();
    let s = solve::solve(&g, &rg, &tun);
    let all = solve::solve(
        &g,
        &rg,
        &GlassTunables {
            lp_work_max: u64::MAX / 4,
            ..GlassTunables::default()
        },
    );
    assert!(s.lp_work <= tun.lp_work_max);
    assert!(s.lp_skipped > 0 && all.lp_skipped == 0, "{}", s.lp_skipped);
    assert!(s.lp_solves < all.lp_solves);
    assert_eq!((s.rounds, s.work), (all.rounds, all.work));
    for c in 0..cells {
        for good in [Good(100 + 2 * c), Good(101 + 2 * c)] {
            let (a, f) = (bound(&s, good), bound(&all, good));
            assert_eq!((a.class, a.in_l, a.out_l), (f.class, f.in_l, f.out_l));
            assert!(a.in_u >= f.in_u && a.out_u >= f.out_u, "cell {c}");
        }
    }
}

/// `cargo test --release -p rtsim-industry --lib base_solve_cpu -- --ignored --nocapture`: the
/// base-shaped solve's CPU with the work budget and without it (the rounds cap alone, as before
/// choice G2-58), 5 runs each, side by side.
#[test]
#[ignore]
fn base_solve_cpu() {
    let unbounded = GlassTunables {
        solve_work_max: u64::MAX / 4,
        lp_work_max: u64::MAX / 4,
        ..GlassTunables::default()
    };
    for cells in [10u32, 138, 400] {
        let b = base_shaped(cells, 9);
        let g = b.graph();
        let rg = base_regime(&b, &g, cells);
        let mut rows = Vec::new();
        for (name, tun) in [
            ("budget", GlassTunables::default()),
            ("cap only", unbounded.clone()),
        ] {
            let mut t = Vec::new();
            let mut sol = Solution::default();
            for _ in 0..5 {
                let t0 = std::time::Instant::now();
                sol = solve::solve(&g, &rg, &tun);
                t.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            t.sort_by(f64::total_cmp);
            rows.push(format!(
                "{name}: median {:.2} ms (min {:.2}, max {:.2}), rounds {}, work {}, lp work {}",
                t[2], t[0], t[4], sol.rounds, sol.work, sol.lp_work
            ));
        }
        println!(
            "cells {cells} ({} nodes): {}",
            g.nodes.len(),
            rows.join("; ")
        );
    }
}

#[test]
fn an_unconverged_solve_bounds_only_the_parts_still_moving() {
    // A line beside an over-fed loop: the loop stops at `glass.solveRoundsMax` (4 rounds) still
    // filling, the line settled in its first. The line keeps its exact L; the loop's part is
    // bounded at L = 0 (choice G2-60).
    let mut b = base_shaped(1, 9);
    let input = add_line(&mut b);
    let g = b.graph();
    let mut rg = base_regime(&b, &g, 1);
    rg.held[input as usize] = vec![Good(900)];
    let s = run(&g, &rg);
    assert!(!s.converged && !s.starved);
    assert_eq!(s.rounds, 4);
    let (lin, lout) = (bound(&s, Good(900)), bound(&s, Good(901)));
    assert_eq!((lin.class, lout.class), (class::EXACT, class::EXACT));
    assert_eq!((lin.out_l, lout.in_l), (Q::new(1, 20), Q::new(1, 20)));
    let (a, o) = (bound(&s, Good(100)), bound(&s, Good(101)));
    assert_eq!((a.class, o.class), (class::BOUNDED, class::BOUNDED));
    assert_eq!((a.out_l, o.in_l), (Q::ZERO, Q::ZERO));
    // The line alone: the same bounds.
    let mut alone = B::default();
    let input = add_line(&mut alone);
    let g2 = alone.graph();
    let rg2 = regime(&g2, &[(input, &[Good(900)])]);
    let s2 = run(&g2, &rg2);
    assert!(s2.converged);
    assert_eq!(bound(&s2, Good(900)), lin);
    assert_eq!(bound(&s2, Good(901)), lout);
}

#[test]
fn an_overfed_loop_probe_out_of_work_leaves_its_part_unsettled() {
    // GB72's stall check runs a probe solve once L settles. With work for L's rounds and none
    // for the probe, whether the loop jams is not known: the solve is not converged and the
    // loop's part has L = 0 (choice G2-60), where the full solve finds the jam (L = 0) too.
    let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
    let (g, stores) = precision_mechanisms_fed(&identity, 9, 8);
    let rg = precision_regime(&g, stores);
    let free = solve::solve_with(&g, &rg, &GlassTunables::default(), true, false);
    assert!(free.converged);
    let tun = GlassTunables {
        solve_work_max: free.work,
        ..GlassTunables::default()
    };
    let s = solve::solve(&g, &rg, &tun);
    assert!(!s.converged && s.starved);
    assert_eq!(s.rounds, free.rounds);
    for good in [BRASS, SHEET, ZINC, COPPER, COAL] {
        let b = bound(&s, good);
        assert_eq!((b.in_l, b.out_l), (Q::ZERO, Q::ZERO), "{good:?}");
        assert_eq!(b.class, class::BOUNDED, "{good:?}");
    }
    // With the work: the jam itself.
    let full = run(&g, &rg);
    assert!(full.converged && full.work > free.work);
}

// ---- the graph store (F5) and the engine path -------------------------------------------------------

fn glass_with(records: &[GlassObs]) -> Glass {
    let mut glass = Glass::default();
    let goods = goods();
    let tun = GlassTunables::default();
    for r in records {
        glass.ingest(Tick(0), r, None, &goods, &tun);
    }
    glass
}

#[test]
fn a_compile_is_accepted_only_with_every_record() {
    let (b, _) = press_line();
    let recs = b.records(1);
    // One edge dropped: the counts do not match.
    let mut cut = recs.clone();
    cut.remove(b.nodes.len());
    let glass = glass_with(&cut);
    assert!(glass.report(D).is_none());
    assert_eq!(glass.stats.compiles_dropped, 1);
    // Complete: accepted and solved once.
    let glass = glass_with(&recs);
    let r = glass.report(D).expect("accepted");
    assert_eq!(r.compile, 1);
    assert_eq!(r.nodes, 3);
    assert_eq!(glass.stats.compiles_accepted, 1);
    assert_eq!(glass.stats.solves, 1);
}

#[test]
fn a_resend_of_the_held_compile_is_ignored_and_a_newer_one_replaces_it() {
    let (b, _) = press_line();
    let mut recs = b.records(1);
    let mut resend = b.records(1);
    if let Some(GlassObs::End(e)) = resend.last_mut() {
        e.flags |= end_flags::RESEND;
    }
    recs.extend(resend);
    let glass = glass_with(&recs);
    assert_eq!(
        glass.stats,
        GlassStats {
            compiles_accepted: 1,
            resends_ignored: 1,
            solves: 1,
            ..GlassStats::default()
        }
    );
    let mut recs2 = recs.clone();
    recs2.extend(b.records(2));
    let glass = glass_with(&recs2);
    assert_eq!(glass.report(D).map(|r| r.compile), Some(2));
    // An older compile's records after it are ignored.
    let mut recs3 = recs2.clone();
    recs3.extend(b.records(1));
    let glass = glass_with(&recs3);
    assert_eq!(glass.report(D).map(|r| r.compile), Some(2));
    assert_eq!(glass.stats.compiles_stale, 1);
}

#[test]
fn a_not_compilable_end_drops_the_held_compile() {
    let (b, _) = press_line();
    let mut recs = b.records(1);
    let mut end = b.end(2);
    end.flags = end_flags::NOT_COMPILABLE;
    end.reason = not_compilable::TOO_BIG;
    recs.push(GlassObs::End(end));
    let glass = glass_with(&recs);
    assert!(glass.report(D).is_none());
    assert_eq!(
        glass.districts[&D].not_compilable,
        Some((2, not_compilable::TOO_BIG))
    );
}

mod engine_path {
    use super::*;
    use crate::cmd::Cmd;
    use crate::engine::Engine;
    use crate::glass::report::discard;
    use crate::obs::{DistrictDecl, Obs, SweepEnd, Timed};
    use crate::tunables::Tunables;

    /// One sweep's records at `t`: both stores, then the END.
    fn push_sweep(out: &mut Vec<Timed>, k: u32, t: u64, made: i64) {
        for (i, c) in [(0, (BRASS, 100_000 - made)), (1, (SHEET, made))] {
            out.push(Timed {
                at: Tick(t),
                obs: Obs::StockSample {
                    district: D,
                    inventory: i,
                    sweep: k,
                    goods: GoodVec::from_pairs([c]),
                },
            });
        }
        out.push(Timed {
            at: Tick(t),
            obs: Obs::SweepEnd(SweepEnd {
                district: D,
                sweep: k,
                records: 2,
                inventories: 2,
                live: true,
                creative: false,
                overstressed: false,
            }),
        });
    }

    /// The district and its two inventories at 0.
    fn district() -> Vec<Timed> {
        let mut out = vec![Timed {
            at: Tick(0),
            obs: Obs::District(DistrictDecl {
                district: D,
                revision: 1,
                dim: 0,
                name: 0,
                not_learnable: None,
                inventory_count: 2,
            }),
        }];
        for i in 0..2 {
            out.push(Timed {
                at: Tick(0),
                obs: Obs::Inventory(inventory(i, &[]).decl),
            });
        }
        out
    }

    /// The records Java would send: the district, its inventories, the compile, then a sweep every
    /// 100 ticks in which the press makes half what the compile predicts (one sweep creative).
    fn stream() -> Vec<Timed> {
        let (b, _) = press_line();
        let mut out = vec![Timed {
            at: Tick(0),
            obs: Obs::District(DistrictDecl {
                district: D,
                revision: 1,
                dim: 0,
                name: 0,
                not_learnable: None,
                inventory_count: 2,
            }),
        }];
        for i in 0..2 {
            out.push(Timed {
                at: Tick(0),
                obs: Obs::Inventory(inventory(i, &[]).decl),
            });
        }
        for r in b.records(1) {
            out.push(Timed {
                at: Tick(0),
                obs: Obs::Glass(r),
            });
        }
        for k in 1..=40u32 {
            let t = u64::from(k) * 100;
            let made = (t / 20) as i64;
            for (i, c) in [(0, (BRASS, 100_000 - made)), (1, (SHEET, made))] {
                out.push(Timed {
                    at: Tick(t),
                    obs: Obs::StockSample {
                        district: D,
                        inventory: i,
                        sweep: k,
                        goods: GoodVec::from_pairs([c]),
                    },
                });
            }
            out.push(Timed {
                at: Tick(t),
                obs: Obs::SweepEnd(SweepEnd {
                    district: D,
                    sweep: k,
                    records: 2,
                    inventories: 2,
                    live: true,
                    creative: k == 20,
                    overstressed: false,
                }),
            });
        }
        out
    }

    fn replay(stream: &[Timed]) -> (Engine, Vec<Cmd>) {
        let mut e = Engine::new(0, Tunables::default(), Tick::ZERO);
        for (g, info) in goods() {
            e.define_good(g, info);
        }
        e.session_start(Tick(0));
        let mut cmds = Vec::new();
        for t in stream {
            if t.at > e.now() {
                e.advance_to(t.at);
            }
            e.ingest(t);
            cmds.extend(e.drain_commands());
        }
        (e, cmds)
    }

    #[test]
    fn the_engine_checks_the_compile_and_asks_for_probes() {
        let (e, cmds) = replay(&stream());
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(r.windows_completed, 1);
        assert_eq!(r.verdict, verdict::FAIL);
        let probes: Vec<&ProbeSet> = cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::ProbeSet(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].district, D);
        assert_eq!(probes[0].compile, 1);
    }

    #[test]
    fn the_same_records_give_the_same_report() {
        let s = stream();
        let (a, _) = replay(&s);
        let (b, _) = replay(&s);
        let mut ra = a.glass.report(D).expect("a");
        let mut rb = b.glass.report(D).expect("b");
        ra.solve_micros = 0.0;
        rb.solve_micros = 0.0;
        assert_eq!(ra.to_f64s(), rb.to_f64s());
        assert_eq!(ra, rb);
    }

    #[test]
    fn a_sweep_that_is_not_live_discards_the_window() {
        let mut s = stream();
        for t in s.iter_mut() {
            if let Obs::SweepEnd(e) = &mut t.obs
                && e.sweep == 25
            {
                e.live = false;
            }
        }
        let (e, _) = replay(&s);
        let r = e.glass.report(D).expect("a compile");
        // The window that began at 1,200 was discarded at 2,500; the next starts after a new
        // warm-up (3,700) and is still open at 4,000.
        assert_eq!(r.windows_completed, 0);
        assert_eq!(r.window_start, 3700);
        assert_eq!(r.discards[discard::NOT_LIVE as usize], 1);
        assert_eq!(r.last_discard, Some((2500, discard::NOT_LIVE)));
        assert_eq!(r.ready_at, 3700);
    }

    #[test]
    fn a_sweep_with_dropped_records_is_skipped_and_the_window_goes_on() {
        // Every 1,200 ticks a sweep loses a record (as a compile resend filling the link's buffer
        // would). When each discarded the window, the next warm-up ended just as the next drop
        // came, and no window ever opened. Now a skip: the window integrates across it.
        let mut s = stream();
        for t in s.iter_mut() {
            if let Obs::SweepEnd(e) = &mut t.obs
                && e.sweep % 12 == 0
            {
                e.records += 1;
            }
        }
        // Sweep 30's samples never arrive at all.
        s.retain(|t| !matches!(&t.obs, Obs::StockSample { sweep: 30, .. }));
        let (e, _) = replay(&s);
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(r.windows_completed, 1);
        assert_eq!(r.window_start, 1300);
        assert_eq!(r.discards[discard::DROPPED as usize], 3);
        assert_eq!(r.discards[discard::NO_SWEEP as usize], 1);
        assert_eq!(r.last_discard, Some((3600, discard::DROPPED)));
        // 40 sweeps less the 4 skipped; sampled from 1,300 (sweep 13) on, less 3 skipped.
        assert_eq!((r.sweeps, r.sweeps_sampled), (36, 25));
        let v = r.to_f64s();
        assert_eq!(&v[24..26], &[3600.0, f64::from(discard::DROPPED)]);
        assert_eq!(&v[26..28], &[1.0, 3.0]);
    }

    #[test]
    fn a_recompile_starts_a_fresh_window_from_its_own_sweeps() {
        // Compile 1 predicts a sheet per 10 ticks, and the press makes that until a player slows it
        // to one per 20 at 2,050 (compile 2). Compile 1's window (from 1,200) goes with it; compile
        // 2's starts after its own warm-up and sees only its own sweeps, so it passes.
        let (b, p) = press_line();
        let mut slow = b.clone();
        slow.nodes[p as usize].rates = vec![entry(Some(0), 1, 20)];
        let made = |t: u64| {
            if t <= 2050 {
                (t / 10) as i64
            } else {
                205 + ((t - 2050) / 20) as i64
            }
        };
        let mut s = district();
        for r in b.records(1) {
            s.push(Timed {
                at: Tick(0),
                obs: Obs::Glass(r),
            });
        }
        for k in 1..=54u32 {
            let t = u64::from(k) * 100;
            if t == 2100 {
                for r in slow.records(2) {
                    s.push(Timed {
                        at: Tick(2050),
                        obs: Obs::Glass(r),
                    });
                }
            }
            push_sweep(&mut s, k, t, made(t));
        }
        let (e, _) = replay(&s);
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(r.compile, 2);
        assert_eq!((r.accepted_at, r.ready_at), (2050, 3250));
        assert_eq!(r.windows_completed, 1);
        assert_eq!(r.window_start, 3300);
        assert_eq!(r.verdict, verdict::PASS);
        assert_eq!(r.discards[discard::RECOMPILE as usize], 1);
        // Compile 2 took the sweeps from 2,100 on, and sampled from 3,300.
        assert_eq!((r.sweeps, r.sweeps_sampled), (34, 22));
        let sheet = r.goods.iter().find(|g| g.good == SHEET.0).expect("sheet");
        assert!(close(sheet.predicted, 0.05));
    }
}

// ---- U is never below L (G-B15) ----------------------------------------------------------------------

/// A random DAG of junctions, movers, buffers, machines and belts between an input store and an
/// output store. A belt's edges from earlier middle nodes are its through traffic (THROUGH) unless
/// filtered, so belts see side insertions (from the input store) merging into them (GB72).
fn random_dag(
    kinds: &[(u8, u8, u8)],
    edges: &[(usize, usize, bool)],
    held: &[bool; 3],
) -> (Graph, Regime) {
    let mut b = B::default();
    let input = b.store(0);
    let rs = [
        b.recipe(&[(ZINC, 1)], &[(BRASS, 1)]),
        b.recipe(&[(ZINC, 1), (COPPER, 1)], &[(BRASS, 2)]),
        b.recipe(&[(BRASS, 1)], &[(SHEET, 1), (ZINC, 1)]),
    ];
    let rules = [
        rule::NONE,
        rule::SPLIT,
        rule::FORCED_SPLIT,
        rule::PREFER_NEAREST,
        rule::ROUND_ROBIN,
        rule::RANDOMIZE,
        rule::ANDESITE_SPLIT,
    ];
    let mut mid = Vec::new();
    for &(k, a, t) in kinds {
        let ticks = 1 + u32::from(t % 40);
        let v = match k % 5 {
            0 => b.junction(rules[a as usize % rules.len()]),
            1 => b.node(
                NodeKind::Mover,
                model::FUNNEL_BRASS,
                0,
                vec![RateEntry {
                    flags: rate_flags::SHARED,
                    ..entry(None, 1 + u32::from(a % 4), ticks)
                }],
            ),
            2 => b.node(NodeKind::Buffer, model::DEPOT, 0, Vec::new()),
            4 => b.node(
                NodeKind::Mover,
                model::BELT,
                0,
                vec![RateEntry {
                    flags: rate_flags::SHARED,
                    ..entry(None, 1 + u32::from(a % 4), ticks)
                }],
            ),
            _ => {
                let m = b.machine(model::PRESS, rs[a as usize % 3], 1, ticks);
                if a % 2 == 0 {
                    b.nodes[m as usize].flags = 0;
                }
                m
            }
        };
        mid.push(v);
    }
    let out = b.store(1);
    let n = mid.len();
    // Every middle node has a way in and a way out.
    for (i, &v) in mid.iter().enumerate() {
        if i == 0 || i % 3 == 0 {
            b.edge(input, v);
        }
        if i + 1 == n || i % 2 == 1 {
            b.edge(v, out);
        }
    }
    for &(a, c, filt) in edges {
        let (a, c) = (a % n, c % n);
        if a < c {
            if filt {
                b.edge_k(EdgeKind::Adjacency, mid[a], mid[c], &[BRASS]);
            } else {
                let e = b.edge(mid[a], mid[c]);
                if b.nodes[mid[c] as usize].model == model::BELT {
                    b.edges[e as usize].flags |= edge_flags::THROUGH;
                }
            }
        }
    }
    let hg: Vec<Good> = [ZINC, COPPER, BRASS]
        .iter()
        .zip(held)
        .filter(|(_, h)| **h)
        .map(|(g, _)| *g)
        .collect();
    let g = b.graph();
    let rg = regime(&g, &[(input, &hg)]);
    (g, rg)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    #[test]
    fn u_is_never_below_l_on_random_dags(
        kinds in prop::collection::vec((0u8..10, 0u8..16, 0u8..64), 1..10),
        edges in prop::collection::vec((0usize..10, 0usize..10, any::<bool>()), 0..16),
        held in prop::array::uniform3(any::<bool>()),
    ) {
        let (g, rg) = random_dag(&kinds, &edges, &held);
        let s = run(&g, &rg);
        for b in &s.goods {
            prop_assert!(b.in_u >= b.in_l && b.out_u >= b.out_l);
            // Stopped at `glass.solveRoundsMax`, the parts still moving have L = 0 and the rest
            // are at their fixed point (choice G2-60): the relaxation and the LP must contain L's
            // flow either way.
            prop_assert!(b.in_u_lin >= b.in_l, "linear in {:?} < L {:?}", b.in_u_lin, b.in_l);
            prop_assert!(b.out_u_lin >= b.out_l, "linear out {:?} < L {:?}", b.out_u_lin, b.out_l);
            let l_in = b.in_l.to_f64();
            let l_out = b.out_l.to_f64();
            if let Some(x) = b.in_u_lp {
                prop_assert!(x >= l_in - 1e-9 * (1.0 + l_in), "LP in {x} < L {l_in}");
            }
            if let Some(x) = b.out_u_lp {
                prop_assert!(x >= l_out - 1e-9 * (1.0 + l_out), "LP out {x} < L {l_out}");
            }
        }
        // Conservation: every mover, junction and buffer passes on what it takes (a belt merge
        // included, GB72); unconverged, a part still moving takes and passes nothing.
        {
            for (v, node) in g.nodes.iter().enumerate() {
                if !matches!(node.kind, NodeKind::Mover | NodeKind::Junction | NodeKind::Buffer) {
                    continue;
                }
                let flow = |into: bool| -> f64 {
                    g.edges
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| if into { e.to } else { e.from } as usize == v)
                        .map(|(i, _)| s.edge_flow[i])
                        .sum()
                };
                let (i, o) = (flow(true), flow(false));
                prop_assert!((i - o).abs() <= 1e-9 * (1.0 + i), "node {v}: in {i} out {o}");
            }
        }
    }
}

proptest! {
    /// A solve cut short by its work (one round) never has an L above the full solve's: the parts
    /// that settled are at the same fixed point, the rest at 0 (choices G2-58, G2-60); and U
    /// stays above the full L.
    #[test]
    fn a_solve_out_of_work_is_never_above_the_full_one(
        kinds in prop::collection::vec((0u8..10, 0u8..16, 0u8..64), 1..10),
        edges in prop::collection::vec((0usize..10, 0usize..10, any::<bool>()), 0..16),
        held in prop::array::uniform3(any::<bool>()),
    ) {
        let (g, rg) = random_dag(&kinds, &edges, &held);
        let full = run(&g, &rg);
        prop_assume!(full.converged);
        let short = GlassTunables { solve_work_max: 1, ..GlassTunables::default() };
        let s = solve::solve(&g, &rg, &short);
        prop_assert_eq!(s.rounds, 1);
        prop_assert!(s.converged || s.starved || full.rounds == 1);
        for b in &s.goods {
            let f = full.goods.iter().find(|x| x.good == b.good).expect("same goods");
            prop_assert!(b.in_l <= f.in_l, "{:?}: in L {:?} above {:?}", b.good, b.in_l, f.in_l);
            prop_assert!(b.out_l <= f.out_l, "{:?}: out L {:?} above {:?}", b.good, b.out_l, f.out_l);
            prop_assert!(b.in_u >= f.in_l && b.out_u >= f.out_l, "{:?}: U below the full L", b.good);
            if !s.converged {
                prop_assert!(b.class >= f.class || b.in_l == f.in_l && b.out_l == f.out_l);
            }
        }
    }
}

// ---- the glass replay (G2, GLASSBOX.md §4) -------------------------------------------------------

mod replay_path {
    use super::*;
    use crate::cmd::Cmd;
    use crate::engine::Engine;
    use crate::glass::check::WindowResult;
    use crate::glass::report::replay_model;
    use crate::obs::{DistrictDecl, EditKind, LoadState, Obs, SweepEnd, Timed};
    use crate::tunables::Tunables;

    fn at(t: u64, obs: Obs) -> Timed {
        Timed { at: Tick(t), obs }
    }

    /// The record at its time, then a save for the page check (`Engine::check_save`, ID5).
    fn feed(e: &mut Engine, t: &Timed) -> Vec<Cmd> {
        if t.at > e.now() {
            e.advance_to(t.at);
        }
        e.ingest(t);
        let cmds = e.drain_commands();
        e.check_save();
        cmds
    }

    /// The district, its inventories (live) and the compile at 0.
    fn setup(b: &B, invs: u32) -> Vec<Timed> {
        setup_n(b, invs, 1)
    }

    /// [`setup`] with the compile numbered `compile`.
    fn setup_n(b: &B, invs: u32, compile: u32) -> Vec<Timed> {
        let mut out = vec![at(
            0,
            Obs::District(DistrictDecl {
                district: D,
                revision: 1,
                dim: 0,
                name: 0,
                not_learnable: None,
                inventory_count: invs,
            }),
        )];
        for i in 0..invs {
            out.push(at(0, Obs::Inventory(inventory(i, &[]).decl)));
        }
        out.push(load_state(0, LoadState::Live));
        for r in b.records(compile) {
            out.push(at(0, Obs::Glass(r)));
        }
        out
    }

    /// One live sweep at `t` of every inventory's contents.
    fn sweep(out: &mut Vec<Timed>, k: u32, t: u64, contents: &[&[(Good, i64)]]) {
        for (i, c) in contents.iter().enumerate() {
            out.push(at(
                t,
                Obs::StockSample {
                    district: D,
                    inventory: i as u32,
                    sweep: k,
                    goods: GoodVec::from_pairs(c.iter().copied()),
                },
            ));
        }
        out.push(at(
            t,
            Obs::SweepEnd(SweepEnd {
                district: D,
                sweep: k,
                records: contents.len() as u32,
                inventories: contents.len() as u32,
                live: true,
                creative: false,
                overstressed: false,
            }),
        ));
    }

    fn load_state(t: u64, state: LoadState) -> Timed {
        at(
            t,
            Obs::LoadState {
                district: D,
                state,
                ticking_chunks: u32::from(state == LoadState::Live),
                total_chunks: 1,
            },
        )
    }

    fn engine() -> Engine {
        let mut e = Engine::new(0, Tunables::default(), Tick::ZERO);
        for (g, info) in goods() {
            e.define_good(g, info);
        }
        e.session_start(Tick(0));
        e
    }

    /// Marks the compile as passed (a window with nothing to learn): the replay's own mechanics,
    /// without driving a window that matches every regime.
    fn pass(e: &mut Engine) {
        let c = e
            .glass
            .districts
            .get_mut(&D)
            .and_then(|g| g.compiled.as_mut())
            .expect("a compile");
        c.passed = Some(WindowResult {
            start: Tick(0),
            ticks: 1,
            verdict: verdict::PASS,
            goods: Vec::new(),
            stores: Vec::new(),
        });
        c.edited = false;
    }

    fn deltas(cmds: &[Cmd]) -> BTreeMap<(u32, Good), i64> {
        let mut out = BTreeMap::new();
        for c in cmds {
            if let Cmd::ChunkReconcile(r) = c {
                for d in &r.deltas {
                    *out.entry((d.inventory, d.good)).or_default() += d.delta;
                }
            }
        }
        out
    }

    /// A press that passed its window replays on the glass model while away, stops when its brass
    /// runs out, and the load lands each store's change in that store.
    #[test]
    fn a_passed_compile_replays_its_press_until_the_brass_runs_out() {
        let (b, _) = press_line();
        let mut s = setup(&b, 2);
        // The press makes a sheet per 10 ticks, as compiled: the window from 1,200 passes.
        for k in 1..=35u32 {
            let t = u64::from(k) * 100;
            let m = (t / 10) as i64;
            sweep(&mut s, k, t, &[&[(BRASS, 1000 - m)], &[(SHEET, m)]]);
        }
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        let r = e.glass.report(D).expect("a compile");
        assert!(r.replay.eligible, "a window passed: {:?}", r.verdict);
        // 650 brass and 350 sheets at 3,500, when the district unloads.
        feed(&mut e, &load_state(3500, LoadState::Unloaded));
        e.advance_to(Tick(3501));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(r.replay.model, replay_model::GLASS);
        assert_eq!(r.replay.since, Some(3500));
        // 3,000 ticks later: 300 more sheets, never more. The press is held a cycle (the item it
        // held at the unload is still in it, G2-6), a brass may be in it, and a unit either way
        // goes to the rates' rounding.
        e.advance_to(Tick(6500));
        let st = e.sim_stock(D).expect("replaying");
        assert!((648..=650).contains(&st.get(SHEET)), "{st:?}");
        assert!((349..=351).contains(&st.get(BRASS)), "{st:?}");
        // Long after the brass ran out, the press stopped where the brass did.
        e.advance_to(Tick(20_000));
        let st = e.sim_stock(D).expect("replaying");
        assert_eq!(st.get(BRASS), 0, "{st:?}");
        assert!((999..=1000).contains(&st.get(SHEET)), "{st:?}");
        let r = e.glass.report(D).expect("a compile");
        assert!(r.replay.breakpoints >= 1);
        assert_eq!(r.replay.oscillations, 0);
        // Back: each store's change lands in its own store.
        let mut cmds = feed(&mut e, &load_state(20_000, LoadState::Live));
        cmds.extend(feed(
            &mut e,
            &at(
                20_000,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: BlockPos::default().chunk(0),
                    applied_epoch: 0,
                },
            ),
        ));
        let d = deltas(&cmds);
        assert_eq!(d.get(&(0, BRASS)), Some(&-650), "{d:?}");
        let made = d.get(&(1, SHEET)).copied().unwrap_or(0);
        assert!((649..=650).contains(&made), "{d:?}");
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(r.replay.model, replay_model::NONE);
        e.check_books().expect("books");
    }

    /// A package written into the brass store's offline buffer while away is the world's own
    /// change: the press uses it, and the load writes the store's change once (choice G2-14).
    #[test]
    fn a_package_into_a_replaying_store_feeds_it_and_is_written_once() {
        use crate::obs::{Direction, PackageEvent};
        let (b, _) = press_line();
        let mut s = setup(&b, 2);
        for k in 1..=35u32 {
            let t = u64::from(k) * 100;
            let m = (t / 10) as i64;
            sweep(&mut s, k, t, &[&[(BRASS, 1000 - m)], &[(SHEET, m)]]);
        }
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        feed(&mut e, &load_state(3500, LoadState::Unloaded));
        e.advance_to(Tick(4000));
        assert_eq!(model(&e), replay_model::GLASS);
        feed(
            &mut e,
            &at(
                4000,
                Obs::Package(PackageEvent {
                    district: D,
                    inventory: 0,
                    direction: Direction::In,
                    address: 0,
                    packages: 1,
                    op: None,
                    goods: GoodVec::from_pairs([(BRASS, 64)]),
                }),
            ),
        );
        assert_eq!(model(&e), replay_model::GLASS);
        // 650 + 64 brass at a sheet per 10 ticks: gone by about 11,000.
        e.advance_to(Tick(20_000));
        let st = e.sim_stock(D).expect("replaying");
        assert_eq!(st.get(BRASS), 0, "{st:?}");
        assert!((1063..=1064).contains(&st.get(SHEET)), "{st:?}");
        let mut cmds = feed(&mut e, &load_state(20_000, LoadState::Live));
        cmds.extend(feed(
            &mut e,
            &at(
                20_000,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: BlockPos::default().chunk(0),
                    applied_epoch: 0,
                },
            ),
        ));
        // The world holds the 650 and the package: the load takes all 714 and adds the sheets.
        let d = deltas(&cmds);
        assert_eq!(d.get(&(0, BRASS)), Some(&-714), "{d:?}");
        let made = d.get(&(1, SHEET)).copied().unwrap_or(0);
        assert!((713..=714).contains(&made), "{d:?}");
        e.check_books().expect("books");
    }

    fn model(e: &Engine) -> u32 {
        e.glass.report(D).map_or(0, |r| r.replay.model)
    }

    /// No window passed yet: the compiled district freezes while away (G-B4), with no replay.
    #[test]
    fn a_compile_with_no_passed_window_freezes_while_away() {
        let (b, _) = press_line();
        let mut s = setup(&b, 2);
        for k in 1..=10u32 {
            let t = u64::from(k) * 100;
            let m = (t / 10) as i64;
            sweep(&mut s, k, t, &[&[(BRASS, 1000 - m)], &[(SHEET, m)]]);
        }
        s.push(load_state(1000, LoadState::Unloaded));
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        e.advance_to(Tick(5000));
        assert!(e.sim_stock(D).is_none());
        let r = e.glass.report(D).expect("a compile");
        assert!(!r.replay.eligible);
        assert_eq!(r.replay.model, replay_model::NONE);
    }

    /// Brass to a press to a store of sheets a consumer drains (1 per 20 ticks); a threshold switch
    /// on the sheets stops the press at 100 and restarts it at 20.
    fn switched_line() -> B {
        switched_line_with(10, 20, 100, 20)
    }

    /// The switched line with a press of `press` ticks, a consumer of `consume` ticks, and the
    /// switch on at `hi` sheets and off at `lo`.
    fn switched_line_with(press: u32, consume: u32, hi: i64, lo: i64) -> B {
        let (mut b, p) = press_line();
        b.nodes[p as usize].rates[0].ticks_num = press;
        let coal = b.store(2);
        let burn = b.recipe(&[(SHEET, 1)], &[(COAL, 1)]);
        let m = b.machine(model::PRESS, burn, 1, consume);
        b.edge(1, m);
        b.edge(m, coal);
        let ctl = b.node(NodeKind::Controller, model::THRESHOLD_SWITCH, 0, Vec::new());
        b.nodes[ctl as usize].params = [hi, lo, 0, 80];
        b.edge_k(EdgeKind::Observe, 1, ctl, &[]);
        let e = b.edge_k(EdgeKind::Control, ctl, p, &[]);
        b.edges[e as usize].param = CONTROL_STOP_WHEN_ON;
        b
    }

    /// The switched line tick by tick, as Create runs it: (brass used, coal made) after `ticks`.
    fn switched_reference(sheets0: i64, ticks: u64) -> (i64, i64) {
        let r = switched_reference_lat((10, 20, 100, 20), sheets0, ticks, 0);
        (r.0, r.1)
    }

    /// The switch cycles the sheets between 20 and 100 with a period of about 3,200 ticks. The
    /// replay finds the cycle and runs it in closed form: a few breakpoints for a hundred periods,
    /// and never more brass pressed than the tick-by-tick reference.
    #[test]
    fn a_threshold_switch_cycle_runs_in_closed_form_and_never_above_the_control() {
        let b = switched_line();
        let mut s = setup(&b, 3);
        sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, 80)], &[]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        let span = 320_000;
        // Conservation on the way: nothing made from nothing, at most a few units in machines.
        for t in (200..200 + span).step_by(397) {
            e.advance_to(Tick(t));
            let st = e.sim_stock(D).expect("replaying");
            let sum = st.get(BRASS) + st.get(SHEET) + st.get(COAL);
            assert!((100_080 - 16..=100_080).contains(&sum), "at {t}: {st:?}");
        }
        e.advance_to(Tick(200 + span));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert!(
            r.replay.breakpoints < 40,
            "a hundred periods in {} breakpoints",
            r.replay.breakpoints
        );
        assert!(
            e.glass.districts[&D]
                .replay
                .as_ref()
                .is_some_and(|x| x.counts().cycles >= 1)
        );
        let st = e.sim_stock(D).expect("replaying");
        let used = 100_000 - st.get(BRASS);
        let coal = st.get(COAL);
        let (ref_used, ref_coal) = switched_reference(80, span);
        // The consumer never runs above the control: what it made is at most what Create made, and
        // the item in its cycle (Create counts it when the cycle ends, the replay's continuous
        // output as it goes: choice G2-21).
        assert!(coal <= ref_coal + 1, "made {coal}, reference {ref_coal}");
        assert!(
            coal as f64 >= ref_coal as f64 * 0.99,
            "made {coal}, reference {ref_coal}"
        );
        // The controller's bound (§4.3): over time the press feeds what the consumer takes, plus
        // at most the switch's band in the store. Within 1% of Create's own count.
        assert!(used <= coal + (100 - 80) + 2, "pressed {used}, made {coal}");
        assert!(
            (used - ref_used).abs() as f64 <= ref_used as f64 * 0.01,
            "pressed {used}, reference {ref_used}"
        );
        e.check_books().expect("books");
    }

    /// An edit while away freezes the glass replay: the compile no longer matches the blocks.
    #[test]
    fn an_edit_stops_the_glass_replay() {
        let b = switched_line();
        let mut s = setup(&b, 3);
        sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, 80)], &[]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        e.advance_to(Tick(1000));
        assert!(e.sim_stock(D).is_some());
        feed(
            &mut e,
            &at(
                1000,
                Obs::Edit {
                    district: D,
                    kind: EditKind::Broken,
                    player: (0, 0),
                    pos: BlockPos::default(),
                },
            ),
        );
        e.advance_to(Tick(1001));
        assert!(e.sim_stock(D).is_none(), "frozen after the edit");
        assert!(!e.glass.report(D).expect("a compile").replay.eligible);
    }

    /// The same records give the same replay whoever reads it when (the composition rule, O4):
    /// reading the stock at many ticks on the way changes nothing at the end.
    #[test]
    fn reads_on_the_way_do_not_change_the_replay() {
        let run = |reads: &[u64]| {
            let b = switched_line();
            let mut s = setup(&b, 3);
            sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, 80)], &[]]);
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            pass(&mut e);
            feed(&mut e, &load_state(200, LoadState::Unloaded));
            for &t in reads {
                e.advance_to(Tick(t));
                e.sim_stock(D);
            }
            e.advance_to(Tick(50_000));
            (
                e.sim_stock(D),
                e.glass.report(D).map(|r| r.replay.breakpoints),
            )
        };
        let a = run(&[]);
        let b = run(&[333, 1601, 1777, 3210, 9999, 17_003, 32_100, 40_000]);
        assert_eq!(a, b);
    }

    /// A blaze burner on 1,593 ticks of burn takes coal from the mixer's input store (one per
    /// 1,600 ticks once its burn falls to 500) while away: its phases are the replay's timers, the
    /// coal runs out, the burner goes out 500 ticks later, and the mixer stops there. Create's
    /// count: lit to 1,593 + 16 x 1,600 = 27,193, a craft of 2 brass per 17 ticks.
    #[test]
    fn a_heater_burns_its_coal_while_away_then_goes_out_and_the_mixer_stops() {
        let b = burning_mixer([1593, i64::from(heat::KINDLED), 1600, 500], true);
        let mut s = setup(&b, 2);
        sweep(
            &mut s,
            1,
            100,
            &[&[(ZINC, 10_000), (COPPER, 10_000), (COAL, 16)], &[]],
        );
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        let crafts = |t: u64| 2 * (t.min(27_193) / 17 - 200 / 17) as i64;
        let mut seen = Vec::new();
        for t in [1_000u64, 20_000, 26_000, 30_000, 60_000] {
            e.advance_to(Tick(t));
            let st = e.sim_stock(D).expect("replaying");
            seen.push((t, st.get(BRASS), st.get(COAL), st.get(ZINC)));
        }
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        for &(t, brass, coal, zinc) in &seen {
            // Never above Create, at most a craft and a tick per breakpoint below.
            let c = crafts(t);
            assert!(
                brass <= c + 1 && brass >= c - 6,
                "at {t}: {brass} vs {c}: {seen:?}"
            );
            // Coal: one when the burn reaches 500 at 1,093, then one per 1,600 ticks.
            let took = (t.saturating_sub(1093).div_ceil(1600) as i64).min(16);
            assert!(
                (coal - (16 - took)).abs() <= 1,
                "at {t}: {coal} coal: {seen:?}"
            );
            // The recipe: a zinc per 2 brass, nothing made from nothing.
            assert!(
                zinc + brass / 2 <= 10_000 && zinc + brass / 2 >= 9_998,
                "{seen:?}"
            );
        }
        // Out by 27,193: the mixer made nothing more.
        assert_eq!(seen[3].1, seen[4].1, "{seen:?}");
        assert_eq!(seen[4].2, 0, "{seen:?}");
        assert!(r.replay.breakpoints >= 3, "{}", r.replay.breakpoints);
        e.check_books().expect("books");
    }

    /// The switched line with the press's output on a belt `lat` ticks long, tick by tick as
    /// Create runs it: (brass used, coal made, the fewest sheets the store held, switch flips).
    /// Each machine takes an item when it is idle and has one, and gives its output a cycle later
    /// (a press on a belt starts when an item reaches it); the switch stops the press taking.
    fn switched_reference_lat(
        line: (u64, u64, i64, i64),
        sheets0: i64,
        ticks: u64,
        lat: u64,
    ) -> (i64, i64, i64, i64) {
        let (press, consume, hi, lo) = line;
        let (mut sheets, mut used, mut coal, mut flips) = (sheets0, 0i64, 0i64, 0i64);
        let mut on = sheets0 >= hi;
        let mut belt = std::collections::VecDeque::new();
        let mut least = sheets0;
        // Ticks left of each machine's cycle (None: idle).
        let (mut p, mut c): (Option<u64>, Option<u64>) = (None, None);
        for t in 1..=ticks {
            if let Some(r) = p {
                p = (r > 1).then(|| r - 1);
                if r == 1 {
                    belt.push_back(t + lat);
                }
            }
            if p.is_none() && !on {
                used += 1;
                p = Some(press);
            }
            while belt.front().is_some_and(|&a| a <= t) {
                belt.pop_front();
                sheets += 1;
            }
            if let Some(r) = c {
                c = (r > 1).then(|| r - 1);
                if r == 1 {
                    coal += 1;
                }
            }
            if c.is_none() && sheets > 0 {
                sheets -= 1;
                c = Some(consume);
            }
            least = least.min(sheets);
            let next = if sheets >= hi {
                true
            } else if sheets <= lo {
                false
            } else {
                on
            };
            flips += i64::from(next != on);
            on = next;
        }
        (used, coal, least, flips)
    }

    /// Latency (§4.4): the press's output reaches the store over a 300-tick belt. A rise waits
    /// the latency (the lower rate runs meanwhile), so after the switch restarts the press the
    /// store keeps draining for 300 ticks, to about 5 sheets, as in Create; a fall is at once, so
    /// the replay never runs ahead of the control.
    #[test]
    fn a_rise_waits_the_path_latency_and_the_replay_stays_under_the_control() {
        let run = |lat: u32| {
            let mut b = switched_line();
            b.edges[1].latency = lat;
            let mut s = setup(&b, 3);
            sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, 80)], &[]]);
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            pass(&mut e);
            assert_eq!(
                e.glass.districts[&D]
                    .compiled
                    .as_ref()
                    .map(|c| c.graph.longest_latency),
                Some(u64::from(lat))
            );
            feed(&mut e, &load_state(200, LoadState::Unloaded));
            let span = 100_000u64;
            let mut least = i64::MAX;
            for t in (200..200 + span).step_by(7) {
                e.advance_to(Tick(t));
                let st = e.sim_stock(D).expect("replaying");
                let sum = st.get(BRASS) + st.get(SHEET) + st.get(COAL);
                assert!((100_080 - 16..=100_080).contains(&sum), "at {t}: {st:?}");
                least = least.min(st.get(SHEET));
            }
            e.advance_to(Tick(200 + span));
            let r = e.glass.report(D).expect("a compile");
            assert_eq!(
                r.replay.model,
                replay_model::GLASS,
                "{:?}",
                e.glass.districts[&D].fallback
            );
            e.check_books().expect("books");
            let st = e.sim_stock(D).expect("replaying");
            (
                100_000 - st.get(BRASS),
                st.get(COAL),
                least,
                r.replay.breakpoints,
            )
        };
        let span = 100_000;
        let (used, coal, least, bps) = run(300);
        let (ref_used, ref_coal, ref_least, _) =
            switched_reference_lat((10, 20, 100, 20), 80, span, 300);
        // Never above the control (the item in the consumer's cycle aside, G2-21).
        assert!(coal <= ref_coal + 1, "made {coal}, reference {ref_coal}");
        assert!(
            coal as f64 >= ref_coal as f64 * 0.99,
            "made {coal}, reference {ref_coal}"
        );
        // The press feeds what is taken, plus the switch's band (Create's belt overshoots the
        // top, the replay's does not, so the two press in different phases).
        assert!(used <= coal + 20 + 2, "pressed {used}, made {coal}");
        assert!(
            (used - ref_used).abs() as f64 <= ref_used as f64 * 0.01 + 40.0,
            "pressed {used}, reference {ref_used}"
        );
        // The store drains through the latency as in Create: down to about 5 sheets.
        assert!(
            (ref_least - 2..=ref_least + 2).contains(&least),
            "{least} vs {ref_least}"
        );
        // Without the belt it turns at 20.
        let (_, _, least0, bps0) = run(0);
        assert!(least0 >= 18, "{least0}");
        assert!(bps < 200 && bps0 < 200, "{bps} {bps0} breakpoints");
    }

    /// A partial load (I7, G-B6): the brass store's chunk runs, the press and the sheet store's do
    /// not. The cut edge from the live store carries brass from the district's stock across, the
    /// simulated press makes sheets in its store, and the reload lands both sides.
    #[test]
    fn a_partial_load_carries_the_cut_edge_from_the_live_store() {
        let (mut b, p) = press_line();
        let far = BlockPos::new(32, 64, 0);
        b.nodes[p as usize].pos = far;
        b.nodes[1].pos = far;
        let mut s = setup(&b, 2);
        for t in &mut s {
            if let Obs::Inventory(decl) = &mut t.obs
                && decl.index == 1
            {
                decl.pos = far;
            }
        }
        let near = BlockPos::default().chunk(0);
        let away = far.chunk(0);
        for c in [near, away] {
            s.push(at(
                0,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: c,
                    applied_epoch: 0,
                },
            ));
        }
        sweep(&mut s, 1, 100, &[&[(BRASS, 1000)], &[]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(
            &mut e,
            &at(
                200,
                Obs::ChunkUnloaded {
                    district: D,
                    chunk: away,
                },
            ),
        );
        feed(
            &mut e,
            &at(
                200,
                Obs::LoadState {
                    district: D,
                    state: LoadState::Partial,
                    ticking_chunks: 1,
                    total_chunks: 2,
                },
            ),
        );
        e.advance_to(Tick(201));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert_eq!(r.replay.cut_edges, 1);
        // A sheet per 10 ticks across the cut, never more; the district's stock pays the brass.
        e.advance_to(Tick(3200));
        let st = e.sim_stock(D).expect("replaying");
        // A cycle's hold at the start (G2-6), and a unit either way to the rates' rounding (a rise
        // rounds down, a fall away from zero).
        assert!((298..=300).contains(&st.get(SHEET)), "{st:?}");
        assert!(
            (998..=1000).contains(&(st.get(BRASS) + st.get(SHEET))),
            "{st:?}"
        );
        // The live store held 1,000: the press stops when the cut has carried them all.
        e.advance_to(Tick(20_000));
        let st = e.sim_stock(D).expect("replaying");
        assert_eq!(st.get(BRASS), 0, "{st:?}");
        assert!((999..=1000).contains(&st.get(SHEET)), "{st:?}");
        e.check_books().expect("books");
        // Back: the sheets land in their store, the brass leaves the live one (the last brass is
        // in the press, as in the whole-district replay).
        let mut cmds = feed(
            &mut e,
            &at(
                20_000,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: away,
                    applied_epoch: 0,
                },
            ),
        );
        cmds.extend(feed(&mut e, &load_state(20_000, LoadState::Live)));
        e.advance_to(Tick(20_001));
        cmds.extend(e.drain_commands());
        let d = deltas(&cmds);
        let made = d.get(&(1, SHEET)).copied().unwrap_or(0);
        assert!((999..=1000).contains(&made), "{d:?}");
        assert_eq!(d.get(&(0, BRASS)), Some(&-1000), "{d:?}");
        e.check_books().expect("books");
    }

    /// A partial load with the cut the other way (I7, G-B6): the brass store and the press are
    /// simulated, the sheet store runs and has room for 312 sheets (8 slots, 200 sheets). A fully
    /// loaded factory stops pressing once the store is full, so the replay may carry at most that
    /// room across the cut (G-B2's ceiling), and the reload owes nothing more. Before choice G2-66
    /// the live store's room was a flag from the compile's regime, not an amount: one with any
    /// room took every sheet, so the press pressed all 1,000 brass into the cut's `Stock` and the
    /// reload owed what did not fit (the straddle smoke's water in the m7 exit runs too).
    #[test]
    fn a_cut_into_a_live_store_carries_no_more_than_its_room() {
        let (mut b, p) = press_line();
        let far = BlockPos::new(32, 64, 0);
        b.nodes[p as usize].pos = far;
        b.nodes[0].pos = far;
        let mut s = setup(&b, 2);
        for t in &mut s {
            if let Obs::Inventory(decl) = &mut t.obs {
                if decl.index == 0 {
                    decl.pos = far;
                } else {
                    decl.item_slots = 8;
                }
            }
        }
        let near = BlockPos::default().chunk(0);
        let away = far.chunk(0);
        for c in [near, away] {
            s.push(at(
                0,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: c,
                    applied_epoch: 0,
                },
            ));
        }
        sweep(&mut s, 1, 100, &[&[(BRASS, 1000)], &[(SHEET, 200)]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(
            &mut e,
            &at(
                200,
                Obs::ChunkUnloaded {
                    district: D,
                    chunk: away,
                },
            ),
        );
        feed(
            &mut e,
            &at(
                200,
                Obs::LoadState {
                    district: D,
                    state: LoadState::Partial,
                    ticking_chunks: 1,
                    total_chunks: 2,
                },
            ),
        );
        e.advance_to(Tick(201));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert_eq!(r.replay.cut_edges, 1);
        e.advance_to(Tick(20_000));
        let st = e.sim_stock(D).expect("replaying");
        let pressed = 1000 - st.get(BRASS);
        // The room fills (the seal is not the store reading full at once).
        assert!(
            (300..=312 + 1).contains(&pressed),
            "pressed {pressed} into 312 of room: {st:?}"
        );
        let mut cmds = feed(
            &mut e,
            &at(
                20_000,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: away,
                    applied_epoch: 0,
                },
            ),
        );
        cmds.extend(feed(&mut e, &load_state(20_000, LoadState::Live)));
        e.advance_to(Tick(20_001));
        cmds.extend(e.drain_commands());
        let d = deltas(&cmds);
        let landed = d.get(&(1, SHEET)).copied().unwrap_or(0);
        assert!((300..=312).contains(&landed), "pressed {pressed}: {d:?}");
        let owed = e.district(D).expect("known").owed.get(SHEET);
        assert_eq!(owed, 0, "the district owes {owed} sheets");
        e.check_books().expect("books");
    }

    /// The cut into a live mover (choice G2-66): the press is simulated, the chute it pushes into
    /// and the sheet store run. A chute holds nothing a reload could place, so its room is 0: the
    /// cut good seals at once, the press stays off while away, the brass stays, and the reload
    /// lands and owes nothing.
    #[test]
    fn a_cut_into_a_live_mover_carries_nothing() {
        let mut b = B::default();
        let input = b.store(0);
        let out = b.store(1);
        let press = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
        let p = b.machine(model::PRESS, press, 1, 10);
        let chute = b.node(
            NodeKind::Mover,
            model::CHUTE,
            0,
            vec![RateEntry {
                flags: rate_flags::SHARED,
                ..entry(None, 16, 7)
            }],
        );
        b.edge(input, p);
        b.edge_k(EdgeKind::Chute, p, chute, &[]);
        b.edge_k(EdgeKind::Chute, chute, out, &[]);
        b.window = 2000;
        let far = BlockPos::new(32, 64, 0);
        b.nodes[p as usize].pos = far;
        b.nodes[input as usize].pos = far;
        let mut s = setup(&b, 2);
        for t in &mut s {
            if let Obs::Inventory(decl) = &mut t.obs {
                if decl.index == 0 {
                    decl.pos = far;
                } else {
                    decl.item_slots = 8;
                }
            }
        }
        let near = BlockPos::default().chunk(0);
        let away = far.chunk(0);
        for c in [near, away] {
            s.push(at(
                0,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: c,
                    applied_epoch: 0,
                },
            ));
        }
        sweep(&mut s, 1, 100, &[&[(BRASS, 1000)], &[(SHEET, 200)]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(
            &mut e,
            &at(
                200,
                Obs::ChunkUnloaded {
                    district: D,
                    chunk: away,
                },
            ),
        );
        feed(
            &mut e,
            &at(
                200,
                Obs::LoadState {
                    district: D,
                    state: LoadState::Partial,
                    ticking_chunks: 1,
                    total_chunks: 2,
                },
            ),
        );
        e.advance_to(Tick(201));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert_eq!(r.replay.cut_edges, 1);
        e.advance_to(Tick(20_000));
        let st = e.sim_stock(D).expect("replaying");
        assert_eq!(st.get(BRASS), 1000, "pressed into a chute: {st:?}");
        let mut cmds = feed(
            &mut e,
            &at(
                20_000,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: away,
                    applied_epoch: 0,
                },
            ),
        );
        cmds.extend(feed(&mut e, &load_state(20_000, LoadState::Live)));
        e.advance_to(Tick(20_001));
        cmds.extend(e.drain_commands());
        let d = deltas(&cmds);
        assert_eq!(d.get(&(1, SHEET)).copied().unwrap_or(0), 0, "{d:?}");
        assert_eq!(e.district(D).expect("known").owed.get(SHEET), 0);
        e.check_books().expect("books");
    }

    /// The residual node (G-B13, I9): a black box between the brass and the sheets is measured
    /// at a sheet per 10 ticks for a brass. Its window passes, and the replay runs what it
    /// measured (shaded by the checksum's z, never above) while the factory is as it saw it:
    /// once the brass runs out the sheets stop too.
    #[test]
    fn the_residual_replays_a_black_box_while_its_stores_hold() {
        let mut b = B::default();
        let input = b.store(0);
        let out = b.store(1);
        let bb = b.node(NodeKind::BlackBox, model::BLACK_BOX, 0, Vec::new());
        b.edge(input, bb);
        b.edge_k(EdgeKind::Adjacency, bb, out, &[SHEET]);
        b.window = 2000;
        let mut s = setup(&b, 2);
        for k in 1..=35u32 {
            let t = u64::from(k) * 100;
            let m = (t / 10) as i64;
            sweep(&mut s, k, t, &[&[(BRASS, 2000 - m)], &[(SHEET, m)]]);
        }
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        let r = e.glass.report(D).expect("a compile");
        assert!(r.replay.eligible, "a window passed: {:?}", r.verdict);
        feed(&mut e, &load_state(3500, LoadState::Unloaded));
        e.advance_to(Tick(3501));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert_eq!(r.replay.learned_sources, 1);
        e.advance_to(Tick(6500));
        let st = e.sim_stock(D).expect("replaying");
        let made = st.get(SHEET) - 350;
        let used = 1650 - st.get(BRASS);
        assert!((270..=300).contains(&made), "{st:?}");
        assert!((270..=301).contains(&used), "{st:?}");
        // The brass runs out near 20,000: the residual stops with it.
        e.advance_to(Tick(40_000));
        let st = e.sim_stock(D).expect("replaying");
        assert_eq!(st.get(BRASS), 0, "{st:?}");
        let end = st.get(SHEET);
        assert!(end <= 2000, "{st:?}");
        e.advance_to(Tick(60_000));
        assert_eq!(e.sim_stock(D).expect("replaying").get(SHEET), end);
        e.check_books().expect("books");
    }

    /// A bounded good measured inside its window [L, U] is calibrated, not a residual (GLASSBOX.md
    /// §5.3, choice G2-57): a residual there books the output from nothing (the exit run's
    /// package_loop made clay balls from no clay, the clay's own window being too narrow to take
    /// any). Only a production above U is a learned source, by its excess over U.
    #[test]
    fn a_bounded_good_inside_its_window_takes_no_residual() {
        let bounded = |good, measured: f64, predicted: f64, (lo, hi): (f64, f64)| {
            crate::glass::check::GoodResult {
                good,
                class: class::BOUNDED,
                why: (0, 0),
                in_l: 0.0,
                in_u: 0.0,
                out_l: 0.0,
                out_u: 0.0,
                predicted,
                lo,
                hi,
                measured,
                sigma: 0.0,
                q: 1.0,
                verdict: verdict::PASS,
                samples: 20,
            }
        };
        // Sheets measured at 0.15 a tick (inside [0.1, 0.2]) and at 0.3 (0.1 above U).
        for (measured, learned) in [(0.15, 0u32), (0.3, 1)] {
            let (b, _) = press_line();
            let mut s = setup(&b, 2);
            sweep(&mut s, 1, 100, &[&[(BRASS, 1000)], &[]]);
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            pass(&mut e);
            let c = e
                .glass
                .districts
                .get_mut(&D)
                .and_then(|g| g.compiled.as_mut())
                .expect("a compile");
            let w = c.passed.as_mut().expect("passed");
            w.goods = vec![
                bounded(BRASS, -0.1, -0.1, (-0.1, 0.0)),
                bounded(SHEET, measured, 0.1, (0.1, 0.2)),
            ];
            w.stores = vec![(0, BRASS, -0.1, -0.1), (1, SHEET, measured, 0.1)];
            feed(&mut e, &load_state(200, LoadState::Unloaded));
            e.advance_to(Tick(201));
            let r = e.glass.report(D).expect("a compile");
            assert_eq!(r.replay.model, replay_model::GLASS, "measured {measured}");
            assert_eq!(r.replay.learned_sources, learned, "measured {measured}");
            e.advance_to(Tick(3200));
            let st = e.sim_stock(D).expect("replaying");
            let made = st.get(SHEET);
            let used = 1000 - st.get(BRASS);
            // The press makes a sheet a brass: the calibrated good makes nothing beside it.
            assert!((295..=300).contains(&used), "measured {measured}: {st:?}");
            if learned == 0 {
                assert!(made <= used, "measured {measured}: {st:?}");
            } else {
                // The learned source adds its excess over U, 0.1 a tick, and no more.
                assert!(
                    (made - used - 300).abs() <= 2,
                    "measured {measured}: {st:?}"
                );
            }
            e.check_books().expect("books");
        }
    }

    /// Corpus 6, precision_mechanisms (the exit run's loop factory): the host's only sheet supply
    /// is round the belt loop. Replayed while away it makes what the solve gives, 4/5 of a
    /// mechanism per sheet at a sheet per 64 ticks, never above it.
    #[test]
    fn a_sequenced_loop_replays_its_mechanisms_while_away() {
        let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
        let (b, _) = precision_builder(&identity, 9, 64);
        let mut s = setup(&b, 5);
        sweep(
            &mut s,
            1,
            100,
            &[
                &[(ZINC, 1000)],
                &[(SHEET, 1000)],
                &[(COAL, 1000)],
                &[(COPPER, 1000)],
                &[],
            ],
        );
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        e.advance_to(Tick(201));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        // 6,400 ticks: 100 sheets, 80 mechanisms, less the start's hold of 113 ticks (the
        // funnel, the feeder, and the whole loop as a bound). Each hold coming due is a
        // breakpoint, and none is an oscillation: holds key on what the regime could run, so the
        // host's other inputs are not held again when the sheets' hold comes due.
        e.advance_to(Tick(6600));
        let r = e.glass.report(D).expect("a compile");
        let st = e
            .sim_stock(D)
            .unwrap_or_else(|| panic!("{r:?} {:?}", e.glass.districts[&D].fallback));
        let made = st.get(BRASS);
        assert!((77..=79).contains(&made), "{st:?}");
        // A sheet in 4/75 comes back; 5 parts a product, a zinc in 30 back.
        let rounds = 1000 - st.get(SHEET);
        assert!((92..=94).contains(&rounds), "{st:?}");
        assert!((487..=490).contains(&(1000 - st.get(ZINC))), "{st:?}");
        assert_eq!(r.replay.oscillations, 0);
        assert!(r.replay.breakpoints <= 8, "{:?}", r.replay);
        e.check_books().expect("books");
    }

    /// [`precision_builder`]'s line with chests of 27 slots, the output one already holding
    /// mechanisms and a stack's worth of `kinds` other goods: what the exit run's
    /// precision_mechanisms held when it unloaded.
    fn precision_in_chests(kinds: i64) -> (Engine, [u32; 5]) {
        let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
        let (mut b, stores) = precision_builder(&identity, 9, 64);
        for v in stores {
            b.nodes[v as usize].capacity = 27;
        }
        let mut s = setup(&b, 5);
        let mut out = vec![(BRASS, 39)];
        out.extend(
            [(WOOL, 64), (ORE, 64), (CRUSHED, 64), (XP, 64)]
                .iter()
                .take(kinds as usize),
        );
        sweep(
            &mut s,
            1,
            100,
            &[
                &[(ZINC, 688)],
                &[(SHEET, 202)],
                &[(COAL, 688)],
                &[(COPPER, 688)],
                &out,
            ],
        );
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        (e, stores)
    }

    /// The exit run's precision_mechanisms took 688 of each part and made nothing: the output
    /// chest's free slots were split by rate, a byproduct's share floored to no slot, and the
    /// chest read full at the unload. Each good arriving now has a slot before the rest is split
    /// by rate (choice G2-26), so the line makes what it does in an empty chest.
    #[test]
    fn a_sequenced_loop_replays_into_a_chest_with_free_slots() {
        let (mut e, _) = precision_in_chests(3);
        e.advance_to(Tick(6600));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        let st = e.sim_stock(D).expect("replaying");
        // 6,400 ticks less the start's hold: about 78 mechanisms, 5 parts a sequence.
        let made = st.get(BRASS) - 39;
        assert!((77..=79).contains(&made), "{st:?}");
        assert!((487..=490).contains(&(688 - st.get(ZINC))), "{st:?}");
        assert!((490..=493).contains(&(688 - st.get(COAL))), "{st:?}");
        e.check_books().expect("books");
    }

    /// A loop whose only exit is a full chest jams: what it makes circles until the belts are
    /// full. The solve has no fixed point there (the loop's population grows round by round), and
    /// a replay booked on an unsettled solve took parts it never delivered. It falls back instead
    /// (choice G2-27), and takes nothing.
    #[test]
    fn a_sequenced_loop_into_a_full_chest_takes_nothing() {
        // 27 slots: the 39 mechanisms' one, 26 kinds of a stack each... the builder's goods give
        // four, so a chest of 5 slots stands in.
        let identity: [u32; 19] = std::array::from_fn(|i| i as u32);
        let (mut b, stores) = precision_builder(&identity, 9, 64);
        b.nodes[stores[4] as usize].capacity = 5;
        let mut s = setup(&b, 5);
        sweep(
            &mut s,
            1,
            100,
            &[
                &[(ZINC, 688)],
                &[(SHEET, 202)],
                &[(COAL, 688)],
                &[(COPPER, 688)],
                &[(BRASS, 64), (WOOL, 64), (ORE, 64), (CRUSHED, 64), (XP, 64)],
            ],
        );
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        e.advance_to(Tick(6600));
        let r = e.glass.report(D).expect("a compile");
        assert_ne!(r.replay.model, replay_model::GLASS, "{:?}", r.replay);
        if let Some(st) = e.sim_stock(D) {
            for (g, was) in [
                (ZINC, 688),
                (SHEET, 202),
                (COAL, 688),
                (COPPER, 688),
                (BRASS, 64),
            ] {
                assert_eq!(st.get(g), was, "{st:?}");
            }
        }
        e.check_books().expect("books");
    }

    /// Corpus 3, arm_burners_depots as the exit run compiled it: two fan-blasted depots, each fed
    /// ore by a chute, each heated by a blaze burner; one arm feeds the burners coal from a smart
    /// chute and carries what the depots make to a chest. The arm closes a loop through the heat
    /// edges. Inventories: 0 the output chest, 1 and 3 the ore, 2 the coal.
    fn arm_depots() -> B {
        let mut b = B::default();
        let out = b.store(0);
        b.nodes[out as usize].capacity = 54;
        let ore1 = b.store(1);
        let coal = b.store(2);
        let ore3 = b.store(3);
        for v in [ore1, coal, ore3] {
            b.nodes[v as usize].capacity = 27;
        }
        let blast = b.recipe(&[(ORE, 1)], &[(CRUSHED, 1)]);
        b.recipes[blast as usize].recipe_type = recipe_type::BLASTING;
        b.recipes[blast as usize].heat = heat::NEEDS_HEATED;
        let fuel = b.recipe(&[(COAL, 1)], &[]);
        b.recipes[fuel as usize].recipe_type = recipe_type::FUEL;
        b.recipes[fuel as usize].heat = heat::KINDLED;
        let shared = |batch, num, den| RateEntry {
            level: 0,
            recipe: None,
            batch,
            ticks_num: num,
            ticks_den: den,
            flags: rate_flags::SHARED,
        };
        let mut heaters = Vec::new();
        for left in [1582, 782] {
            let h = b.node(
                NodeKind::Heater,
                model::BLAZE_BURNER,
                0,
                vec![entry(Some(fuel), 1, 1600)],
            );
            b.nodes[h as usize].params = [left, i64::from(heat::KINDLED), 1600, 500];
            heaters.push(h);
        }
        let mut depots = Vec::new();
        for _ in 0..2 {
            let m = b.node(
                NodeKind::Machine,
                model::FAN_ZONE,
                node_flags::PASS_THROUGH,
                vec![RateEntry {
                    ticks_num: 319,
                    ticks_den: 2,
                    ..entry(Some(blast), 16, 0)
                }],
            );
            depots.push(m);
        }
        let smart = b.node(
            NodeKind::Mover,
            model::SMART_CHUTE,
            0,
            vec![shared(1, 7, 1)],
        );
        let arm = b.node(
            NodeKind::Mover,
            model::ARM,
            node_flags::BOUNDED,
            vec![shared(64, 110, 7)],
        );
        b.nodes[arm as usize].rule = rule::PREFER_FIRST;
        let ordered = |b: &mut B, kind, from, to, order, filter: &[Good]| {
            let e = b.edge_k(kind, from, to, filter);
            b.edges[e as usize].order = order;
            e
        };
        for (ore, depot) in [(ore1, depots[0]), (ore3, depots[1])] {
            let chute = b.node(NodeKind::Mover, model::CHUTE, 0, vec![shared(16, 7, 1)]);
            ordered(&mut b, EdgeKind::Chute, ore, chute, 0, &[]);
            ordered(&mut b, EdgeKind::Chute, chute, depot, 0, &[]);
        }
        ordered(&mut b, EdgeKind::Chute, coal, smart, 0, &[]);
        ordered(&mut b, EdgeKind::ArmReach, smart, arm, 0, &[]);
        ordered(&mut b, EdgeKind::ArmReach, depots[0], arm, 1, &[]);
        ordered(&mut b, EdgeKind::ArmReach, depots[1], arm, 2, &[]);
        ordered(&mut b, EdgeKind::ArmReach, arm, heaters[0], 0, &[]);
        ordered(&mut b, EdgeKind::ArmReach, arm, heaters[1], 1, &[]);
        ordered(&mut b, EdgeKind::ArmReach, arm, out, 2, &[CRUSHED]);
        for (h, m) in heaters.iter().zip(&depots) {
            let e = ordered(&mut b, EdgeKind::Heat, *h, *m, 0, &[]);
            b.edges[e as usize].param = 3;
        }
        b
    }

    /// The exit run's arm_burners_depots made a fifth of the control while away. Two causes: the
    /// path latency counted each burner's 1,600 ticks per coal as a transit (a start hold of
    /// 3,500 ticks, in which the unfed burners went out), and the solve's first round read a
    /// burner later in the order as unlit, capping its depot's input at nothing for good
    /// (choices G2-23, G2-24). Replayed here, both depots blast at their rate once the holds
    /// are over: the start's and each burner's first take of coal, when the arm's flow rises and
    /// the whole arm waits the loop's bound.
    #[test]
    fn an_arm_fed_pair_of_heated_depots_replays_at_its_rate() {
        let b = arm_depots();
        let mut s = setup(&b, 4);
        sweep(
            &mut s,
            1,
            100,
            &[&[], &[(ORE, 1200)], &[(COAL, 58)], &[(ORE, 1200)]],
        );
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        let c = e.glass.districts[&D].compiled.as_ref().expect("compiled");
        assert_eq!(bound(&c.sol, CRUSHED).in_l, Q::new(2 * 16 * 2, 319));
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        e.advance_to(Tick(2200));
        let early = e.sim_stock(D).expect("replaying").get(CRUSHED);
        let span = 6000u64;
        e.advance_to(Tick(200 + span));
        let r = e.glass.report(D).expect("a compile");
        assert_eq!(
            r.replay.model,
            replay_model::GLASS,
            "{:?}",
            e.glass.districts[&D].fallback
        );
        assert_eq!(r.replay.oscillations, 0);
        let st = e.sim_stock(D).expect("replaying");
        let made = st.get(CRUSHED);
        let rate = 2.0 * 16.0 / 159.5;
        assert!(made as f64 <= rate * span as f64 + 2.0, "{st:?}");
        assert!(made as f64 >= 0.85 * rate * span as f64, "{st:?}");
        // Past the holds, at the rate.
        let late = (made - early) as f64;
        assert!((late - rate * 4000.0).abs() <= 3.0, "{late}");
        // What is taken and not yet made sits on the depots and in the arm's hand.
        let wip = 1200 * 2 - st.get(ORE) - made;
        assert!((0..=2 * 16 + 2).contains(&wip), "{wip}");
        e.check_books().expect("books");
    }

    /// One switched line (its press's output on a belt `lat` ticks long) replayed from its unload
    /// at 200 to `200 + span`, reading the stock at `reads` (sorted, inside the span): the stocks
    /// read, then the final stock and report.
    fn switched_run(
        line: (u32, u32, i64, i64),
        lat: u32,
        s0: i64,
        span: u64,
        reads: &[u64],
    ) -> (Vec<(u64, GoodVec)>, GoodVec, u32, u32, bool) {
        let (press, consume, hi, lo) = line;
        let mut b = switched_line_with(press, consume, hi, lo);
        b.edges[1].latency = lat;
        let mut s = setup(&b, 3);
        sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, s0)], &[]]);
        let mut e = engine();
        for t in &s {
            feed(&mut e, t);
        }
        pass(&mut e);
        feed(&mut e, &load_state(200, LoadState::Unloaded));
        let mut seen = Vec::new();
        for &t in reads {
            e.advance_to(Tick(200 + t));
            seen.push((t, e.sim_stock(D).unwrap_or_default()));
        }
        e.advance_to(Tick(200 + span));
        let st = e.sim_stock(D).unwrap_or_default();
        let r = e.glass.report(D).expect("a compile");
        let books = e.check_books().is_ok();
        (seen, st, r.replay.breakpoints, r.replay.model, books)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// Switched lines of every shape (G2 property tests, GLASSBOX.md §11.3), half of them with
        /// the press's output on a belt: goods are conserved (nothing made from nothing, a few
        /// units at most lost to machines and time shifts), reads on the way change nothing (the
        /// composition rule with periodic edges), and the controlled line never runs above the
        /// tick-by-tick control.
        #[test]
        fn switched_lines_conserve_compose_and_stay_under_the_control(
            press in 4u32..=30,
            slow in 11u32..=40,
            hi in 30i64..=200,
            band in 5i64..=60,
            s0f in 0.0f64..=1.2,
            span in 20_000u64..=200_000,
            lat in prop_oneof![Just(0u32), 1u32..=400],
            mut reads in prop::collection::vec(1u64..=200_000, 0..6),
        ) {
            // The consumer slower than the press (a tenth to four times slower), so the switch
            // cycles; the band inside the levels.
            let consume = press * slow / 10;
            let lo = (hi - band).max(1);
            let s0 = (hi as f64 * s0f) as i64;
            reads.retain(|&t| t < span);
            reads.sort_unstable();
            let line = (press, consume, hi, lo);
            let (seen, st, bps, model, books) = switched_run(line, lat, s0, span, &reads);
            prop_assert_eq!(model, replay_model::GLASS, "fell back after {} breakpoints", bps);
            prop_assert!(books, "books");
            let (ref_used, ref_coal, _, flips) = switched_reference_lat(
                (u64::from(press), u64::from(consume), hi, lo),
                s0,
                span,
                u64::from(lat),
            );
            // Conservation: never more than there was; at most a unit per switch flip (a time
            // shift's rounding, G2-12) and a few in machines less. The latency rule holds nodes
            // off, so it loses nothing (G2-6).
            let total = 100_000 + s0;
            let b = hi - lo;
            let periods = 2 * span as i64 / ((b - 2).max(1) * i64::from(consume)) + 2;
            let slack = 4 + if lat == 0 { flips } else { periods };
            for (t, g) in seen.iter().chain(std::iter::once(&(span, st.clone()))) {
                let sum = g.get(BRASS) + g.get(SHEET) + g.get(COAL);
                prop_assert!(sum <= total, "at {}: {} above {}", t, sum, total);
                prop_assert!(sum >= total - slack, "at {}: {} lost more than {}", t, total - sum, slack);
            }
            // The composition rule: the same run without reads ends the same.
            let (_, st2, bps2, _, _) = switched_run(line, lat, s0, span, &[]);
            prop_assert_eq!(&st, &st2);
            prop_assert_eq!(bps, bps2);
            // Never above the control: the consumer made at most what Create made (and the item
            // in its cycle, G2-21), and the press pressed at most what was taken plus the room
            // left to the switch's top.
            let used = 100_000 - st.get(BRASS);
            let coal = st.get(COAL);
            prop_assert!(coal <= ref_coal + 1, "made {} above {}", coal, ref_coal);
            prop_assert!(
                used <= coal + (hi - s0).max(0) + 2,
                "pressed {} for {} taken (switch at {}, from {})", used, coal, hi, s0
            );
            // And not far below it. A switch's breakpoint costs its regime under a tick (choice
            // G2-12), and the replay's switch flips a tick before Create's at each threshold
            // (G2-20), so it flips more often, by its band over the band less two. With a belt
            // the replay's cycle is shorter than Create's (it stops at the top, Create's belt
            // still delivers), so it is held only where the consumer never runs dry.
            if lat == 0 {
                let lost_ticks = flips * b / (b - 2).max(1) + 4;
                prop_assert!(
                    coal >= ref_coal - lost_ticks / i64::from(consume) - 2,
                    "made {}, reference {} ({} flips)", coal, ref_coal, flips
                );
                prop_assert!(
                    (used - ref_used).abs()
                        <= ref_used / 100 + (hi - lo) + 2 + lost_ticks / i64::from(press),
                    "pressed {}, reference {}", used, ref_used
                );
            } else if lo > i64::from(lat) / i64::from(consume) + 3 && s0 > i64::from(lat) / i64::from(consume) + 3 {
                prop_assert!(
                    coal >= ref_coal - periods / i64::from(consume) - 2,
                    "made {}, reference {} (belt {})", coal, ref_coal, lat
                );
            }
        }
    }

    /// The glass box's pages (G2-9, `glass::page`): a district saved and restored in the middle
    /// of its glass replay replays exactly as one that never stopped, page for page.
    mod paging {
        use super::*;
        use crate::glass::page::{
            GLASS_MAGIC, GLASS_MAGIC_IGL1, PageRead, district_page, read_district_page,
        };
        use crate::persist::GLASS_TABLE;
        use rtsim_kernel::page::PageKey;

        type Pages = Vec<(PageKey, Vec<u8>)>;

        /// The record at its time, with no save: these tests make their own (`save`).
        fn feed(e: &mut Engine, t: &Timed) -> Vec<Cmd> {
            if t.at > e.now() {
                e.advance_to(t.at);
            }
            e.ingest(t);
            e.drain_commands()
        }

        /// What a run does after its start: advance to a tick, or feed a record.
        enum Act {
            To(u64),
            Feed(Timed),
        }
        use Act::{Feed, To};

        /// The engine through a save and a restore; the restored engine saves the same pages.
        fn round_trip(e: &Engine) -> Engine {
            let pages = e.snapshot();
            assert!(
                pages.iter().any(|(k, _)| k.table() == GLASS_TABLE),
                "a glass page"
            );
            let r = Engine::restore(pages.clone(), Tunables::default()).expect("restore");
            assert!(
                r.snapshot() == pages,
                "a restored engine saves what it was restored from"
            );
            r
        }

        /// A store's industry pages: what the saves put and deleted, by key.
        type Disk = std::collections::BTreeMap<PageKey, Vec<u8>>;

        /// One save into `disk`, as the host makes it (docs/choices-industry-dirty-pages.md ID2):
        /// only the pages changed since the save before, and the deletions. The disk then holds
        /// the engine's whole state, page for page. Returns the pages written or deleted, the
        /// globals page not counted.
        fn save(e: &mut Engine, disk: &mut Disk) -> usize {
            let c = e.save_changed();
            for k in &c.delete {
                disk.remove(k);
            }
            for (k, b) in c.put {
                disk.insert(k, b);
            }
            let whole: Disk = e.snapshot().into_iter().collect();
            assert!(
                *disk == whole,
                "the saves' pages are the engine's whole state"
            );
            if let Some(c) = e.page_check() {
                assert_eq!(c.unmarked, 0, "{:?}", c.first_unmarked);
            }
            c.ticket.len()
        }

        /// The engine the disk holds, on tunables `tun`; the restored engine saves the same pages.
        fn restored(disk: &Disk, tun: &Tunables) -> Engine {
            let r = Engine::restore(disk.clone(), tun.clone()).expect("restore");
            let pages: Disk = r.snapshot().into_iter().collect();
            assert!(
                pages == *disk,
                "a restored engine saves what it was restored from"
            );
            r
        }

        /// Runs `acts` on the engine `start` makes, saving after its start and after each act
        /// and restoring it from the saves' pages before each act whose index is in `cuts`: its
        /// pages at the end, every command on the way, its replay model at each act, and the
        /// pages each save wrote.
        fn run(
            start: &dyn Fn() -> Engine,
            acts: &[Act],
            cuts: &[usize],
        ) -> (Pages, Vec<Cmd>, Vec<u32>, Vec<usize>) {
            run_with(&Tunables::default(), start, acts, cuts)
        }

        /// [`run`] on tunables `tun` (the engine `start` makes runs on them too).
        fn run_with(
            tun: &Tunables,
            start: &dyn Fn() -> Engine,
            acts: &[Act],
            cuts: &[usize],
        ) -> (Pages, Vec<Cmd>, Vec<u32>, Vec<usize>) {
            let mut e = start();
            let mut disk = Disk::new();
            let mut writes = vec![save(&mut e, &mut disk)];
            let mut cmds = e.drain_commands();
            let mut models = Vec::new();
            for (i, a) in acts.iter().enumerate() {
                if cuts.contains(&i) {
                    e = restored(&disk, tun);
                }
                match a {
                    To(t) => e.advance_to(Tick(*t)),
                    Feed(t) => cmds.extend(feed(&mut e, t)),
                }
                cmds.extend(e.drain_commands());
                models.push(model(&e));
                writes.push(save(&mut e, &mut disk));
            }
            e.check_books().expect("books");
            (e.snapshot(), cmds, models, writes)
        }

        /// The run whole, and cut before each act in turn and before all of them at once.
        fn exact(start: &dyn Fn() -> Engine, acts: &[Act]) -> Vec<u32> {
            exact_with(&Tunables::default(), start, acts)
        }

        /// [`exact`] on tunables `tun`.
        fn exact_with(tun: &Tunables, start: &dyn Fn() -> Engine, acts: &[Act]) -> Vec<u32> {
            let whole = run_with(tun, start, acts, &[]);
            assert!(
                whole.2.contains(&replay_model::GLASS),
                "a glass replay: {:?}",
                whole.2
            );
            let every: Vec<usize> = (0..acts.len()).collect();
            for cuts in every.iter().map(|&i| vec![i]).chain([every.clone()]) {
                let cut = run_with(tun, start, acts, &cuts);
                assert_eq!(cut.2, whole.2, "models, cut at {cuts:?}");
                assert!(cut.1 == whole.1, "commands, cut at {cuts:?}");
                assert_eq!(cut.0.len(), whole.0.len(), "pages, cut at {cuts:?}");
                for (a, b) in cut.0.iter().zip(&whole.0) {
                    assert!(a == b, "page {:#x}, cut at {cuts:?}", a.0.0);
                }
            }
            whole.2
        }

        fn press() -> Engine {
            let (b, _) = press_line();
            let mut s = setup_n(&b, 2, 5);
            for k in 1..=35u32 {
                let t = u64::from(k) * 100;
                let m = (t / 10) as i64;
                sweep(&mut s, k, t, &[&[(BRASS, 1000 - m)], &[(SHEET, m)]]);
            }
            s.push(load_state(3500, LoadState::Unloaded));
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            e
        }

        /// A replay by unit, cut before each step (§9.3): a unit frozen by a fallback and the
        /// unit replaying on are paged, and the restored replay books what the whole run does.
        #[test]
        fn a_restored_replay_by_unit_freezes_as_one_that_never_stopped() {
            use super::units::{engine_with, judge, lines, two_lines};
            let mut tun = Tunables::default();
            tun.glass.breakpoints_per_event_max = 0;
            let start = {
                let tun = tun.clone();
                move || {
                    let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
                    let mut e = engine_with(&b, tun.clone(), &lines(500, (ORE, 2000)));
                    judge(&mut e, &[(p0, verdict::PASS), (p1, verdict::PASS)]);
                    feed(&mut e, &load_state(200, LoadState::Unloaded));
                    e
                }
            };
            let mut acts = vec![
                To(201),
                To(5000),
                To(5300),
                To(8000),
                To(15_000),
                To(25_000),
                Feed(load_state(25_000, LoadState::Live)),
            ];
            for x in [0, 40] {
                acts.push(Feed(at(
                    25_000,
                    Obs::ChunkLoaded {
                        district: D,
                        chunk: BlockPos::new(x, 64, 0).chunk(0),
                        applied_epoch: 0,
                    },
                )));
            }
            let models = exact_with(&tun, &start, &acts);
            assert_eq!(models[3], replay_model::GLASS, "{models:?}");
            assert_ne!(models[5], replay_model::GLASS, "{models:?}");
        }

        fn back(t: u64) -> [Act; 2] {
            [
                Feed(load_state(t, LoadState::Live)),
                Feed(at(
                    t,
                    Obs::ChunkLoaded {
                        district: D,
                        chunk: BlockPos::default().chunk(0),
                        applied_epoch: 0,
                    },
                )),
            ]
        }

        #[test]
        fn a_restored_press_replays_as_one_that_never_stopped() {
            let mut acts = vec![
                To(3501),
                To(4000),
                To(6500),
                To(9000),
                To(12_000),
                To(20_000),
            ];
            acts.extend(back(20_000));
            let models = exact(&press, &acts);
            assert_eq!(models[5], replay_model::GLASS);
        }

        /// Cut while the replay looks for its cycle, inside a cycle run in closed form, and
        /// between: the marks and the running cycle are paged.
        #[test]
        fn a_restored_switch_cycle_replays_as_one_that_never_stopped() {
            let start = || {
                let b = switched_line();
                let mut s = setup(&b, 3);
                sweep(&mut s, 1, 100, &[&[(BRASS, 100_000)], &[(SHEET, 80)], &[]]);
                let mut e = engine();
                for t in &s {
                    feed(&mut e, t);
                }
                pass(&mut e);
                feed(&mut e, &load_state(200, LoadState::Unloaded));
                e
            };
            let acts = [
                To(1_000),
                To(3_333),
                To(7_001),
                To(40_000),
                To(100_003),
                To(200_000),
                To(320_200),
            ];
            exact(&start, &acts);
            let e = run(&start, &acts, &[4]);
            let cycles = Engine::restore(e.0, Tunables::default())
                .expect("restore")
                .glass
                .districts[&D]
                .replay
                .as_ref()
                .map(|r| r.counts().cycles);
            assert!(cycles.is_some_and(|c| c >= 1), "{cycles:?}");
        }

        /// A heater's burn left is the replay's regime state.
        #[test]
        fn a_restored_heater_burns_as_one_that_never_stopped() {
            let start = || {
                let b = burning_mixer([1593, i64::from(heat::KINDLED), 1600, 500], true);
                let mut s = setup(&b, 2);
                sweep(
                    &mut s,
                    1,
                    100,
                    &[&[(ZINC, 10_000), (COPPER, 10_000), (COAL, 16)], &[]],
                );
                let mut e = engine();
                for t in &s {
                    feed(&mut e, t);
                }
                pass(&mut e);
                feed(&mut e, &load_state(200, LoadState::Unloaded));
                e
            };
            exact(
                &start,
                &[
                    To(1_000),
                    To(1_094),
                    To(20_000),
                    To(26_000),
                    To(30_000),
                    To(60_000),
                ],
            );
        }

        /// A partial load: the cut edge from the live store is paged with the replay.
        #[test]
        fn a_restored_partial_load_carries_its_cut_as_one_that_never_stopped() {
            let far = BlockPos::new(32, 64, 0);
            let away = far.chunk(0);
            let start = || {
                let (mut b, p) = press_line();
                b.nodes[p as usize].pos = far;
                b.nodes[1].pos = far;
                let mut s = setup(&b, 2);
                for t in &mut s {
                    if let Obs::Inventory(decl) = &mut t.obs
                        && decl.index == 1
                    {
                        decl.pos = far;
                    }
                }
                for c in [BlockPos::default().chunk(0), away] {
                    s.push(at(
                        0,
                        Obs::ChunkLoaded {
                            district: D,
                            chunk: c,
                            applied_epoch: 0,
                        },
                    ));
                }
                sweep(&mut s, 1, 100, &[&[(BRASS, 1000)], &[]]);
                let mut e = engine();
                for t in &s {
                    feed(&mut e, t);
                }
                pass(&mut e);
                feed(
                    &mut e,
                    &at(
                        200,
                        Obs::ChunkUnloaded {
                            district: D,
                            chunk: away,
                        },
                    ),
                );
                feed(
                    &mut e,
                    &at(
                        200,
                        Obs::LoadState {
                            district: D,
                            state: LoadState::Partial,
                            ticking_chunks: 1,
                            total_chunks: 2,
                        },
                    ),
                );
                e
            };
            exact(
                &start,
                &[
                    To(201),
                    To(3200),
                    To(20_000),
                    Feed(at(
                        20_000,
                        Obs::ChunkLoaded {
                            district: D,
                            chunk: away,
                            applied_epoch: 0,
                        },
                    )),
                    Feed(load_state(20_000, LoadState::Live)),
                    To(20_001),
                ],
            );
        }

        /// The residual's learned source is paged with the replay.
        #[test]
        fn a_restored_residual_replays_as_one_that_never_stopped() {
            let start = || {
                let mut b = B::default();
                let input = b.store(0);
                let out = b.store(1);
                let bb = b.node(NodeKind::BlackBox, model::BLACK_BOX, 0, Vec::new());
                b.edge(input, bb);
                b.edge_k(EdgeKind::Adjacency, bb, out, &[SHEET]);
                b.window = 2000;
                let mut s = setup(&b, 2);
                for k in 1..=35u32 {
                    let t = u64::from(k) * 100;
                    let m = (t / 10) as i64;
                    sweep(&mut s, k, t, &[&[(BRASS, 2000 - m)], &[(SHEET, m)]]);
                }
                s.push(load_state(3500, LoadState::Unloaded));
                let mut e = engine();
                for t in &s {
                    feed(&mut e, t);
                }
                e
            };
            exact(&start, &[To(3501), To(6500), To(40_000), To(60_000)]);
        }

        /// A real restart: the pages, then a session start. Java numbers the new session's
        /// compiles from 1, so the press's compile 5 is followed by a compile 1.
        fn restart(e: &Engine, at_: u64) -> Engine {
            let mut r = round_trip(e);
            r.session_start(Tick(at_));
            r
        }

        /// The press replays across a restart as it would across a session start without one;
        /// back, the same factory compiled anew keeps its passed window under its new number, so
        /// the next time away is a glass replay again at once.
        #[test]
        fn a_restart_keeps_the_replay_and_the_same_factory_keeps_its_window() {
            let (b, _) = press_line();
            let mut whole = press();
            whole.advance_to(Tick(6000));
            let mut cut = restart(&whole, 6000);
            whole.session_start(Tick(6000));
            for e in [&mut whole, &mut cut] {
                e.advance_to(Tick(8000));
                assert_eq!(model(e), replay_model::GLASS);
            }
            assert!(
                cut.snapshot() == whole.snapshot(),
                "a restart is a session start"
            );
            let e = &mut cut;
            for a in back(8000) {
                let Feed(t) = a else { unreachable!() };
                feed(e, &t);
            }
            assert_eq!(model(e), replay_model::NONE);
            for r in b.records(1) {
                feed(e, &at(8050, Obs::Glass(r)));
            }
            let dg = &e.glass.districts[&D];
            let c = dg.compiled.as_ref().expect("a compile");
            assert_eq!(c.graph.end.compile, 1);
            assert!(c.passed.is_some(), "the passed window is kept");
            assert!(!dg.numbers_reset);
            assert_eq!(e.glass.stats.compiles_kept, 1);
            assert_eq!(e.glass.stats.compiles_accepted, 1);
            assert_eq!(e.glass.stats.compiles_stale, 0);
            // A resend of it is ignored, as any resend of the compile held.
            let mut resend = b.records(1);
            if let Some(GlassObs::End(end)) = resend.last_mut() {
                end.flags |= end_flags::RESEND;
            }
            for r in resend {
                feed(e, &at(8060, Obs::Glass(r)));
            }
            assert_eq!(e.glass.stats.resends_ignored, 1);
            feed(e, &load_state(8100, LoadState::Unloaded));
            e.advance_to(Tick(8200));
            assert_eq!(model(e), replay_model::GLASS);
            assert_eq!(
                e.glass.districts[&D].replay.as_ref().map(|r| r.compile()),
                Some(1)
            );
            e.advance_to(Tick(30_000));
            assert_eq!(e.sim_stock(D).expect("replaying").get(BRASS), 0);
            e.check_books().expect("books");
        }

        /// After a restart, a compile of another factory (numbered 1, below the compile held) is
        /// recompiled, not ignored as stale: its window has not passed, so the press freezes.
        #[test]
        fn a_different_compile_after_a_restart_is_recompiled() {
            let (mut b, p) = press_line();
            b.nodes[p as usize].rates[0].ticks_num = 20;
            let mut e = press();
            e.advance_to(Tick(6000));
            let mut e = restart(&e, 6000);
            for a in back(8000) {
                let Feed(t) = a else { unreachable!() };
                feed(&mut e, &t);
            }
            for r in b.records(1) {
                feed(&mut e, &at(8050, Obs::Glass(r)));
            }
            let c = e.glass.districts[&D].compiled.as_ref().expect("a compile");
            assert_eq!(c.graph.end.compile, 1);
            assert!(c.passed.is_none());
            assert_eq!(e.glass.stats.compiles_kept, 0);
            assert_eq!(e.glass.stats.compiles_accepted, 2);
            feed(&mut e, &load_state(8100, LoadState::Unloaded));
            e.advance_to(Tick(9000));
            assert_eq!(model(&e), replay_model::NONE);
            assert!(e.sim_stock(D).is_none(), "frozen until a window passes");
        }

        /// What counts as the same factory (G2-81): a switch's level and output and a heater's
        /// burn and heat level are its state at the compile; their thresholds and burn per fuel,
        /// and every other record, are its build.
        #[test]
        fn the_same_factory_is_its_build_not_its_state() {
            fn complete(b: &B) -> crate::glass::store::Complete {
                let g = b.graph();
                crate::glass::store::Complete {
                    end: GraphEnd {
                        compile: 1,
                        ..g.end
                    },
                    nodes: g.nodes,
                    edges: g.edges,
                    recipes: g.recipes,
                    domains: g.domains,
                }
            }
            let same = |held: &B, b: &B| crate::glass::same_factory(&held.graph(), &complete(b));
            let kind =
                |b: &B, k: NodeKind| b.nodes.iter().position(|n| n.kind == k).expect("the node");
            let line = switched_line();
            let ctl = kind(&line, NodeKind::Controller);
            assert!(same(&line, &line));
            let mut b = switched_line();
            b.nodes[ctl].params[2] ^= 1;
            b.nodes[ctl].params[3] = 55;
            assert!(same(&line, &b), "the switch's state");
            for i in [0, 1] {
                let mut b = switched_line();
                b.nodes[ctl].params[i] += 1;
                assert!(!same(&line, &b), "the switch's threshold {i}");
            }
            let mixer = || burning_mixer([1593, i64::from(heat::KINDLED), 1600, 500], true);
            let held = mixer();
            let h = kind(&held, NodeKind::Heater);
            let mut b = mixer();
            b.nodes[h].params[0] = 20;
            b.nodes[h].params[1] = i64::from(heat::SEETHING);
            assert!(same(&held, &b), "the heater's state");
            for i in [2, 3] {
                let mut b = mixer();
                b.nodes[h].params[i] += 1;
                assert!(!same(&held, &b), "the heater's build {i}");
            }
            let mut b = mixer();
            b.edges[0].order += 1;
            assert!(!same(&held, &b), "an edge");
            let mut c = complete(&held);
            c.end.fingerprint = 1;
            assert!(
                !crate::glass::same_factory(&held.graph(), &c),
                "the fingerprint"
            );
        }

        /// An edit after a restore stops the restored replay, as it stops one that never stopped.
        #[test]
        fn an_edit_after_a_restore_stops_the_glass_replay() {
            let mut e = press();
            e.advance_to(Tick(5000));
            let mut e = round_trip(&e);
            assert_eq!(model(&e), replay_model::GLASS);
            feed(
                &mut e,
                &at(
                    5000,
                    Obs::Edit {
                        district: D,
                        kind: EditKind::Broken,
                        player: (0, 0),
                        pos: BlockPos::default(),
                    },
                ),
            );
            e.advance_to(Tick(5001));
            assert!(e.sim_stock(D).is_none(), "frozen after the edit");
            assert!(!e.glass.report(D).expect("a compile").replay.eligible);
        }

        /// A glass page cut short anywhere, with a byte too many, or of another format, is an
        /// error, never a panic, and the engine's restore fails with it.
        #[test]
        fn a_corrupt_or_short_glass_page_is_refused() {
            let mut e = press();
            e.advance_to(Tick(6000));
            let pages = e.snapshot();
            let (gk, gb) = pages
                .iter()
                .find(|(k, _)| k.table() == GLASS_TABLE)
                .cloned()
                .expect("a glass page");
            assert_eq!(gk.id(), D.0);
            assert!(gb == district_page(&e.glass.districts[&D]));
            let read = |b: &[u8]| read_district_page(b, MAX);
            assert!(read(&gb).expect("the page itself").kept().is_some());
            for n in 0..gb.len() {
                assert!(read(&gb[..n]).is_err(), "cut at {n}");
            }
            let mut long = gb.clone();
            long.push(0);
            assert!(read(&long).is_err());
            let mut other = gb.clone();
            other[..4].copy_from_slice(&(GLASS_MAGIC + 1).to_le_bytes());
            assert!(read(&other).is_err());
            for bad in [
                Vec::new(),
                gb[..gb.len() / 2].to_vec(),
                gb[..gb.len() - 1].to_vec(),
                other,
            ] {
                let mut p = pages.clone();
                for (k, b) in &mut p {
                    if *k == gk {
                        *b = bad.clone();
                    }
                }
                assert!(Engine::restore(p, Tunables::default()).is_err());
            }
        }

        /// The page of a replay of another compile's nodes is refused.
        #[test]
        fn a_glass_page_whose_replay_is_not_its_compiles_is_refused() {
            let mut e = press();
            e.advance_to(Tick(6000));
            let mut dg = e.glass.districts[&D].clone();
            dg.compiled.as_mut().expect("a compile").graph = switched_line().graph();
            assert!(read_district_page(&district_page(&dg), MAX).is_err());
        }

        /// `glass.sectionsMax` as the engine's tunables have it.
        const MAX: usize = 64;

        /// The glass pages of a snapshot, by key.
        fn glass_pages(pages: &Pages) -> Vec<(PageKey, Vec<u8>)> {
            pages
                .iter()
                .filter(|(k, _)| k.table() == GLASS_TABLE)
                .cloned()
                .collect()
        }

        /// A glass page of the format before sections (IGL1), or one whose section fingerprints
        /// are not those its compile cuts into now, drops its district's glass state alone
        /// (SECTIONS.md §5): the restore goes on, says which district and why, and the next save
        /// deletes the page. The district then learns again as one with no glass page.
        #[test]
        fn a_glass_page_of_the_old_format_or_other_sections_drops_its_district_alone() {
            assert_eq!(Tunables::default().glass.sections_max, MAX);
            let mut e = press();
            e.advance_to(Tick(6000));
            let pages = e.snapshot();
            let (gk, gb) = glass_pages(&pages).pop().expect("a glass page");
            let compiled = e.glass.districts[&D].compiled.as_ref().expect("a compile");
            assert!(!compiled.sections.is_empty(), "the press is a section");
            // The page as written: its fingerprints are its compile's.
            let PageRead::Kept(dg) = read_district_page(&gb, MAX).expect("decodes") else {
                panic!("the page as written is kept");
            };
            assert_eq!(
                dg.compiled
                    .as_ref()
                    .expect("a compile")
                    .sections
                    .fingerprints(),
                compiled.sections.fingerprints()
            );
            // An IGL1 page: dropped by its magic alone, whatever follows.
            let mut old = gb.clone();
            old[..4].copy_from_slice(&GLASS_MAGIC_IGL1.to_le_bytes());
            assert!(matches!(
                read_district_page(&old, MAX),
                Ok(PageRead::Dropped(_))
            ));
            assert!(matches!(
                read_district_page(&old[..4], MAX),
                Ok(PageRead::Dropped(_))
            ));
            // Fingerprints not the compile's: the last byte is the last fingerprint's.
            let mut other = gb.clone();
            *other.last_mut().expect("bytes") ^= 1;
            assert!(matches!(
                read_district_page(&other, MAX),
                Ok(PageRead::Dropped(_))
            ));
            for bad in [old, other] {
                let p: Pages = pages
                    .iter()
                    .map(|(k, b)| (*k, if *k == gk { bad.clone() } else { b.clone() }))
                    .collect();
                let mut disk: Disk = p.iter().cloned().collect();
                let mut r = Engine::restore(p, Tunables::default()).expect("the restore goes on");
                assert!(!r.glass.districts.contains_key(&D), "dropped");
                assert_eq!(
                    r.glass
                        .dropped_on_restore
                        .iter()
                        .map(|d| d.0)
                        .collect::<Vec<_>>(),
                    vec![D]
                );
                assert!(r.district(D).is_some(), "the district itself is kept");
                save(&mut r, &mut disk);
                assert!(!disk.contains_key(&gk), "the next save deletes the page");
                // The district goes on as one restored without its glass page.
                r.advance_to(Tick(7000));
                r.check_books().expect("books");
            }
        }

        /// A different `glass.sectionsMax` that cuts the compile otherwise drops the page; one
        /// that cuts it the same keeps it.
        #[test]
        fn a_changed_sections_cap_drops_a_page_only_when_it_cuts_otherwise() {
            // Two presses in a row on a middle store: two sections, which a cap of one merges.
            let mut b = B::default();
            let (input, mid, out) = (b.store(0), b.store(1), b.store(2));
            let r1 = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
            let r2 = b.recipe(&[(SHEET, 1)], &[(COPPER, 1)]);
            let p1 = b.machine(model::PRESS, r1, 1, 10);
            let p2 = b.machine(model::PRESS, r2, 1, 10);
            b.edge(input, p1);
            b.edge(p1, mid);
            b.edge(mid, p2);
            b.edge(p2, out);
            b.window = 2000;
            let mut s = setup_n(&b, 3, 5);
            for k in 1..=35u32 {
                let t = u64::from(k) * 100;
                let m = (t / 10) as i64;
                sweep(
                    &mut s,
                    k,
                    t,
                    &[&[(BRASS, 1000 - m)], &[(SHEET, 5)], &[(COPPER, m)]],
                );
            }
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            let (_, gb) = glass_pages(&e.snapshot()).pop().expect("a glass page");
            let n = e.glass.districts[&D]
                .compiled
                .as_ref()
                .expect("a compile")
                .sections
                .len();
            assert_eq!(n, 2, "a section per press");
            assert!(matches!(read_district_page(&gb, n), Ok(PageRead::Kept(_))));
            assert!(matches!(read_district_page(&gb, 64), Ok(PageRead::Kept(_))));
            assert!(matches!(
                read_district_page(&gb, n - 1),
                Ok(PageRead::Dropped(_))
            ));
        }

        /// Without its glass page (rtsim-view's inspect restores one district alone), a glass span
        /// goes on by the black box or freezes, its books whole.
        #[test]
        fn a_glass_span_restored_without_its_glass_page_goes_on() {
            let mut e = press();
            e.advance_to(Tick(6000));
            let pages: Pages = e
                .snapshot()
                .into_iter()
                .filter(|(k, _)| k.table() != GLASS_TABLE)
                .collect();
            let mut r = Engine::restore(pages, Tunables::default()).expect("restore");
            assert!(r.glass.districts.is_empty());
            r.advance_to(Tick(8000));
            assert_eq!(model(&r), 0);
            r.check_books().expect("books");
        }
    }

    /// Judging units (SECTIONS.md §3.4 to §3.6, §4, §7.2; slice S2b): each line of a district
    /// learns on its own window beside the district's.
    mod units {
        use super::*;
        use crate::glass::check::{Liveness, UnitState};
        use crate::glass::page::GLASS_MAGIC_IGL2;
        use crate::glass::page::{PageRead, district_page, read_district_page};
        use crate::glass::replay::Replay;
        use crate::glass::report::discard;
        use crate::ids::ChunkPos;

        /// Two press lines apart, each a section and a unit: brass (inventory 0) into press A
        /// into sheets (1) at x 0 to 2 (chunk 0), and `b_in` (2) into press B into `b_out` (3) at
        /// x 40 to 42 (chunk 2), each a press per 10 ticks (B's per `b_ticks`). Returns the
        /// builder and the two presses.
        pub(super) fn two_lines(b_in: Good, b_out: Good, b_ticks: u32) -> (B, u32, u32) {
            let mut b = B::default();
            let (i0, o0, i1, o1) = (b.store(0), b.store(1), b.store(2), b.store(3));
            let r0 = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
            let r1 = b.recipe(&[(b_in, 1)], &[(b_out, 1)]);
            let p0 = b.machine(model::PRESS, r0, 1, 10);
            let p1 = b.machine(model::PRESS, r1, 1, b_ticks);
            b.edge(i0, p0);
            b.edge(p0, o0);
            b.edge(i1, p1);
            b.edge(p1, o1);
            for (v, x) in [(i0, 0), (p0, 1), (o0, 2), (i1, 40), (p1, 41), (o1, 42)] {
                b.nodes[v as usize].pos = BlockPos::new(x, 64, 0);
            }
            b.window = 2000;
            (b, p0, p1)
        }

        /// The two lines' inventories when A made `a` and B made `bm`.
        fn invs(b_in: Good, b_out: Good, a: i64, bm: i64) -> VecMap<u32, Inventory> {
            let mut v = VecMap::new();
            v.insert(0, inventory(0, &[(BRASS, 100_000 - a)]));
            v.insert(1, inventory(1, &[(SHEET, a)]));
            v.insert(2, inventory(2, &[(b_in, 100_000 - bm)]));
            v.insert(3, inventory(3, &[(b_out, bm)]));
            v
        }

        /// Sweeps a compile every 100 ticks from `from` to `to` (both included).
        #[allow(clippy::too_many_arguments)]
        fn drive2(
            c: &mut Compiled,
            from: u64,
            to: u64,
            (b_in, b_out): (Good, Good),
            a: impl Fn(u64) -> i64,
            bm: impl Fn(u64) -> i64,
            tun: &GlassTunables,
        ) {
            let goods = goods();
            let ports = GoodVec::new();
            let mut t = from;
            while t <= to {
                let inv = invs(b_in, b_out, a(t), bm(t));
                let view = View {
                    revision: 1,
                    inventories: &inv,
                    ports: &ports,
                };
                if t == 0 {
                    c.start(&view, &goods, tun);
                }
                c.sweep(Tick(t), &view, &goods, tun);
                t += 100;
            }
        }

        fn unit_of(c: &Compiled, v: u32) -> u16 {
            c.units.of_node[v as usize]
        }

        fn verdict_of(st: &UnitState) -> u32 {
            st.last.as_ref().map_or(verdict::NONE, |r| r.verdict)
        }

        /// The compile's records for district `d`.
        fn records_for(b: &B, compile: u32, d: DistrictId) -> Vec<GlassObs> {
            let mut out = b.records(compile);
            for r in out.iter_mut() {
                match r {
                    GlassObs::Node(n) => n.head.district = d,
                    GlassObs::Edge(e) => e.head.district = d,
                    GlassObs::Recipe(x) => x.head.district = d,
                    GlassObs::Domain(x) => x.head.district = d,
                    GlassObs::End(e) => e.district = d,
                    _ => {}
                }
            }
            out
        }

        fn ingest_all(g: &mut Glass, t: u64, recs: &[GlassObs], tun: &GlassTunables) {
            let goods = goods();
            for r in recs {
                g.ingest(Tick(t), r, None, &goods, tun);
            }
        }

        /// [`ingest_all`] with the district's inventories known, as the engine has them after
        /// its IND_DISTRICT: a recompile carries the units it keeps only then.
        fn ingest_seen(
            g: &mut Glass,
            t: u64,
            recs: &[GlassObs],
            inv: &VecMap<u32, Inventory>,
            tun: &GlassTunables,
        ) {
            let goods = goods();
            let ports = GoodVec::new();
            let view = View {
                revision: 1,
                inventories: inv,
                ports: &ports,
            };
            for r in recs {
                g.ingest(Tick(t), r, Some(&view), &goods, tun);
            }
        }

        /// One sweep of district `d` through [`Glass::sweep`].
        #[allow(clippy::too_many_arguments)]
        fn glass_sweep(
            g: &mut Glass,
            d: DistrictId,
            t: u64,
            inv: &VecMap<u32, Inventory>,
            ports: &GoodVec,
            live: &Liveness<'_>,
            tun: &GlassTunables,
        ) {
            let goods = goods();
            let view = View {
                revision: 1,
                inventories: inv,
                ports,
            };
            g.sweep(Tick(t), d, &view, live, &goods, tun);
        }

        fn comp(g: &Glass, d: DistrictId) -> &Compiled {
            g.districts[&d].compiled.as_ref().expect("a compile")
        }

        /// Two lines with goods of their own: each is judged on its own window, so the slow one
        /// fails its window and the other passes and may replay, while the district's window (the
        /// checksum over both) fails on the slow line's goods alone.
        #[test]
        fn two_lines_are_judged_apart_and_the_slow_one_fails_alone() {
            let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let tun = GlassTunables::default();
            let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
            assert!(c.units.active() && c.units.len() == 2);
            let (ua, ub) = (unit_of(&c, p0), unit_of(&c, p1));
            assert_ne!(ua, ub);
            // B makes half what its press is compiled to.
            drive2(
                &mut c,
                0,
                3200,
                (ORE, CRUSHED),
                |t| (t / 10) as i64,
                |t| (t / 20) as i64,
                &tun,
            );
            let (a, bs) = (&c.unit_state[ua as usize], &c.unit_state[ub as usize]);
            assert_eq!(verdict_of(a), verdict::PASS);
            assert_eq!(verdict_of(bs), verdict::FAIL);
            assert_eq!((a.windows_completed, a.strikes), (1, 0));
            assert_eq!((bs.windows_completed, bs.strikes), (1, 1));
            assert_eq!(a.passed, Some((Tick(1200), 2000)));
            let ra = a.last.as_ref().expect("judged");
            let sheet = ra
                .goods
                .iter()
                .find(|g| g.good == SHEET)
                .expect("sheet row");
            assert!(close(sheet.measured, 0.1) && close(sheet.predicted, 0.1));
            assert!(ra.goods.iter().all(|g| g.good == BRASS || g.good == SHEET));
            // The district's window fails on B's goods only.
            let d = c.last.as_ref().expect("the district judged");
            assert_eq!(d.verdict, verdict::FAIL);
            for g in &d.goods {
                let failed = g.verdict == verdict::FAIL;
                assert_eq!(failed, g.good == ORE || g.good == CRUSHED, "{:?}", g.good);
            }
            assert!(c.unit_may_replay(ua));
            assert!(!c.unit_may_replay(ub));
            assert!(c.section_may_replay(c.sections.section_of[p0 as usize]));
            assert!(!c.section_may_replay(c.sections.section_of[p1 as usize]));
            // Each unit's next window opened where its last concluded.
            assert_eq!(c.unit_window(ua), Some((Tick(3200), 1)));
            // Goods of their own cost no extra rows (§7.2).
            assert_eq!(c.extra_rows(), 0);
        }

        /// Two lines of the same goods: a line whose own window passes still may not replay while
        /// the district's last window failed on a good it touches (§3.4).
        #[test]
        fn a_unit_does_not_replay_on_a_good_the_district_failed() {
            let (b, p0, p1) = two_lines(BRASS, SHEET, 10);
            let tun = GlassTunables::default();
            let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
            let (ua, ub) = (unit_of(&c, p0), unit_of(&c, p1));
            drive2(
                &mut c,
                0,
                3200,
                (BRASS, SHEET),
                |t| (t / 10) as i64,
                |t| (t / 20) as i64,
                &tun,
            );
            assert_eq!(verdict_of(&c.unit_state[ua as usize]), verdict::PASS);
            assert_eq!(verdict_of(&c.unit_state[ub as usize]), verdict::FAIL);
            assert_eq!(c.last.as_ref().map(|r| r.verdict), Some(verdict::FAIL));
            assert!(!c.unit_may_replay(ua), "the district failed on sheets");
            assert!(!c.unit_may_replay(ub));
            // Each unit holds a row per good, two goods past one row each.
            assert_eq!(c.extra_rows(), 2);
        }

        /// Both lines at their compiled rates: a unit's window and the district's both pass.
        fn both_passed(goods_b: (Good, Good)) -> (Compiled, u32, u32, GlassTunables) {
            let (b, p0, p1) = two_lines(goods_b.0, goods_b.1, 10);
            let tun = GlassTunables::default();
            let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
            drive2(
                &mut c,
                0,
                3200,
                goods_b,
                |t| (t / 10) as i64,
                |t| (t / 10) as i64,
                &tun,
            );
            (c, p0, p1, tun)
        }

        /// An edit stales the unit whose box holds it and drops its window and the district's;
        /// the other unit keeps its window and may replay. An edit in no box stales no unit; a
        /// command's stales every one (§3.6).
        #[test]
        fn an_edit_stales_only_the_unit_whose_box_holds_it() {
            let (mut c, p0, p1, tun) = both_passed((ORE, CRUSHED));
            let (ua, ub) = (unit_of(&c, p0), unit_of(&c, p1));
            assert!(c.unit_may_replay(ua) && c.unit_may_replay(ub));
            assert!(c.window().is_some());
            c.edit(Tick(3250), Some(BlockPos::new(41, 64, 0)), &tun);
            assert!(c.edited && c.window().is_none(), "the district window goes");
            let (a, bs) = (&c.unit_state[ua as usize], &c.unit_state[ub as usize]);
            assert!(!a.edited && bs.edited);
            assert_eq!(c.unit_window(ua), Some((Tick(3200), 1)));
            assert_eq!(c.unit_window(ub), None);
            assert_eq!(bs.ready_at, Tick(3250 + tun.warmup_ticks));
            assert!(c.unit_may_replay(ua) && !c.unit_may_replay(ub));
            // Between the lines: no box holds it.
            c.edit(Tick(3260), Some(BlockPos::new(20, 64, 0)), &tun);
            assert!(!c.unit_state[ua as usize].edited);
            assert_eq!(c.unit_window(ua), Some((Tick(3200), 1)));
            // A command's edit has no place: every unit.
            c.edit(Tick(3270), None, &tun);
            assert!(c.unit_state.iter().all(|s| s.edited && !s.window_open()));
            assert!(!c.unit_may_replay(ua));
        }

        /// A sweep that was not live: the district's window is discarded (`NOT_LIVE`), and the
        /// unit whose chunks all ran samples and passes; the other never opens. A chunk that
        /// stopped between two sweeps discards its unit's window though it runs again (§3.4).
        #[test]
        fn a_unit_whose_chunks_ran_learns_through_partial_sweeps() {
            let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let tun = GlassTunables::default();
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            let (ua, ub) = (unit_of(comp(&g, D), p0), unit_of(comp(&g, D), p1));
            let loaded: BTreeSet<ChunkPos> = [ChunkPos { dim: 0, x: 0, z: 0 }].into();
            let ports = GoodVec::new();
            let live = Liveness::Partial {
                loaded: &loaded,
                dim: 0,
            };
            for k in 0..=32u64 {
                let t = k * 100;
                // B made nothing it could be judged by: its chunk is away.
                let inv = invs(ORE, CRUSHED, (t / 10) as i64, 0);
                glass_sweep(&mut g, D, t, &inv, &ports, &live, &tun);
            }
            let dg = &g.districts[&D];
            assert_eq!(dg.discards[discard::NOT_LIVE as usize], 33);
            let c = comp(&g, D);
            assert!(c.window().is_none() && c.last.is_none());
            assert_eq!(c.sweeps, 0, "no live sweep");
            assert_eq!(verdict_of(&c.unit_state[ua as usize]), verdict::PASS);
            assert_eq!(c.unit_state[ub as usize].last, None);
            assert_eq!(c.unit_window(ub), None);
            assert_eq!(c.unit_window(ua), Some((Tick(3200), 1)));
            // Chunk 0 unloads and loads again before the next sweep: A did not run throughout.
            g.chunk_stopped(D, ChunkPos { dim: 0, x: 0, z: 0 });
            let inv = invs(ORE, CRUSHED, 330, 0);
            glass_sweep(&mut g, D, 3300, &inv, &ports, &live, &tun);
            let c = comp(&g, D);
            assert_eq!(c.unit_window(ua), None);
            assert_eq!(
                c.unit_state[ua as usize].ready_at,
                Tick(3300 + tun.warmup_ticks)
            );
            assert!(
                g.districts[&D].stopped.is_empty(),
                "the sweep took the stops"
            );
            // No unit ran: today's discard of everything.
            let none: BTreeSet<ChunkPos> = BTreeSet::new();
            let away = Liveness::Partial {
                loaded: &none,
                dim: 0,
            };
            glass_sweep(&mut g, D, 3400, &inv, &ports, &away, &tun);
            assert_eq!(g.districts[&D].discards[discard::NOT_LIVE as usize], 35);
        }

        /// A recompile keeps the unit it did not change: its passed window, its open window (the
        /// old model integrated up to the recompile) and its ports, and clears its edit; the
        /// changed unit starts learning after a warm-up (§3.6).
        #[test]
        fn a_recompile_keeps_the_unit_it_did_not_change() {
            let tun = GlassTunables::default();
            let (b, p0, _) = two_lines(ORE, CRUSHED, 10);
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            let ports = GoodVec::new();
            let sweep_to = |g: &mut Glass, from: u64, to: u64, b_rate: u64| {
                let mut t = from;
                while t <= to {
                    let inv = invs(ORE, CRUSHED, (t / 10) as i64, (t / b_rate) as i64);
                    glass_sweep(g, D, t, &inv, &ports, &Liveness::All, &tun);
                    t += 100;
                }
            };
            sweep_to(&mut g, 0, 3500, 10);
            let ua = unit_of(comp(&g, D), p0);
            let before = comp(&g, D).unit_state[ua as usize].clone();
            assert_eq!(comp(&g, D).unit_window(ua), Some((Tick(3200), 4)));
            // A stales itself by an edit at its own press, then B's press is rebuilt slower.
            g.edit(Tick(3520), D, BlockPos::new(1, 64, 0), false, &tun);
            g.port(D, 1, 1, &GoodVec::from_pairs([(SHEET, 7)]));
            let (b2, p0b, p1b) = two_lines(ORE, CRUSHED, 20);
            let inv = invs(ORE, CRUSHED, 350, 350);
            ingest_seen(&mut g, 3550, &b2.records(2), &inv, &tun);
            let c = comp(&g, D);
            assert_eq!(c.graph.end.compile, 2);
            assert_eq!(g.districts[&D].discards[discard::RECOMPILE as usize], 1);
            let (ua2, ub2) = (unit_of(c, p0b), unit_of(c, p1b));
            let a = &c.unit_state[ua2 as usize];
            assert_eq!(a.passed, before.passed);
            assert_eq!(a.windows_completed, 1);
            assert!(!a.edited, "the recompile found it unchanged");
            // The edit took A's window; it opens again after its warm-up.
            assert_eq!(c.unit_window(ua2), None);
            assert_eq!(a.ready_at, Tick(3520 + tun.warmup_ticks));
            let bs = &c.unit_state[ub2 as usize];
            assert_eq!((bs.passed, bs.windows_completed), (None, 0));
            assert_eq!(bs.ready_at, Tick(3550 + tun.warmup_ticks));
            assert_eq!(
                c.ports_at.get(&1).map(|v| v.get(SHEET)),
                Some(7),
                "the ports go on"
            );
            assert!(c.window().is_none());
        }

        /// An open window carried across a recompile goes on and passes: the old compile's model
        /// integrated it to the recompile, the new one from there.
        #[test]
        fn a_window_carried_across_a_recompile_passes() {
            let tun = GlassTunables::default();
            let (b, p0, _) = two_lines(ORE, CRUSHED, 10);
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            let ports = GoodVec::new();
            let mut t = 0;
            while t <= 6000 {
                let b_made = if t < 4050 {
                    t / 10
                } else {
                    405 + (t - 4050) / 20
                };
                let inv = invs(ORE, CRUSHED, (t / 10) as i64, b_made as i64);
                glass_sweep(&mut g, D, t, &inv, &ports, &Liveness::All, &tun);
                if t == 4000 {
                    // B's press rebuilt slower: a recompile between two sweeps.
                    let (b2, _, _) = two_lines(ORE, CRUSHED, 20);
                    ingest_seen(&mut g, 4050, &b2.records(2), &inv, &tun);
                }
                t += 100;
            }
            let c = comp(&g, D);
            assert_eq!(c.graph.end.compile, 2);
            let ua = unit_of(c, p0);
            let a = &c.unit_state[ua as usize];
            assert_eq!(a.windows_completed, 2);
            assert_eq!(
                a.passed,
                Some((Tick(3200), 2000)),
                "the carried window passed"
            );
            assert_eq!(verdict_of(a), verdict::PASS);
        }

        /// Ports into a unit's store are not its production: a player's sheets put into A's
        /// output, credited to that inventory, leave A's window passing; uncredited they fail it.
        #[test]
        fn ports_into_a_units_store_are_not_its_production() {
            let tun = GlassTunables::default();
            let (b, p0, _) = two_lines(ORE, CRUSHED, 10);
            for credit in [true, false] {
                let mut g = Glass::default();
                ingest_all(&mut g, 0, &b.records(1), &tun);
                for k in 0..=32u64 {
                    let t = k * 100;
                    let extra = if t >= 2000 { 500 } else { 0 };
                    if t == 2000 && credit {
                        g.port(D, 1, 1, &GoodVec::from_pairs([(SHEET, 500)]));
                    }
                    // The district's own ports always count the sheets.
                    let ports = GoodVec::from_pairs([(SHEET, extra)]);
                    let mut inv = invs(ORE, CRUSHED, (t / 10) as i64, (t / 10) as i64);
                    inv.insert(1, inventory(1, &[(SHEET, (t / 10) as i64 + extra)]));
                    glass_sweep(&mut g, D, t, &inv, &ports, &Liveness::All, &tun);
                }
                let c = comp(&g, D);
                let ua = unit_of(c, p0);
                assert_eq!(c.last.as_ref().map(|r| r.verdict), Some(verdict::PASS));
                let want = if credit { verdict::PASS } else { verdict::FAIL };
                assert_eq!(
                    verdict_of(&c.unit_state[ua as usize]),
                    want,
                    "credit {credit}"
                );
            }
            // A port into an inventory no unit holds, or of another revision, is not kept.
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            g.port(D, 1, 9, &GoodVec::from_pairs([(SHEET, 5)]));
            g.port(D, 2, 1, &GoodVec::from_pairs([(SHEET, 5)]));
            assert!(comp(&g, D).ports_at.is_empty());
        }

        /// The row caps (§7.2): a district past `glass.sectionRowsMax` extra rows is cut into
        /// fewer sections and keeps the cut for its later compiles and through its page; past
        /// `glass.sectionRowsWorldMax` the district with the most (the lowest id of equals) is cut.
        #[test]
        fn the_row_caps_cut_a_district_into_fewer_sections() {
            let tun = GlassTunables {
                section_rows_max: 1,
                ..GlassTunables::default()
            };
            let (b, _, _) = two_lines(BRASS, SHEET, 10);
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            assert_eq!(comp(&g, D).sections.len(), 2);
            let ports = GoodVec::new();
            for k in 0..=13u64 {
                let t = k * 100;
                let inv = invs(BRASS, SHEET, (t / 10) as i64, (t / 10) as i64);
                glass_sweep(&mut g, D, t, &inv, &ports, &Liveness::All, &tun);
            }
            let c = comp(&g, D);
            assert_eq!(c.sections.len(), 1, "merged by the cap");
            assert!(c.unit_state.is_empty());
            assert_eq!(g.districts[&D].sections_cap, Some(1));
            assert_eq!(g.extra_rows(), 0);
            assert!(c.window().is_some(), "the district window goes on");
            // Its page keeps the cut.
            let page = district_page(&g.districts[&D]);
            let PageRead::Kept(dg) = read_district_page(&page, 64).expect("decodes") else {
                panic!("kept");
            };
            assert_eq!(dg.compiled.as_ref().expect("compile").sections.len(), 1);
            // A later compile is cut the same.
            ingest_all(&mut g, 1400, &b.records(2), &tun);
            assert_eq!(comp(&g, D).sections.len(), 1);

            // The world cap: two such districts, four extra rows, three allowed.
            let tun = GlassTunables {
                section_rows_world_max: 3,
                ..GlassTunables::default()
            };
            let (d1, d2) = (DistrictId(3), DistrictId(4));
            let mut g = Glass::default();
            for d in [d2, d1] {
                ingest_all(&mut g, 0, &records_for(&b, 1, d), &tun);
            }
            for k in 0..=13u64 {
                let t = k * 100;
                let inv = invs(BRASS, SHEET, (t / 10) as i64, (t / 10) as i64);
                for d in [d2, d1] {
                    glass_sweep(&mut g, d, t, &inv, &ports, &Liveness::All, &tun);
                }
            }
            assert_eq!(
                g.districts[&d1].sections_cap,
                Some(1),
                "the lower id of equals"
            );
            assert_eq!(g.districts[&d2].sections_cap, None);
            assert_eq!(comp(&g, d2).sections.len(), 2);
            assert_eq!(g.extra_rows(), 2);
        }

        /// A page keeps the units' windows, rows, verdicts and ports: read back, it is the state
        /// it was written from, byte for byte. An IGL2 page drops its district (§5).
        #[test]
        fn a_page_keeps_the_units_windows() {
            let tun = GlassTunables::default();
            let (b, _, _) = two_lines(ORE, CRUSHED, 10);
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            let ports = GoodVec::new();
            for k in 0..=35u64 {
                let t = k * 100;
                let inv = invs(ORE, CRUSHED, (t / 10) as i64, (t / 20) as i64);
                glass_sweep(&mut g, D, t, &inv, &ports, &Liveness::All, &tun);
            }
            g.port(D, 1, 1, &GoodVec::from_pairs([(SHEET, 3)]));
            let dg = &g.districts[&D];
            let c = dg.compiled.as_ref().expect("compile");
            assert!(c.rows_len() > 0 && c.unit_state.iter().all(|s| s.last.is_some()));
            let page = district_page(dg);
            let PageRead::Kept(back) = read_district_page(&page, 64).expect("decodes") else {
                panic!("kept");
            };
            let r = back.compiled.as_ref().expect("compile");
            assert_eq!(r.units, c.units);
            assert_eq!(r.ubounds, c.ubounds);
            assert_eq!(r.unit_state, c.unit_state);
            assert_eq!(r.rows_len(), c.rows_len());
            assert_eq!(r.ports_at, c.ports_at);
            assert_eq!(district_page(&back), page);
            let mut old = page.clone();
            old[..4].copy_from_slice(&GLASS_MAGIC_IGL2.to_le_bytes());
            assert!(matches!(
                read_district_page(&old, 64),
                Ok(PageRead::Dropped(_))
            ));
        }

        /// Through the engine: a partial load learns the loaded line (its chunk loaded, the sweep
        /// not live), an `IND_EDIT` at the other line's press stales that line alone, and a save
        /// and restore keeps the units' state.
        #[test]
        fn the_engine_judges_units_through_its_hooks() {
            let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let mut s = setup(&b, 4);
            s.push(load_state(0, LoadState::Partial));
            s.push(at(
                0,
                Obs::ChunkLoaded {
                    district: D,
                    chunk: ChunkPos { dim: 0, x: 0, z: 0 },
                    applied_epoch: 0,
                },
            ));
            for k in 1..=32u32 {
                let t = u64::from(k) * 100;
                let m = (t / 10) as i64;
                sweep(
                    &mut s,
                    k,
                    t,
                    &[
                        &[(BRASS, 100_000 - m)],
                        &[(SHEET, m)],
                        &[(ORE, 100_000)],
                        &[],
                    ],
                );
                // Some chunk did not tick throughout.
                if let Some(Timed {
                    obs: Obs::SweepEnd(e),
                    ..
                }) = s.last_mut()
                {
                    e.live = false;
                }
            }
            s.push(at(
                3250,
                Obs::Edit {
                    district: D,
                    kind: EditKind::Placed,
                    player: (1, 2),
                    pos: BlockPos::new(41, 64, 0),
                },
            ));
            let mut e = engine();
            for t in &s {
                feed(&mut e, t);
            }
            let c = e.glass.districts[&D].compiled.as_ref().expect("compile");
            let (ua, ub) = (c.units.of_node[p0 as usize], c.units.of_node[p1 as usize]);
            let a = &c.unit_state[ua as usize];
            assert_eq!(verdict_of(a), verdict::PASS);
            assert!(!a.edited && c.unit_state[ub as usize].edited);
            assert!(c.unit_may_replay(ua) && !c.unit_may_replay(ub));
            let r = Engine::restore(e.snapshot(), Tunables::default()).expect("restore");
            let rc = r.glass.districts[&D].compiled.as_ref().expect("compile");
            assert_eq!(rc.unit_state, c.unit_state);
            assert_eq!(r.glass.extra_rows(), e.glass.extra_rows());
        }

        // ---- the replay by unit (SECTIONS.md §3.5, slice S2c) ------------------------------------

        /// An inventory of a test district: its block, a fluid tank or not, and what it holds.
        pub(super) struct Inv {
            pos: BlockPos,
            fluid: bool,
            contents: Vec<(Good, i64)>,
        }

        pub(super) fn inv_at(x: i32, contents: &[(Good, i64)]) -> Inv {
            Inv {
                pos: BlockPos::new(x, 64, 0),
                fluid: false,
                contents: contents.to_vec(),
            }
        }

        /// The inventories of [`two_lines`] at their stores' blocks (A's in chunk 0, B's in chunk
        /// 2): A's brass and B's input as given.
        pub(super) fn lines(brass: i64, b_in: (Good, i64)) -> Vec<Inv> {
            vec![
                inv_at(0, &[(BRASS, brass)]),
                inv_at(2, &[]),
                inv_at(40, &[b_in]),
                inv_at(42, &[]),
            ]
        }

        pub(super) fn chunk(x: i32) -> ChunkPos {
            BlockPos::new(x, 64, 0).chunk(0)
        }

        /// `b`'s compile in an engine on tunables `tun`: the district live with `invs`, and one
        /// live sweep at 100 of what they hold.
        pub(super) fn engine_with(b: &B, tun: Tunables, invs: &[Inv]) -> Engine {
            let mut s = vec![at(
                0,
                Obs::District(DistrictDecl {
                    district: D,
                    revision: 1,
                    dim: 0,
                    name: 0,
                    not_learnable: None,
                    inventory_count: invs.len() as u32,
                }),
            )];
            for (i, x) in invs.iter().enumerate() {
                let mut d = inventory(i as u32, &[]).decl;
                d.pos = x.pos;
                if x.fluid {
                    d.kind = InvKind(InvKind::FLUIDS);
                    d.item_slots = 0;
                    d.fluid_capacity = 1_000_000_000;
                }
                s.push(at(0, Obs::Inventory(d)));
            }
            s.push(load_state(0, LoadState::Live));
            for r in b.records(1) {
                s.push(at(0, Obs::Glass(r)));
            }
            let contents: Vec<&[(Good, i64)]> =
                invs.iter().map(|x| x.contents.as_slice()).collect();
            sweep(&mut s, 1, 100, &contents);
            let mut e = Engine::new(0, tun, Tick::ZERO);
            for (g, info) in goods() {
                e.define_good(g, info);
            }
            e.session_start(Tick(0));
            for t in &s {
                feed(&mut e, t);
            }
            e
        }

        /// Sets the last window of each node's unit to `verdict` by hand (passed when it passes),
        /// with no district window concluded: the replay's own mechanics, without driving windows
        /// that match every regime.
        pub(super) fn judge(e: &mut Engine, verdicts: &[(u32, u32)]) {
            let c = e
                .glass
                .districts
                .get_mut(&D)
                .and_then(|g| g.compiled.as_mut())
                .expect("a compile");
            for &(v, verdict) in verdicts {
                let u = c.units.of_node[v as usize] as usize;
                let st = &mut c.unit_state[u];
                st.last = Some(WindowResult {
                    start: Tick(0),
                    ticks: 1,
                    verdict,
                    goods: Vec::new(),
                    stores: Vec::new(),
                });
                st.passed = (verdict == verdict::PASS).then_some((Tick(0), 1));
                st.edited = false;
            }
            c.last = None;
        }

        fn replay(e: &Engine) -> &Replay {
            e.glass.districts[&D].replay.as_ref().expect("replaying")
        }

        /// The district back at `t` with the chunks of blocks `xs` loaded: each store's change.
        fn back(e: &mut Engine, t: u64, xs: &[i32]) -> BTreeMap<(u32, Good), i64> {
            let mut cmds = feed(e, &load_state(t, LoadState::Live));
            for &x in xs {
                cmds.extend(feed(
                    e,
                    &at(
                        t,
                        Obs::ChunkLoaded {
                            district: D,
                            chunk: chunk(x),
                            applied_epoch: 0,
                        },
                    ),
                ));
            }
            deltas(&cmds)
        }

        /// A unit that may not replay is gated off while the other replays (§3.5): its press runs
        /// nothing, and its stores are pools only the world's own changes touch (a package into
        /// one stays there). Before S2c the district window's failure froze the whole district.
        #[test]
        fn a_unit_that_may_not_replay_is_gated_and_its_stores_keep_what_they_hold() {
            use crate::obs::{Direction, PackageEvent};
            let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let mut e = engine_with(&b, Tunables::default(), &lines(1000, (ORE, 1000)));
            judge(&mut e, &[(p0, verdict::PASS), (p1, verdict::FAIL)]);
            feed(&mut e, &load_state(200, LoadState::Unloaded));
            e.advance_to(Tick(201));
            assert_eq!(model(&e), replay_model::GLASS);
            let c = comp(&e.glass, D);
            let (ua, ub) = (unit_of(c, p0) as usize, unit_of(c, p1) as usize);
            let rp = replay(&e);
            assert!(rp.units_on()[ua] && !rp.units_on()[ub]);
            assert!(rp.gated()[p1 as usize] && !rp.gated()[p0 as usize]);
            assert!(rp.frozen().iter().all(|&f| !f));
            feed(
                &mut e,
                &at(
                    1000,
                    Obs::Package(PackageEvent {
                        district: D,
                        inventory: 2,
                        direction: Direction::In,
                        address: 0,
                        packages: 1,
                        op: None,
                        goods: GoodVec::from_pairs([(ORE, 64)]),
                    }),
                ),
            );
            e.advance_to(Tick(3200));
            let st = e.sim_stock(D).expect("replaying");
            assert!((298..=300).contains(&st.get(SHEET)), "{st:?}");
            assert_eq!((st.get(ORE), st.get(CRUSHED)), (1064, 0), "{st:?}");
            let d = back(&mut e, 3200, &[0, 40]);
            let made = d.get(&(1, SHEET)).copied().unwrap_or(0);
            assert!((298..=300).contains(&made), "{d:?}");
            assert!((-300..=-298).contains(&d.get(&(0, BRASS)).copied().unwrap_or(0)));
            assert!(
                d.keys().all(|&(i, _)| i < 2),
                "B's stores keep what they hold: {d:?}"
            );
            e.check_books().expect("books");
        }

        /// No unit may replay: the district freezes while away (staleness did it, I5), with no
        /// replay and no black box. One unit that may is enough for the glass (§3.5).
        #[test]
        fn a_district_freezes_while_no_unit_may_replay() {
            for (vb, want) in [
                (verdict::FAIL, replay_model::NONE),
                (verdict::PASS, replay_model::GLASS),
            ] {
                let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
                let mut e = engine_with(&b, Tunables::default(), &lines(1000, (ORE, 1000)));
                judge(&mut e, &[(p0, verdict::FAIL), (p1, vb)]);
                feed(&mut e, &load_state(200, LoadState::Unloaded));
                e.advance_to(Tick(3000));
                assert_eq!(model(&e), want);
                let r = e.glass.report(D).expect("a compile");
                assert_eq!(r.replay.eligible, want == replay_model::GLASS);
                assert_eq!(e.sim_stock(D).is_some(), want == replay_model::GLASS);
            }
        }

        /// A fallback freezes the unit it names, at its tick, keeping what it booked, and the rest
        /// replays on (§3.5). With no breakpoint allowed between outside events, A's brass running
        /// out falls back: A freezes, once, and B goes on. B's own fallback later leaves no unit
        /// to replay, and only then does the district leave the glass.
        #[test]
        fn a_fallback_freezes_the_unit_it_names_and_the_rest_replays_on() {
            let mut tun = Tunables::default();
            tun.glass.breakpoints_per_event_max = 0;
            let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let mut e = engine_with(&b, tun, &lines(500, (ORE, 2000)));
            judge(&mut e, &[(p0, verdict::PASS), (p1, verdict::PASS)]);
            feed(&mut e, &load_state(200, LoadState::Unloaded));
            e.advance_to(Tick(201));
            assert_eq!(model(&e), replay_model::GLASS);
            let c = comp(&e.glass, D);
            let (ua, ub) = (unit_of(c, p0) as usize, unit_of(c, p1) as usize);
            assert_eq!(replay(&e).freezes(), 0);
            // A's 500 brass runs out about 5,200.
            e.advance_to(Tick(8000));
            assert_eq!(model(&e), replay_model::GLASS);
            let rp = replay(&e);
            assert_eq!(rp.freezes(), 1);
            assert_eq!(rp.counts().freezes, 1);
            assert!(rp.frozen()[ua] && !rp.frozen()[ub]);
            assert!(rp.gated()[p0 as usize] && !rp.gated()[p1 as usize]);
            let st = e.sim_stock(D).expect("replaying");
            assert_eq!(st.get(BRASS), 0, "{st:?}");
            assert!(
                (499..=500).contains(&st.get(SHEET)),
                "A's bookings kept: {st:?}"
            );
            assert!((778..=780).contains(&st.get(CRUSHED)), "{st:?}");
            // B's 2,000 ore runs out about 20,200: no unit is left to replay.
            e.advance_to(Tick(30_000));
            assert!(e.glass.districts[&D].fallback.is_some());
            assert_ne!(model(&e), replay_model::GLASS);
            let d = back(&mut e, 30_000, &[0, 40]);
            assert_eq!(d.get(&(0, BRASS)), Some(&-500), "{d:?}");
            assert!((499..=500).contains(&d[&(1, SHEET)]), "{d:?}");
            assert_eq!(d.get(&(2, ORE)), Some(&-2000), "{d:?}");
            assert!((1999..=2000).contains(&d[&(3, CRUSHED)]), "{d:?}");
            e.check_books().expect("books");
        }

        /// [`two_lines`] with B's press on a network a boiler drives, the boiler's pump drawing
        /// from a water tank (inventory 4, at x 80): three units. Returns the builder and A's
        /// press, B's and the boiler.
        pub(super) fn boiler_lines() -> (B, [u32; 3]) {
            let (mut b, p0, p1) = two_lines(ORE, CRUSHED, 10);
            let water = b.store(4);
            b.nodes[water as usize].model = model::STORE_FLUID;
            b.nodes[water as usize].flags = node_flags::FLUID;
            let pump = b.node(
                NodeKind::Mover,
                model::PUMP,
                node_flags::FLUID,
                vec![RateEntry {
                    flags: rate_flags::SHARED,
                    ..entry(None, 40, 1)
                }],
            );
            let boiler = b.node(
                NodeKind::Boiler,
                model::BOILER,
                node_flags::FLUID,
                Vec::new(),
            );
            b.nodes[boiler as usize].params = [16, 1, 0, 0];
            let h = b.node(NodeKind::Heater, model::BLAZE_BURNER, 0, Vec::new());
            b.nodes[h as usize].params = [100_000_000, i64::from(heat::KINDLED), 1600, 0];
            for (v, x) in [(water, 80), (pump, 81), (boiler, 82), (h, 83)] {
                b.nodes[v as usize].pos = BlockPos::new(x, 64, 0);
            }
            b.edge_k(EdgeKind::Pipe, water, pump, &[WATER]);
            b.edge_k(EdgeKind::Pipe, pump, boiler, &[WATER]);
            let he = b.edge_k(EdgeKind::Heat, h, boiler, &[]);
            b.edges[he as usize].param = 4;
            let r = b.nodes[p1 as usize].rates[0].recipe;
            b.nodes[p1 as usize].rates = vec![
                RateEntry {
                    level: 1,
                    ..entry(r, 1, 20)
                },
                RateEntry {
                    level: 2,
                    ..entry(r, 1, 10)
                },
            ];
            b.nodes[p1 as usize].domain = Some(0);
            let level = |selector: u32, speed: i32, capacity: f64| DomainLevel {
                selector,
                speed_num: speed,
                speed_den: 1,
                capacity,
            };
            b.domains.push(GraphDomain {
                head: head(0),
                network: 1,
                current: 2,
                driver: Some(boiler),
                flags: 0,
                live_stress: 0.0,
                live_capacity: 0.0,
                levels: vec![level(0, 0, 0.0), level(2, 16, 1000.0), level(4, 32, 4096.0)],
                members: vec![Member {
                    node: Some(p1),
                    count: 1,
                    ratio_num: 1,
                    ratio_den: 1,
                    impact: 8.0,
                }],
            });
            (b, [p0, p1, boiler])
        }

        /// A boiler whose unit may not replay stops its network for every unit on it (§3.5): B
        /// passed its window, yet makes nothing while the boiler is gated; A, on no network, runs.
        /// With the boiler's unit allowed, B runs at the boiler's level.
        #[test]
        fn a_gated_boiler_stops_its_network() {
            for boiler_may in [false, true] {
                let (b, [p0, p1, boiler]) = boiler_lines();
                let mut invs = lines(1000, (ORE, 1000));
                invs.push(Inv {
                    pos: BlockPos::new(80, 64, 0),
                    fluid: true,
                    contents: vec![(WATER, 100_000_000)],
                });
                let mut e = engine_with(&b, Tunables::default(), &invs);
                let c = comp(&e.glass, D);
                assert_eq!(c.units.len(), 3);
                let vk = if boiler_may {
                    verdict::PASS
                } else {
                    verdict::FAIL
                };
                judge(
                    &mut e,
                    &[(p0, verdict::PASS), (p1, verdict::PASS), (boiler, vk)],
                );
                feed(&mut e, &load_state(200, LoadState::Unloaded));
                e.advance_to(Tick(3200));
                assert_eq!(model(&e), replay_model::GLASS);
                let rp = replay(&e);
                assert_eq!(rp.gated()[p1 as usize], !boiler_may, "B's press");
                assert_eq!(rp.gated()[boiler as usize], !boiler_may);
                assert!(!rp.gated()[p0 as usize]);
                let st = e.sim_stock(D).expect("replaying");
                assert!((298..=300).contains(&st.get(SHEET)), "{st:?}");
                if boiler_may {
                    assert!((296..=300).contains(&st.get(CRUSHED)), "{st:?}");
                    assert!(st.get(WATER) < 100_000_000, "{st:?}");
                } else {
                    assert_eq!(st.get(CRUSHED), 0, "{st:?}");
                    assert_eq!(st.get(WATER), 100_000_000, "{st:?}");
                }
            }
        }

        /// The domain check (§3.6): a recompile keeps a unit on a network while the network's
        /// stress stays at or below the least capacity its passed window ran at, and stales it
        /// past that. A load added elsewhere on the network changes no fingerprint, so only the
        /// domain check sees it.
        #[test]
        fn the_domain_check_stales_only_past_the_least_capacity_run_at() {
            use crate::glass::check::DomCap;
            let tun = GlassTunables::default();
            let with_load = |others: u32| {
                let (mut b, p0, p1) = two_lines(ORE, CRUSHED, 10);
                b.nodes[p1 as usize].domain = Some(0);
                let member = |node: Option<u32>, count: u32| Member {
                    node,
                    count,
                    ratio_num: 1,
                    ratio_den: 1,
                    impact: 4.0,
                };
                b.domains.push(GraphDomain {
                    head: head(0),
                    network: 1,
                    current: 0,
                    driver: None,
                    flags: 0,
                    live_stress: 0.0,
                    live_capacity: 0.0,
                    levels: vec![DomainLevel {
                        selector: 0,
                        speed_num: 64,
                        speed_den: 1,
                        capacity: 1024.0,
                    }],
                    members: vec![member(Some(p1), 1), member(None, others)],
                });
                (b, p0, p1)
            };
            let (b, p0, p1) = with_load(1);
            let mut g = Glass::default();
            ingest_all(&mut g, 0, &b.records(1), &tun);
            let ports = GoodVec::new();
            for k in 0..=35u64 {
                let t = k * 100;
                let inv = invs(ORE, CRUSHED, (t / 10) as i64, (t / 10) as i64);
                glass_sweep(&mut g, D, t, &inv, &ports, &Liveness::All, &tun);
            }
            let c = comp(&g, D);
            let (ua, ub) = (unit_of(c, p0), unit_of(c, p1));
            assert!(c.unit_may_replay(ua) && c.unit_may_replay(ub));
            assert_eq!(
                c.unit_state[ub as usize].passed_caps,
                vec![DomCap {
                    domain: 0,
                    selector: 0,
                    capacity: 1024.0
                }]
            );
            assert!(c.unit_state[ua as usize].passed_caps.is_empty());
            let inv = invs(ORE, CRUSHED, 350, 350);
            // Two more loads elsewhere on the network: 768 of the 1,024 B ran at. B is kept.
            let (b2, _, _) = with_load(2);
            ingest_seen(&mut g, 3550, &b2.records(2), &inv, &tun);
            let c = comp(&g, D);
            assert_eq!(c.graph.end.compile, 2);
            assert!(c.unit_may_replay(ua) && c.unit_may_replay(ub));
            assert_eq!(c.unit_state[ub as usize].passed_caps.len(), 1);
            // Four: 1,280, past it. B goes stale and learns again; A is kept.
            let (b3, _, _) = with_load(4);
            ingest_seen(&mut g, 3600, &b3.records(3), &inv, &tun);
            let c = comp(&g, D);
            assert_eq!(c.graph.end.compile, 3);
            let bs = &c.unit_state[ub as usize];
            assert!(bs.edited && bs.passed_caps.is_empty());
            assert_eq!(c.unit_window(ub), None);
            assert!(c.unit_may_replay(ua) && !c.unit_may_replay(ub));
        }

        /// A partly loaded district (§3.5): the loaded line is live, and the line away replays
        /// when it may and is gated when it may not. An edit that leaves no unit allowed ends the
        /// replay (staleness freezes, I5); one that leaves the line away allowed starts it again
        /// over the same units, and it goes on.
        #[test]
        fn a_partial_load_replays_the_units_away_that_may() {
            for b_may in [true, false] {
                let (b, p0, p1) = two_lines(ORE, CRUSHED, 10);
                let mut e = engine_with(&b, Tunables::default(), &lines(1000, (ORE, 1000)));
                let vb = if b_may { verdict::PASS } else { verdict::FAIL };
                judge(&mut e, &[(p0, verdict::PASS), (p1, vb)]);
                feed(
                    &mut e,
                    &at(
                        200,
                        Obs::ChunkLoaded {
                            district: D,
                            chunk: chunk(0),
                            applied_epoch: 0,
                        },
                    ),
                );
                feed(&mut e, &load_state(200, LoadState::Partial));
                e.advance_to(Tick(1200));
                assert_eq!(model(&e), replay_model::GLASS, "B may: {b_may}");
                let rp = replay(&e);
                assert!(!rp.gated()[p0 as usize], "A is live");
                assert_eq!(rp.gated()[p1 as usize], !b_may);
                let st = e.sim_stock(D).expect("replaying");
                assert_eq!(st.get(SHEET), 0, "A's stores are not the replay's: {st:?}");
                let crushed = if b_may { 100 } else { 0 };
                assert!((crushed - 2..=crushed).contains(&st.get(CRUSHED)), "{st:?}");
                // An edit at A's press stales A.
                feed(
                    &mut e,
                    &at(
                        1200,
                        Obs::Edit {
                            district: D,
                            kind: EditKind::Placed,
                            player: (1, 2),
                            pos: BlockPos::new(1, 64, 0),
                        },
                    ),
                );
                e.advance_to(Tick(2200));
                if b_may {
                    assert_eq!(model(&e), replay_model::GLASS);
                    let st = e.sim_stock(D).expect("replaying");
                    assert!((195..=200).contains(&st.get(CRUSHED)), "{st:?}");
                } else {
                    assert_eq!(model(&e), replay_model::NONE);
                    assert!(e.sim_stock(D).is_none());
                }
                e.check_books().expect("books");
            }
        }
    }
}
