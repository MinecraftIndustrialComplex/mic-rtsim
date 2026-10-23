//! Findings of the second review of SEC-1 S2b's judging units (SECTIONS.md §3.4): the veto
//! across a warm-up, the derate from a unit's window judged against another model, and a unit of
//! unbounded goods alone leaning on the district's pass. Built on the public API, as an engine
//! drives it.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use rtsim_industry::district::Inventory;
use rtsim_industry::glass::Glass;
use rtsim_industry::glass::check::{Compiled, GlassEvent, Liveness, View};
use rtsim_industry::glass::graph::Graph;
use rtsim_industry::glass::records::*;
use rtsim_industry::glass::report::verdict;
use rtsim_industry::ids::{BlockPos, ChunkPos, DistrictId, Good, GoodInfo, GoodVec, VecMap};
use rtsim_industry::obs::{InvKind, InventoryDecl};
use rtsim_industry::tunables::GlassTunables;
use rtsim_kernel::clock::Tick;

const D: DistrictId = DistrictId(7);
const COPPER: Good = Good(2);
const BRASS: Good = Good(3);
const SHEET: Good = Good(4);
const ORE: Good = Good(8);
const CRUSHED: Good = Good(9);

fn goods() -> BTreeMap<Good, GoodInfo> {
    let mut m = BTreeMap::new();
    for (g, n) in [
        (COPPER, "copper"),
        (BRASS, "brass"),
        (SHEET, "sheet"),
        (ORE, "ore"),
        (CRUSHED, "crushed"),
    ] {
        m.insert(g, GoodInfo::item(n, 64));
    }
    m
}

#[derive(Default, Clone)]
struct B {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    recipes: Vec<GraphRecipe>,
    window: u32,
}

fn head(index: u32) -> GraphHead {
    GraphHead {
        district: D,
        compile: 1,
        index,
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
            vec![RateEntry {
                level: 0,
                recipe: Some(recipe),
                batch,
                ticks_num: ticks,
                ticks_den: 1,
                flags: 0,
            }],
        )
    }
    fn edge(&mut self, from: u32, to: u32) -> u32 {
        let i = self.edges.len() as u32;
        let order = self.edges.iter().filter(|e| e.from == from).count() as u32;
        self.edges.push(GraphEdge {
            head: head(i),
            kind: EdgeKind::Adjacency,
            from,
            to,
            order,
            flags: 0,
            capacity: None,
            latency: 0,
            param: 0,
            filter: Vec::new(),
        });
        i
    }
    fn end(&self, compile: u32) -> GraphEnd {
        let (n, e, r) = (
            self.nodes.len() as u32,
            self.edges.len() as u32,
            self.recipes.len() as u32,
        );
        GraphEnd {
            district: D,
            compile,
            flags: end_flags::COMPLETE,
            window: self.window,
            records: n + e + r,
            nodes: n,
            edges: e,
            recipes: r,
            domains: 0,
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
            Vec::new(),
        )
        .expect("valid")
    }
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
        out.push(GlassObs::End(self.end(compile)));
        out
    }
}

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

/// Two press lines: A brass(0) -> press -> sheet(1) at x 0..2 (chunk 0); B ore(2) -> press ->
/// crushed(3) at x 40..42 (chunk 2). Optionally a Port node feeding A's input store.
fn two_lines(port_on_a: bool) -> (B, u32, u32) {
    let mut b = B::default();
    let (i0, o0, i1, o1) = (b.store(0), b.store(1), b.store(2), b.store(3));
    let r0 = b.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let r1 = b.recipe(&[(ORE, 1)], &[(CRUSHED, 1)]);
    let p0 = b.machine(model::PRESS, r0, 1, 10);
    let p1 = b.machine(model::PRESS, r1, 1, 10);
    b.edge(i0, p0);
    b.edge(p0, o0);
    b.edge(i1, p1);
    b.edge(p1, o1);
    for (v, x) in [(i0, 0), (p0, 1), (o0, 2), (i1, 40), (p1, 41), (o1, 42)] {
        b.nodes[v as usize].pos = BlockPos::new(x, 64, 0);
    }
    if port_on_a {
        let port = b.node(NodeKind::Port, model::UNKNOWN, 0, Vec::new());
        b.nodes[port as usize].pos = BlockPos::new(0, 64, 2);
        b.edge(port, i0);
    }
    b.window = 2000;
    (b, p0, p1)
}

fn invs(a: i64, bm: i64) -> VecMap<u32, Inventory> {
    let mut v = VecMap::new();
    v.insert(0, inventory(0, &[(BRASS, 100_000 - a)]));
    v.insert(1, inventory(1, &[(SHEET, a)]));
    v.insert(2, inventory(2, &[(ORE, 100_000 - bm)]));
    v.insert(3, inventory(3, &[(CRUSHED, bm)]));
    v
}

fn ingest_all(g: &mut Glass, t: u64, recs: &[GlassObs], tun: &GlassTunables) {
    let gd = goods();
    for r in recs {
        g.ingest(Tick(t), r, None, &gd, tun);
    }
}

fn sweep(
    g: &mut Glass,
    t: u64,
    inv: &VecMap<u32, Inventory>,
    live: &Liveness<'_>,
    tun: &GlassTunables,
) {
    let gd = goods();
    let ports = GoodVec::new();
    let view = View {
        revision: 1,
        inventories: inv,
        ports: &ports,
    };
    g.sweep(Tick(t), D, &view, live, &gd, tun);
}

fn comp(g: &Glass) -> &Compiled {
    g.districts[&D].compiled.as_ref().expect("a compile")
}

fn ticking(x: i32, on: bool) -> GlassObs {
    GlassObs::ChunkTicking(ChunkTicking {
        district: D,
        chunk: ChunkPos { dim: 0, x, z: 0 },
        ticking: on,
    })
}

/// A unit holding a Port (a station feeding its store) has every stocked good UNBOUNDED, so its
/// windows judge nothing: it leans on the district's pass, as the same line as a one-unit
/// district does under G-B4 (choice SC66).
#[test]
fn a_port_unit_replays_on_the_districts_pass_as_the_line_alone_does() {
    let (b, p0, _) = two_lines(true);
    let tun = GlassTunables::default();
    let gd = goods();
    let ports = GoodVec::new();
    let mut c = Compiled::new(b.graph(), Tick(0), &tun, &BTreeSet::new());
    assert!(c.units.active());
    let ua = c.units.of_node[p0 as usize];
    assert!(c.units.unbounded[ua as usize], "the port is in A's unit");
    let mut ever = false;
    let mut before_pass = false;
    for k in 0..=200u64 {
        let t = k * 100;
        let inv = invs((t / 10) as i64, (t / 10) as i64);
        let view = View {
            revision: 1,
            inventories: &inv,
            ports: &ports,
        };
        if t == 0 {
            c.start(&view, &gd, &tun);
        }
        c.sweep(Tick(t), &view, &gd, &tun);
        ever |= c.unit_may_replay(ua);
        before_pass |= c.unit_may_replay(ua) && !c.eligible();
    }
    let last = c.unit_state[ua as usize].last.as_ref().expect("judged");
    eprintln!(
        "A's last: verdict {} goods {:?}",
        last.verdict,
        last.goods
            .iter()
            .map(|g| (g.good, g.class, g.verdict))
            .collect::<Vec<_>>()
    );
    assert_eq!(last.verdict, verdict::PASS);
    assert!(ever, "A may replay once the district passed");
    assert!(!before_pass, "never without the district's pass");

    // The same line A with its port, alone: one unit, the district's rule.
    let mut one = B::default();
    let (i, o) = (one.store(0), one.store(1));
    let r = one.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let p = one.machine(model::PRESS, r, 1, 10);
    one.edge(i, p);
    one.edge(p, o);
    let port = one.node(NodeKind::Port, model::UNKNOWN, 0, Vec::new());
    one.edge(port, i);
    one.window = 2000;
    let mut c1 = Compiled::new(one.graph(), Tick(0), &tun, &BTreeSet::new());
    assert!(!c1.units.active());
    for k in 0..=40u64 {
        let t = k * 100;
        let mut inv = VecMap::new();
        inv.insert(0, inventory(0, &[(BRASS, 100_000 - (t / 10) as i64)]));
        inv.insert(1, inventory(1, &[(SHEET, (t / 10) as i64)]));
        let view = View {
            revision: 1,
            inventories: &inv,
            ports: &ports,
        };
        if t == 0 {
            c1.start(&view, &gd, &tun);
        }
        c1.sweep(Tick(t), &view, &gd, &tun);
    }
    let l1 = c1.last.as_ref().expect("judged");
    eprintln!(
        "alone: verdict {} goods {:?}",
        l1.verdict,
        l1.goods
            .iter()
            .map(|g| (g.good, g.class, g.verdict))
            .collect::<Vec<_>>()
    );
    assert!(
        c1.unit_may_replay(0),
        "alone, the district rule lets it replay"
    );
}

/// A unit's window judged against another model names no node to derate (choice SC61). B's
/// chunk stops between live sweeps (SC67 discards B's window each time, so B's `last` stays the
/// FAIL of the first window at eta 0.5), while the district window (today's rule) concludes and
/// fails at eta about 0.9 against the derated model: the second derate is the district's 0.9, not
/// B's stale 0.5 again, and the model settles at B's 0.45.
#[test]
fn a_derate_does_not_reuse_a_units_window_from_before_the_last_one() {
    let (b, _p0, p1) = two_lines(false);
    let tun = GlassTunables::default();
    let mut g = Glass::default();
    ingest_all(&mut g, 0, &b.records(1), &tun);
    // B: half its modelled rate to 3200, then 0.45 of it (0.9 of the derated model).
    let bm = |t: u64| -> i64 {
        if t <= 3200 {
            (t / 20) as i64
        } else {
            160 + ((t - 3200) * 45 / 1000) as i64
        }
    };
    let mut t = 0;
    while t <= 5200 {
        if t > 3200 && (t / 100) % 8 == 0 {
            // B's chunk unloaded and loaded again since the last sweep.
            g.chunk_stopped(D, ChunkPos { dim: 0, x: 2, z: 0 });
        }
        sweep(
            &mut g,
            t,
            &invs((t / 10) as i64, bm(t)),
            &Liveness::All,
            &tun,
        );
        t += 100;
    }
    let c = comp(&g);
    let ub = c.units.of_node[p1 as usize];
    let st = &c.unit_state[ub as usize];
    eprintln!(
        "B's last start {:?} verdict {:?}",
        st.last.as_ref().map(|r| r.start),
        st.last.as_ref().map(|r| r.verdict)
    );
    let ders: Vec<(u32, f64, Good)> = c
        .events
        .iter()
        .filter_map(|e| match e {
            GlassEvent::Derated { node, eta, good } => Some((*node, eta.to_f64(), *good)),
            _ => None,
        })
        .collect();
    eprintln!("derates {ders:?}");
    let dl = c.last.as_ref().expect("district window");
    eprintln!(
        "district last start {:?}: {:?}",
        dl.start,
        dl.goods
            .iter()
            .map(|g| (g.good, g.verdict, g.measured / g.predicted))
            .collect::<Vec<_>>()
    );
    let d = c.regime.derate.get(&p1).map(|q| q.to_f64());
    eprintln!("B's derate now {d:?}");
    // Afterwards, no stops: B runs at 0.45 of its first model, the model says 0.25.
    let mut t = 5300;
    while t <= 20_000 {
        sweep(
            &mut g,
            t,
            &invs((t / 10) as i64, bm(t)),
            &Liveness::All,
            &tun,
        );
        t += 100;
    }
    let c2 = comp(&g);
    let dl = c2.last.as_ref().expect("district window");
    eprintln!(
        "later: district last start {:?} verdict {} {:?}; strikes {}; B's derate {:?}; B's last {:?}; B may replay {}",
        dl.start,
        dl.verdict,
        dl.goods
            .iter()
            .map(|g| (g.good, g.verdict, g.measured / g.predicted))
            .collect::<Vec<_>>(),
        c2.strikes,
        c2.regime.derate.get(&p1).map(|q| q.to_f64()),
        c2.unit_state[ub as usize]
            .last
            .as_ref()
            .map(|r| (r.start, r.verdict)),
        c2.unit_may_replay(ub),
    );
    assert_eq!(ders.len(), 2, "two district failures");
    assert!((ders[0].1 - 0.5).abs() < 0.02, "{ders:?}");
    assert!(
        (ders[1].1 - 0.9).abs() < 0.05,
        "the district's eta: {ders:?}"
    );
    let settled = c2
        .regime
        .derate
        .get(&p1)
        .map(|q| q.to_f64())
        .expect("derated");
    assert!((settled - 0.45).abs() < 0.03, "{settled}");
    assert_eq!(dl.verdict, verdict::PASS, "the model is B's again");
}

/// The goods a unit touches outlive the rows that held them (choice SC65). A discard (here a stop
/// between live sweeps, SC67) drops the open rows, and with them a good the unit's store holds but
/// its model does not carry (copper in A's output): A stays vetoed through its warm-up while the
/// district's last window failed on copper.
#[test]
fn the_veto_holds_while_a_unit_warms_up() {
    let (b, p0, _) = two_lines(false);
    let tun = GlassTunables::default();
    let mut g = Glass::default();
    ingest_all(&mut g, 0, &b.records(1), &tun);
    ingest_all(&mut g, 0, &[ticking(0, true)], &tun);
    let inv = |t: u64| {
        let m = (t / 10) as i64;
        let copper = if t >= 5300 {
            ((t - 5300) / 5) as i64
        } else {
            0
        };
        let mut out = vec![(SHEET, m)];
        if copper > 0 {
            out.push((COPPER, copper));
        }
        let mut v = VecMap::new();
        v.insert(0, inventory(0, &[(BRASS, 100_000 - m)]));
        v.insert(1, inventory(1, &out));
        v.insert(2, inventory(2, &[(ORE, 100_000 - m)]));
        v.insert(3, inventory(3, &[(CRUSHED, m)]));
        v
    };
    let loaded: BTreeSet<ChunkPos> = [ChunkPos { dim: 0, x: 0, z: 0 }].into();
    let partial = Liveness::Partial {
        loaded: &loaded,
        dim: 0,
    };
    let ua = comp(&g).units.of_node[p0 as usize];
    let mut vetoed_seen = false;
    let mut lapse = Vec::new();
    for k in 0..=90u64 {
        let t = k * 100;
        if t == 6600 {
            // A's chunk unloaded and loaded again since the sweep at 6500.
            g.chunk_stopped(D, ChunkPos { dim: 0, x: 0, z: 0 });
        }
        let live = if t == 3300 { &partial } else { &Liveness::All };
        sweep(&mut g, t, &inv(t), live, &tun);
        let c = comp(&g);
        let fails_cu = c.district_failing().contains(&COPPER);
        if fails_cu && !c.unit_may_replay(ua) {
            vetoed_seen = true;
        }
        if fails_cu && c.unit_may_replay(ua) {
            lapse.push(t);
        }
    }
    eprintln!(
        "vetoed seen {vetoed_seen}; may replay while the district fails on copper at {lapse:?}"
    );
    assert!(vetoed_seen);
    assert!(lapse.is_empty(), "the veto lapsed at {lapse:?}");
}
