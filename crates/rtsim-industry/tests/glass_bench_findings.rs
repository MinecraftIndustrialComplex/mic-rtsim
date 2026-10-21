// SPDX-License-Identifier: AGPL-3.0-or-later
//! Engine findings of the G2 replay bench (`rtsim-cli bench industry`, GLASSBOX.md §9.3), as
//! tests built only from public records through `Engine::ingest`. Findings the engine never
//! handled are ignored so the workspace stays green until it does; run them with
//! `cargo test -p rtsim-industry --test glass_bench_findings -- --ignored`. A regression is not
//! ignored.

use rtsim_industry::engine::Engine;
use rtsim_industry::glass::check::WindowResult;
use rtsim_industry::glass::records::*;
use rtsim_industry::glass::report::{replay_model, verdict};
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, SweepEnd, Timed};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

const BRASS: Good = Good(13);
const SHEET: Good = Good(14);
const COAL: Good = Good(15);
const D: DistrictId = DistrictId(1);
const BIG: u32 = 1 << 24;
const UNLOAD: u64 = 200;

/// One district's compile and inventories (item slots, contents), as Java would send them.
#[derive(Default)]
struct Line {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    recipes: Vec<GraphRecipe>,
    invs: Vec<(u32, Vec<(Good, i64)>)>,
}

fn head(index: usize) -> GraphHead {
    GraphHead {
        district: D,
        compile: 1,
        index: index as u32,
    }
}

impl Line {
    fn node(&mut self, kind: NodeKind, model: u32, flags: u32, rates: Vec<RateEntry>) -> u32 {
        let i = self.nodes.len();
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
        i as u32
    }

    fn store(&mut self, slots: u32, contents: &[(Good, i64)]) -> u32 {
        let inv = self.invs.len() as u32;
        self.invs.push((slots, contents.to_vec()));
        let v = self.node(NodeKind::Store, model::STORE_ITEMS, 0, Vec::new());
        self.nodes[v as usize].inventory = Some(inv);
        v
    }

    /// A press making `batch` of `outs` from `ins` every `ticks`.
    fn machine(&mut self, ins: Good, outs: Good, batch: u32, ticks: u32) -> u32 {
        let r = self.recipes.len();
        let term = |good| Term {
            good,
            flags: 0,
            num: 1,
            den: 1,
        };
        self.recipes.push(GraphRecipe {
            head: head(r),
            recipe_type: recipe_type::OTHER,
            heat: heat::NONE,
            inputs: vec![term(ins)],
            outputs: vec![term(outs)],
            duration: 0,
            name: 0,
        });
        let rate = RateEntry {
            level: 0,
            recipe: Some(r as u32),
            batch,
            ticks_num: ticks,
            ticks_den: 1,
            flags: 0,
        };
        self.node(
            NodeKind::Machine,
            model::PRESS,
            node_flags::ANY_OUTPUT_BLOCKS,
            vec![rate],
        )
    }

    fn edge(&mut self, kind: EdgeKind, from: u32, to: u32) -> usize {
        let i = self.edges.len();
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
            filter: Vec::new(),
        });
        i
    }

    /// A press loop: `src` to a press of (batch, ticks) to a store of sheets (from `s0`) a
    /// consumer of (batch, ticks) burns into coal, a threshold switch on the sheets stopping the
    /// press at `band.0` and restarting it at `band.1`.
    fn switched_loop(
        &mut self,
        src: u32,
        press: (u32, u32),
        burn: (u32, u32),
        band: (i64, i64),
        s0: i64,
    ) {
        let sheets = self.store(BIG, &[(SHEET, s0)]);
        let pm = self.machine(BRASS, SHEET, press.0, press.1);
        self.edge(EdgeKind::Adjacency, src, pm);
        self.edge(EdgeKind::Adjacency, pm, sheets);
        let coal = self.store(BIG, &[]);
        let cm = self.machine(SHEET, COAL, burn.0, burn.1);
        self.edge(EdgeKind::Adjacency, sheets, cm);
        self.edge(EdgeKind::Adjacency, cm, coal);
        let ctl = self.node(NodeKind::Controller, model::THRESHOLD_SWITCH, 0, Vec::new());
        self.nodes[ctl as usize].params = [band.0, band.1, 0, 80];
        self.edge(EdgeKind::Observe, sheets, ctl);
        let c = self.edge(EdgeKind::Control, ctl, pm);
        self.edges[c].param = CONTROL_STOP_WHEN_ON;
    }

    fn records(&self) -> Vec<GlassObs> {
        let (n, e, r) = (
            self.nodes.len() as u32,
            self.edges.len() as u32,
            self.recipes.len() as u32,
        );
        let mut out: Vec<GlassObs> = Vec::new();
        out.extend(self.nodes.iter().cloned().map(GlassObs::Node));
        out.extend(self.edges.iter().cloned().map(GlassObs::Edge));
        out.extend(self.recipes.iter().cloned().map(GlassObs::Recipe));
        out.push(GlassObs::End(GraphEnd {
            district: D,
            compile: 1,
            flags: end_flags::COMPLETE,
            window: 0,
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
        }));
        out
    }
}

fn send(e: &mut Engine, t: u64, obs: Obs) {
    if Tick(t) > e.now() {
        e.advance_to(Tick(t));
    }
    e.ingest(&Timed { at: Tick(t), obs });
    e.drain_commands();
}

fn load_state(state: LoadState) -> Obs {
    Obs::LoadState {
        district: D,
        state,
        ticking_chunks: u32::from(state == LoadState::Live),
        total_chunks: 1,
    }
}

/// A fresh engine with `line` compiled live at 0, swept at 100, marked passed (G-B4's gate, as
/// the engine's own replay tests do) and unloaded at [`UNLOAD`], where its glass replay starts.
fn replaying(line: &Line) -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    for (g, name) in [(BRASS, "brass"), (SHEET, "sheet"), (COAL, "coal")] {
        e.define_good(g, GoodInfo::item(name, 64));
    }
    e.session_start(Tick(0));
    let n = line.invs.len() as u32;
    send(
        &mut e,
        0,
        Obs::District(DistrictDecl {
            district: D,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: n,
        }),
    );
    for (index, &(slots, _)) in line.invs.iter().enumerate() {
        send(
            &mut e,
            0,
            Obs::Inventory(InventoryDecl {
                district: D,
                revision: 1,
                index: index as u32,
                pos: BlockPos::new(index as i32 * 2, 64, 0),
                kind: InvKind(InvKind::ITEMS),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    send(&mut e, 0, load_state(LoadState::Live));
    for r in line.records() {
        send(&mut e, 0, Obs::Glass(r));
    }
    for (index, (_, c)) in line.invs.iter().enumerate() {
        send(
            &mut e,
            100,
            Obs::StockSample {
                district: D,
                inventory: index as u32,
                sweep: 1,
                goods: GoodVec::from_pairs(c.iter().copied()),
            },
        );
    }
    send(
        &mut e,
        100,
        Obs::SweepEnd(SweepEnd {
            district: D,
            sweep: 1,
            records: n,
            inventories: n,
            live: true,
            creative: false,
            overstressed: false,
        }),
    );
    let c = e
        .glass
        .districts
        .get_mut(&D)
        .and_then(|g| g.compiled.as_mut())
        .expect("the compile is accepted");
    c.passed = Some(WindowResult {
        start: Tick(0),
        ticks: 1,
        verdict: verdict::PASS,
        goods: Vec::new(),
        stores: Vec::new(),
    });
    c.edited = false;
    send(&mut e, UNLOAD, load_state(LoadState::Unloaded));
    e.advance_to(Tick(UNLOAD));
    e
}

fn model(e: &Engine) -> u32 {
    e.glass.report(D).map_or(0, |r| r.replay.model)
}

/// A press fed by a belt from a store runs steadily on the glass box: one regime, no breakpoint
/// after the start's latency hold. Passes on feab04d. Found on 20b740a ("latency holds nodes
/// off"): the belt and the press hold each other off in turn (regime timers 10, 4, 10, 14, 10,
/// 14, ... ticks apart), so the district passes `breakpointsPerEventMax` within about 600 ticks,
/// falls back as oscillating and makes nothing. A store feeding the press directly, with or
/// without a belt edge's latency, is fine; the Mover node between them is what it takes. The
/// bench's factory cell (glass_elapsed, glass_quiet) fell back the same way. Fixed by d83ffd0
/// (holds key on what a regime could run); kept as a guard.
#[test]
fn a_belt_fed_press_stays_on_the_glass_box() {
    let mut l = Line::default();
    let brass = l.store(BIG, &[(BRASS, 1 << 29)]);
    let sheets = l.store(BIG, &[]);
    let rate = RateEntry {
        level: 0,
        recipe: None,
        batch: 1,
        ticks_num: 4,
        ticks_den: 1,
        flags: rate_flags::SHARED,
    };
    let belt = l.node(NodeKind::Mover, model::BELT, 0, vec![rate]);
    let press = l.machine(BRASS, SHEET, 1, 10);
    l.edge(EdgeKind::Adjacency, brass, belt);
    l.edge(EdgeKind::Adjacency, belt, press);
    l.edge(EdgeKind::Adjacency, press, sheets);
    let mut e = replaying(&l);
    for day in 1..=2 {
        e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
        let _ = e.sim_stock(D);
    }
    let r = e.glass.report(D).expect("a glass report");
    assert_eq!(r.replay.oscillations, 0, "fell back as oscillating");
    assert_eq!(r.replay.model, replay_model::GLASS);
    assert!(
        r.replay.breakpoints <= 4,
        "{} breakpoints",
        r.replay.breakpoints
    );
    // 1 sheet per 10 ticks for two days, less the start's hold.
    let made = e.sim_stock(D).unwrap_or_default().get(SHEET);
    let want = 2 * TICKS_PER_DAY as i64 / 10;
    assert!((want - made).abs() <= 10, "made {made}, want {want}");
}

/// Brass imported into a dry press line that replays on the glass box must reach the press.
/// Found: `Engine::inject` (like a train's, a carriage's or a package's import) adds to the
/// district's `PoolKey::Stock` pool, which no glass process reads, so the press never restarts
/// and the brass sits inert in `sim_stock`.
#[test]
#[ignore = "engine finding (G2 bench): an import into a glass replay lands in a pool no process reads"]
fn an_import_restarts_a_dry_glass_replay() {
    let mut l = Line::default();
    let brass = l.store(BIG, &[]);
    let sheets = l.store(BIG, &[]);
    let press = l.machine(BRASS, SHEET, 1, 10);
    l.edge(EdgeKind::Adjacency, brass, press);
    l.edge(EdgeKind::Adjacency, press, sheets);
    let mut e = replaying(&l);
    assert_eq!(model(&e), replay_model::GLASS);
    e.advance_to(Tick(UNLOAD + TICKS_PER_DAY));
    assert_eq!(e.sim_stock(D).unwrap_or_default().get(SHEET), 0, "dry");
    e.inject(D, BRASS, 640).expect("the district replays");
    // 640 sheets at 1 per 10 ticks take 6,400 ticks; give it a day.
    e.advance_to(Tick(UNLOAD + 2 * TICKS_PER_DAY));
    let st = e.sim_stock(D).unwrap_or_default();
    assert_eq!(model(&e), replay_model::GLASS);
    assert_eq!(
        (st.get(BRASS), st.get(SHEET)),
        (0, 640),
        "the imported brass is pressed"
    );
}

/// Two independent switched loops (sharing only a source that never runs dry) are each strictly
/// periodic, so G-B8/G-B9 promise closed-form cycles whose cost does not grow with elapsed time.
/// Found: the cycle search looks for a period of the whole district's state, which two loops of
/// unequal periods never repeat (here 3,200 and 6,400 ticks by rates; the replay runs them at
/// about 3,157 and 6,359), so the district passes `breakpointsPerEventMax` (64) breakpoints and
/// falls back as oscillating. Loops of equal period find their cycle. On 20b740a a loop's
/// period also depends on its press's cycle (the latency hold), so even equal rates in other
/// batches (a press of 1 per 10 ticks beside one of 2 per 20) fall back the same way.
#[test]
#[ignore = "engine finding (G2 bench): two switched loops of unequal periods fall back as oscillating"]
fn two_switched_loops_of_unequal_periods_stay_on_the_glass_box() {
    let mut l = Line::default();
    let brass = l.store(BIG, &[(BRASS, 1_000_000_000)]);
    // Press 1 per 10 ticks, consumer 1 per 20: a band of 80 sheets cycles in 3,200 ticks, 160 in
    // 6,400.
    l.switched_loop(brass, (1, 10), (1, 20), (100, 20), 80);
    l.switched_loop(brass, (1, 10), (1, 20), (180, 20), 50);
    let mut e = replaying(&l);
    for day in 1..=30 {
        e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
        let _ = e.sim_stock(D);
    }
    let r = e.glass.report(D).expect("a glass report");
    assert_eq!(r.replay.oscillations, 0, "fell back as oscillating");
    assert_eq!(r.replay.model, replay_model::GLASS);
    // Each consumer burns 1 per 20 ticks all the way.
    let coal = e.sim_stock(D).unwrap_or_default().get(COAL);
    let want = 2 * (30 * TICKS_PER_DAY as i64) / 20;
    assert!(
        (coal - want).abs() <= want / 100,
        "coal {coal}, want {want}"
    );
}

/// The control: two identical loops in other phases stay on the glass box, with one
/// closed-form cycle.
#[test]
fn two_switched_loops_of_equal_periods_stay_on_the_glass_box() {
    let mut l = Line::default();
    let brass = l.store(BIG, &[(BRASS, 1_000_000_000)]);
    l.switched_loop(brass, (1, 10), (1, 20), (100, 20), 80);
    l.switched_loop(brass, (1, 10), (1, 20), (100, 20), 30);
    let mut e = replaying(&l);
    for day in 1..=30 {
        e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
        let _ = e.sim_stock(D);
    }
    let r = e.glass.report(D).expect("a glass report");
    assert_eq!(r.replay.oscillations, 0);
    assert_eq!(r.replay.model, replay_model::GLASS);
}
