// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass box's tests: GLASSBOX.md's worked examples, the solver's rules (docs/industry-boundary.md
//! §9.3), the checksum (§5.3) and the engine path.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use rtsim_kernel::clock::Tick;

use super::check::{Compiled, GlassEvent, POISSON_SLOPE_FACTOR, View};
use super::graph::Graph;
use super::q::Q;
use super::records::*;
use super::report::{class, verdict};
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
    let rep = c.report(0);
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
    use crate::obs::{DistrictDecl, Obs, SweepEnd, Timed};
    use crate::tunables::Tunables;

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
    }
}

// ---- U is never below L (G-B15) ----------------------------------------------------------------------

/// A random DAG of junctions, movers, buffers and machines between an input store and an output
/// store.
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
        let v = match k % 4 {
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
                b.edge(mid[a], mid[c]);
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
        kinds in prop::collection::vec((0u8..8, 0u8..16, 0u8..64), 1..10),
        edges in prop::collection::vec((0usize..10, 0usize..10, any::<bool>()), 0..16),
        held in prop::array::uniform3(any::<bool>()),
    ) {
        let (g, rg) = random_dag(&kinds, &edges, &held);
        let s = run(&g, &rg);
        for b in &s.goods {
            prop_assert!(b.in_u >= b.in_l && b.out_u >= b.out_l);
            // The guards aside, the relaxation and the LP must contain L's flow.
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
    }
}
