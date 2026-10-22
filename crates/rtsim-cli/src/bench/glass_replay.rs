// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass replay's sweeps (GLASSBOX.md §4 and §9.3, phase G2), run by `rtsim-cli bench
//! industry`:
//!
//! | Sweep | Varies | Metric | Declared slope |
//! |---|---|---|---|
//! | `glass_items` | items per tick through the switched line (batch sizes and recipe ticks, 1x to 1000x) | CPU per sim day | 0 (flat) |
//! | `glass_elapsed` | days between reads of one replaying factory cell | CPU per catch-up read | 0 (flat) |
//! | `glass_periodic` | switch periods between reads of the switched line | CPU per catch-up read | 0 (flat) |
//! | `glass_regimes` | exports a day from a full store (one breakpoint each) | CPU per sim day | 1 |
//! | `glass_quiet` | replaying factory cells with nothing happening | CPU per sim day | 0 (flat) |
//! | `glass_quiet` | same | resident heap | 1 |
//! | `glass_oscillation` | adversarial controller graphs | districts marked oscillating | reported |
//!
//! Every district is built as Java would send it: the district, its inventories, a live load, the
//! compile's records and a live sweep through `Engine::ingest`, then an unload. Driving a
//! checksum window that passes for every graph here is not what these sweeps measure, so the
//! compile is marked passed through the engine's public state, as the engine's own replay tests
//! do (G-B4's gate, not its window). Each point records the replay model at its end (2 is the
//! glass box) and the glass replay's breakpoints and regimes over the measured span, so a point
//! that fell back to the black box shows.

use rtsim_industry::engine::Engine;
use rtsim_industry::glass::check::WindowResult;
use rtsim_industry::glass::records::*;
use rtsim_industry::glass::report::{replay_model, verdict};
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::{DistrictDecl, InvKind, InventoryDecl, LoadState, Obs, SweepEnd, Timed};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use serde::Serialize;

use crate::sys;

pub const ZINC: Good = Good(11);
pub const COPPER: Good = Good(12);
pub const BRASS: Good = Good(13);
pub const SHEET: Good = Good(14);
pub const COAL: Good = Good(15);

/// The tick every district unloads at (after its compile at 0 and its sweep at 100).
pub const UNLOAD: u64 = 200;
/// The switched line's period at scale 1 (press 1 per 10 ticks, consumer 1 per 20, switch on at
/// 100 sheets and off at 20): 1,600 ticks up at 1/20 a tick, 1,600 down.
pub const PERIOD: u64 = 3_200;
/// Item slots of a big store: 2^24 slots, about a billion items.
const BIG: u32 = 1 << 24;

/// One district's compile and inventories, as Java would send them.
#[derive(Clone, Debug)]
pub struct Line {
    pub d: DistrictId,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    recipes: Vec<GraphRecipe>,
    domains: Vec<GraphDomain>,
    /// Per inventory: item slots and contents at the sweep.
    pub invs: Vec<(u32, Vec<(Good, i64)>)>,
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

impl Line {
    pub fn new(d: DistrictId) -> Line {
        Line {
            d,
            nodes: Vec::new(),
            edges: Vec::new(),
            recipes: Vec::new(),
            domains: Vec::new(),
            invs: Vec::new(),
        }
    }

    fn head(&self, index: u32) -> GraphHead {
        GraphHead {
            district: self.d,
            compile: 1,
            index,
        }
    }

    pub fn node(&mut self, kind: NodeKind, model: u32, flags: u32, rates: Vec<RateEntry>) -> u32 {
        let i = self.nodes.len() as u32;
        let head = self.head(i);
        self.nodes.push(GraphNode {
            head,
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

    /// A store node on a new inventory of `slots` item slots holding `contents`.
    pub fn store(&mut self, slots: u32, contents: &[(Good, i64)]) -> u32 {
        let inv = self.invs.len() as u32;
        self.invs.push((slots, contents.to_vec()));
        let v = self.node(NodeKind::Store, model::STORE_ITEMS, 0, Vec::new());
        self.nodes[v as usize].inventory = Some(inv);
        v
    }

    pub fn recipe(&mut self, ins: &[(Good, i64)], outs: &[(Good, i64)]) -> u32 {
        let i = self.recipes.len() as u32;
        let term = |&(good, num): &(Good, i64)| Term {
            good,
            flags: 0,
            num,
            den: 1,
        };
        let head = self.head(i);
        self.recipes.push(GraphRecipe {
            head,
            recipe_type: recipe_type::OTHER,
            heat: heat::NONE,
            inputs: ins.iter().map(term).collect(),
            outputs: outs.iter().map(term).collect(),
            duration: 0,
            name: 0,
        });
        i
    }

    pub fn machine(&mut self, model: u32, recipe: u32, batch: u32, ticks: u32) -> u32 {
        self.node(
            NodeKind::Machine,
            model,
            node_flags::ANY_OUTPUT_BLOCKS,
            vec![entry(Some(recipe), batch, ticks)],
        )
    }

    pub fn edge_k(&mut self, kind: EdgeKind, from: u32, to: u32) -> u32 {
        let i = self.edges.len() as u32;
        let order = self.edges.iter().filter(|e| e.from == from).count() as u32;
        let head = self.head(i);
        self.edges.push(GraphEdge {
            head,
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

    pub fn edge(&mut self, from: u32, to: u32) -> u32 {
        self.edge_k(EdgeKind::Adjacency, from, to)
    }

    /// A threshold switch on `store`: on at `hi` items, off at `lo`, gating `gated` by `param`
    /// ([`CONTROL_STOP_WHEN_ON`] or [`CONTROL_RUN_WHEN_ON`]).
    pub fn switch(&mut self, store: u32, gated: u32, hi: i64, lo: i64, param: i64) -> u32 {
        let ctl = self.node(NodeKind::Controller, model::THRESHOLD_SWITCH, 0, Vec::new());
        self.nodes[ctl as usize].params = [hi, lo, 0, 80];
        self.edge_k(EdgeKind::Observe, store, ctl);
        let e = self.edge_k(EdgeKind::Control, ctl, gated);
        self.edges[e as usize].param = param;
        ctl
    }

    /// The compile's records, END last.
    pub fn records(&self) -> Vec<GlassObs> {
        let (n, e, r, d) = (
            self.nodes.len() as u32,
            self.edges.len() as u32,
            self.recipes.len() as u32,
            self.domains.len() as u32,
        );
        let mut out = Vec::with_capacity((n + e + r + d + 1) as usize);
        out.extend(self.nodes.iter().cloned().map(GlassObs::Node));
        out.extend(self.edges.iter().cloned().map(GlassObs::Edge));
        out.extend(self.recipes.iter().cloned().map(GlassObs::Recipe));
        out.extend(self.domains.iter().cloned().map(GlassObs::Domain));
        out.push(GlassObs::End(GraphEnd {
            district: self.d,
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
}

/// Brass to a press (`batch` sheets every `ticks`) to a store of sheets of `out_slots` slots.
/// Nodes: 0 the brass, 1 the sheets, 2 the press.
pub fn press_line(d: DistrictId, brass: i64, batch: u32, ticks: u32, out_slots: u32) -> Line {
    let mut l = Line::new(d);
    let input = l.store(BIG, &[(BRASS, brass)]);
    let out = l.store(out_slots, &[]);
    let r = l.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let p = l.machine(model::PRESS, r, batch, ticks);
    l.edge(input, p);
    l.edge(p, out);
    l
}

/// The switched line (the engine's `switched_line_with`): brass to a press of `press` (batch,
/// ticks) to a store of sheets a consumer of `consume` (batch, ticks) burns into coal; a
/// threshold switch on the sheets stops the press at `hi` and restarts it at `lo`. Nodes: 0 the
/// brass, 1 the sheets, 2 the press, 3 the coal, 4 the consumer, 5 the switch.
pub fn switched_line(
    d: DistrictId,
    press: (u32, u32),
    consume: (u32, u32),
    hi: i64,
    lo: i64,
    sheets0: i64,
) -> Line {
    let mut l = press_line(d, 1_000_000_000, press.0, press.1, BIG);
    l.invs[1].1 = vec![(SHEET, sheets0)];
    let coal = l.store(BIG, &[]);
    let burn = l.recipe(&[(SHEET, 1)], &[(COAL, 1)]);
    let m = l.machine(model::PRESS, burn, consume.0, consume.1);
    l.edge(1, m);
    l.edge(m, coal);
    l.switch(1, 2, hi, lo, CONTROL_STOP_WHEN_ON);
    l
}

/// One switched loop of [`loops`]: press (batch, ticks), consumer (batch, ticks), `hi`, `lo` and
/// the sheets at the compile.
pub type Loop = ((u32, u32), (u32, u32), i64, i64, i64);

/// Independent switched loops in one district, sharing one brass store: per loop a press of
/// (batch, ticks), a store of sheets, a consumer of (batch, ticks) into its own coal store, and
/// a switch on the sheets stopping the press at `hi` and restarting it at `lo` (from `sheets0`).
pub fn loops(d: DistrictId, specs: &[Loop]) -> Line {
    let mut l = Line::new(d);
    let brass = l.store(BIG, &[(BRASS, 1_000_000_000)]);
    let press = l.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let burn = l.recipe(&[(SHEET, 1)], &[(COAL, 1)]);
    for &(p, c, hi, lo, s0) in specs {
        let sheets = l.store(BIG, &[(SHEET, s0)]);
        let pm = l.machine(model::PRESS, press, p.0, p.1);
        l.edge(brass, pm);
        l.edge(pm, sheets);
        let coal = l.store(BIG, &[]);
        let cm = l.machine(model::PRESS, burn, c.0, c.1);
        l.edge(sheets, cm);
        l.edge(cm, coal);
        l.switch(sheets, pm, hi, lo, CONTROL_STOP_WHEN_ON);
    }
    l
}

/// The glass_nodes factory's cell (`bench/glass.rs`: a funnel, a brass mixer, a belt, two brass
/// tunnels over three presses, ten nodes) for district `d`, with stores big enough that nothing
/// fills or empties for years.
pub fn factory_cell(d: DistrictId) -> Line {
    let mut l = Line::new(d);
    for g in super::glass::factory(1, false) {
        match g {
            GlassObs::Node(mut n) => {
                n.head.district = d;
                l.nodes.push(n);
            }
            GlassObs::Edge(mut e) => {
                e.head.district = d;
                l.edges.push(e);
            }
            GlassObs::Recipe(mut r) => {
                r.head.district = d;
                l.recipes.push(r);
            }
            GlassObs::Domain(mut x) => {
                x.head.district = d;
                l.domains.push(x);
            }
            _ => {}
        }
    }
    // The factory's inventories 0 (zinc and copper) and 1 (sheets).
    l.invs = vec![
        (BIG, vec![(ZINC, 1 << 29), (COPPER, 1 << 29)]),
        (BIG, Vec::new()),
    ];
    l
}

/// An engine that knows the goods, at tick 0 of a session.
pub fn engine() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    for (g, name) in [
        (ZINC, "zinc"),
        (COPPER, "copper"),
        (BRASS, "brass"),
        (SHEET, "sheet"),
        (COAL, "coal"),
    ] {
        e.define_good(g, GoodInfo::item(name, 64));
    }
    e.session_start(Tick(0));
    e
}

fn send(e: &mut Engine, t: u64, obs: Obs) {
    let at = Tick(t);
    if at > e.now() {
        e.advance_to(at);
    }
    e.ingest(&Timed { at, obs });
    e.drain_commands();
}

/// Declares `line`'s district live in a fresh engine: the district, its inventories and the
/// compile at 0, a live sweep at 100; marks the compile passed and unloads at [`UNLOAD`], where
/// the glass replay starts.
pub fn add(e: &mut Engine, line: &Line, slot: u64) {
    assert_eq!(e.now(), Tick(0), "a fresh engine");
    declare(e, line, slot, 0);
    sweep(e, line, 100);
    pass(e, line.d);
    send(e, UNLOAD, load_state(line.d, LoadState::Unloaded));
    // The replay starts when the engine next brings coverage up to date (the step's advance).
    e.advance_to(Tick(UNLOAD));
}

/// The district, its inventories, a live load and the compile's records at `t`. `slot` spreads
/// the inventories over the world (a district per 64 blocks).
fn declare(e: &mut Engine, line: &Line, slot: u64, t: u64) {
    let d = line.d;
    send(
        e,
        t,
        Obs::District(DistrictDecl {
            district: d,
            revision: 1,
            dim: 0,
            name: 0,
            not_learnable: None,
            inventory_count: line.invs.len() as u32,
        }),
    );
    for (index, &(slots, _)) in line.invs.iter().enumerate() {
        send(
            e,
            t,
            Obs::Inventory(InventoryDecl {
                district: d,
                revision: 1,
                index: index as u32,
                pos: BlockPos::new(
                    (slot % 1_000) as i32 * 64 + index as i32 * 2,
                    64,
                    (slot / 1_000) as i32 * 64,
                ),
                kind: InvKind(InvKind::ITEMS),
                item_slots: slots,
                fluid_capacity: 0,
                energy_capacity: 0,
            }),
        );
    }
    send(e, t, load_state(d, LoadState::Live));
    for r in line.records() {
        send(e, t, Obs::Glass(r));
    }
}

/// One live sweep of every inventory at `t`.
fn sweep(e: &mut Engine, line: &Line, t: u64) {
    let d = line.d;
    for (index, (_, c)) in line.invs.iter().enumerate() {
        send(
            e,
            t,
            Obs::StockSample {
                district: d,
                inventory: index as u32,
                sweep: 1,
                goods: GoodVec::from_pairs(c.iter().copied()),
            },
        );
    }
    send(
        e,
        t,
        Obs::SweepEnd(SweepEnd {
            district: d,
            sweep: 1,
            records: line.invs.len() as u32,
            inventories: line.invs.len() as u32,
            live: true,
            creative: false,
            overstressed: false,
        }),
    );
}

fn load_state(d: DistrictId, state: LoadState) -> Obs {
    Obs::LoadState {
        district: d,
        state,
        ticking_chunks: u32::from(state == LoadState::Live),
        total_chunks: 1,
    }
}

/// Marks the compile as passed (a window with nothing to learn), through the engine's public
/// state: the replay's gate (G-B4) without a window that matches every regime.
fn pass(e: &mut Engine, d: DistrictId) {
    let c = e
        .glass
        .districts
        .get_mut(&d)
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
}

/// The glass replay's counters of a set of districts, summed, and how many replay on the glass
/// box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counters {
    pub districts: u64,
    /// Districts on the glass model (2).
    pub glass: u64,
    /// The lowest replay model among them (2: every one on the glass box).
    pub model: u32,
    pub breakpoints: u64,
    pub regimes: u64,
    pub oscillations: u64,
    /// Closed-form cycles started (the replays' own counter).
    pub cycles: u64,
}

impl Counters {
    pub fn of(e: &Engine, ds: impl IntoIterator<Item = DistrictId>) -> Counters {
        let mut c = Counters {
            model: u32::MAX,
            ..Counters::default()
        };
        for d in ds {
            c.districts += 1;
            let Some(r) = e.glass.report(d) else {
                c.model = 0;
                continue;
            };
            c.model = c.model.min(r.replay.model);
            c.glass += u64::from(r.replay.model == replay_model::GLASS);
            c.breakpoints += u64::from(r.replay.breakpoints);
            c.regimes += u64::from(r.replay.regimes);
            c.oscillations += u64::from(r.replay.oscillations);
            c.cycles += e.glass.districts[&d]
                .replay
                .as_ref()
                .map_or(0, |x| u64::from(x.counts().cycles));
        }
        if c.model == u32::MAX {
            c.model = 0;
        }
        c
    }

    /// The counts since `before` (the model and districts are this one's).
    pub fn since(self, before: Counters) -> Counters {
        Counters {
            breakpoints: self.breakpoints - before.breakpoints,
            regimes: self.regimes - before.regimes,
            oscillations: self.oscillations - before.oscillations,
            cycles: self.cycles.saturating_sub(before.cycles),
            ..self
        }
    }
}

/// One point of a glass replay sweep.
#[derive(Clone, Debug, Serialize)]
pub struct ReplayPoint {
    pub n: u64,
    /// The metric: CPU per sim day or per catch-up read, in ns.
    pub cpu_ns: f64,
    /// Heap the districts added (the bench's counting allocator), for `glass_quiet`.
    pub heap_bytes: Option<u64>,
    /// The engine's own heap accounting (`Engine::heap_bytes`: the clusters, their pools and
    /// processes), for `glass_quiet`.
    pub engine_heap_bytes: Option<u64>,
    /// One save at the end of the span ([`save_cost`]), for `glass_quiet` and `glass_regimes`.
    pub save_bytes: Option<u64>,
    pub save_cpu_ns: Option<f64>,
    pub setup_ms: f64,
    /// The glass replay's counters over the measured span; the model at its end.
    pub counters: Counters,
    /// Cluster settles over the measured span (`Engine::stats`).
    pub settles: u64,
    /// Why districts fell back, if any did.
    pub fallbacks: Vec<String>,
}

fn cpu() -> u64 {
    sys::thread_cpu_ns().unwrap_or(0)
}

/// One save of the engine: its pages' bytes and the CPU of [`Engine::snapshot`], the least of
/// three (the industry engine writes every page at every save, F36).
pub fn save_cost(e: &Engine) -> (u64, f64) {
    let mut best = f64::INFINITY;
    let mut bytes = 0;
    for _ in 0..3 {
        let c0 = cpu();
        let pages = std::hint::black_box(e.snapshot());
        let c1 = cpu();
        bytes = pages.iter().map(|(_, b)| b.len() as u64).sum();
        best = best.min((c1 - c0) as f64);
    }
    (bytes, best)
}

fn fallbacks(e: &Engine, ds: impl IntoIterator<Item = DistrictId>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in ds {
        if let Some(f) = e.glass.districts.get(&d).and_then(|g| g.fallback) {
            let w = format!("{f:?}");
            if !out.contains(&w) {
                out.push(w);
            }
        }
    }
    out
}

const D1: DistrictId = DistrictId(1);

/// A measured span of `ds` in `e`: the counters before, the settles before, the CPU clock.
struct Span {
    k0: Counters,
    s0: u64,
    c0: u64,
}

impl Span {
    fn start(e: &Engine, ds: &[DistrictId]) -> Span {
        Span {
            k0: Counters::of(e, ds.iter().copied()),
            s0: e.stats.cluster_settles,
            c0: cpu(),
        }
    }

    /// Ends the span: `per` divides the CPU (days or reads).
    fn end(self, e: &Engine, ds: &[DistrictId], n: u64, per: u64, setup_ms: f64) -> ReplayPoint {
        let c1 = cpu();
        ReplayPoint {
            n,
            cpu_ns: (c1 - self.c0) as f64 / per as f64,
            heap_bytes: None,
            engine_heap_bytes: None,
            save_bytes: None,
            save_cpu_ns: None,
            setup_ms,
            counters: Counters::of(e, ds.iter().copied()).since(self.k0),
            settles: e.stats.cluster_settles - self.s0,
            fallbacks: fallbacks(e, ds.iter().copied()),
        }
    }
}

fn timed_add(e: &mut Engine, line: &Line) -> f64 {
    let t0 = std::time::Instant::now();
    add(e, line, 0);
    t0.elapsed().as_secs_f64() * 1e3
}

/// The switched line at `scale` times its items per tick: the recipe ticks shorten up to
/// tenfold, then the batches grow; the switch's levels scale with them, so the period stays
/// [`PERIOD`] ticks and only the items per tick change.
pub fn scaled_switched_line(d: DistrictId, scale: u32) -> Line {
    let speed = scale.clamp(1, 10);
    let batch = (scale / speed).max(1);
    let k = i64::from(scale);
    switched_line(
        d,
        (batch, 10 / speed),
        (batch, 20 / speed),
        100 * k,
        20 * k,
        80 * k,
    )
}

/// `glass_items`: the switched line at `scale` times the items, read once a sim day for `days`.
pub fn items(scale: u32, days: u64) -> ReplayPoint {
    let mut e = engine();
    let setup_ms = timed_add(&mut e, &scaled_switched_line(D1, scale));
    let span = Span::start(&e, &[D1]);
    for day in 1..=days {
        e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
        std::hint::black_box(e.sim_stock(D1));
    }
    span.end(&e, &[D1], u64::from(scale), days, setup_ms)
}

/// One district replaying from its unload, read once `ticks` later (everything due on the way
/// fires in the same call): CPU per catch-up.
fn catch_up(line: &Line, ticks: u64, n: u64) -> ReplayPoint {
    let mut e = engine();
    let setup_ms = timed_add(&mut e, line);
    let span = Span::start(&e, &[line.d]);
    e.advance_to(Tick(UNLOAD + ticks));
    std::hint::black_box(e.sim_stock(line.d));
    span.end(&e, &[line.d], n, 1, setup_ms)
}

/// `glass_elapsed`: the factory cell read `days` days after its unload.
pub fn elapsed(days: u64) -> ReplayPoint {
    catch_up(&factory_cell(D1), days * TICKS_PER_DAY, days)
}

/// `glass_periodic`: the switched line read `periods` switch periods after its unload.
pub fn periodic(periods: u64) -> ReplayPoint {
    catch_up(
        &switched_line(D1, (1, 10), (1, 20), 100, 20, 80),
        periods * PERIOD,
        periods,
    )
}

/// The export line: a press of 4 sheets a tick fills a store of two stacks, which a train takes
/// a stack from (an outside event that frees the store); the press fills it again 16 ticks later
/// (a breakpoint).
pub fn export_line(d: DistrictId) -> Line {
    press_line(d, 1 << 30, 4, 1, 2)
}

/// Exports a stack from each of `ds` at `per_day` evenly spaced ticks of day `day` (from 1),
/// `stagger` ticks apart between districts, then runs to the day's end.
fn export_day(e: &mut Engine, ds: &[DistrictId], per_day: u64, day: u64, stagger: u64) {
    let step = TICKS_PER_DAY / per_day;
    for j in 0..per_day {
        for (i, &d) in ds.iter().enumerate() {
            let t = UNLOAD + (day - 1) * TICKS_PER_DAY + j * step + 1 + i as u64 * stagger;
            e.advance_to(Tick(t));
            std::hint::black_box(e.inject(d, SHEET, -64));
        }
    }
    e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
}

/// `glass_regimes`: the export line exported `per_day` times a day for `days` days after a day
/// unmeasured, so it breaks `per_day` times a day.
pub fn regimes(per_day: u64, days: u64) -> ReplayPoint {
    let mut e = engine();
    let setup_ms = timed_add(&mut e, &export_line(D1));
    // A day unmeasured: the store fills for the first time.
    export_day(&mut e, &[D1], per_day, 1, 0);
    let span = Span::start(&e, &[D1]);
    for day in 2..=days + 1 {
        export_day(&mut e, &[D1], per_day, day, 0);
    }
    let mut p = span.end(&e, &[D1], per_day, days, setup_ms);
    let (bytes, cpu_ns) = save_cost(&e);
    p.save_bytes = Some(bytes);
    p.save_cpu_ns = Some(cpu_ns);
    p
}

/// Busy districts beside the quiet ones (an export a day each).
pub const BUSY: u64 = 10;

/// `glass_quiet`: `n` factory cells replaying with nothing happening (their stores neither fill
/// nor empty for years, so no breakpoint and no event) beside [`BUSY`] export lines exported once
/// a day, for `days` days after a day unmeasured. The heap is what the districts added once their replays run, the
/// bench's counting allocator before and after.
pub fn quiet(n: u64, days: u64) -> ReplayPoint {
    let heap0 = sys::heap_live();
    let t0 = std::time::Instant::now();
    let mut e = engine();
    let busy: Vec<DistrictId> = (1..=BUSY).map(DistrictId).collect();
    let quiet: Vec<DistrictId> = (0..n).map(|i| DistrictId(1_000 + i)).collect();
    let lines: Vec<Line> = busy
        .iter()
        .map(|&d| export_line(d))
        .chain(quiet.iter().map(|&d| factory_cell(d)))
        .collect();
    add_all(&mut e, &lines);
    drop(lines);
    let setup_ms = t0.elapsed().as_secs_f64() * 1e3;
    let heap = sys::heap_live().saturating_sub(heap0) as u64;
    let engine_heap = e.heap_bytes() as u64;
    let all: Vec<DistrictId> = busy.iter().chain(quiet.iter()).copied().collect();
    // A day unmeasured: the busy stores fill for the first time.
    export_day(&mut e, &busy, 1, 1, 100);
    let span = Span::start(&e, &all);
    for day in 2..=days + 1 {
        export_day(&mut e, &busy, 1, day, 100);
    }
    let mut p = span.end(&e, &all, n, days, setup_ms);
    p.heap_bytes = Some(heap);
    p.engine_heap_bytes = Some(engine_heap);
    let (bytes, cpu_ns) = save_cost(&e);
    p.save_bytes = Some(bytes);
    p.save_cpu_ns = Some(cpu_ns);
    p
}

/// [`add`] for many districts as one hand-over: every district, inventory and compile at 0,
/// every sweep at 100, every unload at 200.
pub fn add_all(e: &mut Engine, lines: &[Line]) {
    for (slot, l) in lines.iter().enumerate() {
        declare(e, l, slot as u64, 0);
    }
    for l in lines {
        sweep(e, l, 100);
        pass(e, l.d);
    }
    for l in lines {
        send(e, UNLOAD, load_state(l.d, LoadState::Unloaded));
    }
    e.advance_to(Tick(UNLOAD));
}

/// One adversarial graph of `glass_oscillation`.
#[derive(Clone, Debug, Serialize)]
pub struct OscillationCase {
    pub name: &'static str,
    pub what: &'static str,
    pub days: u64,
    /// Over the replay, from its unload.
    pub counters: Counters,
    pub fallbacks: Vec<String>,
    /// CPU per sim day, read once a day.
    pub cpu_ns_per_day: f64,
}

/// The switched line's loop spec: press (batch, ticks), consumer (batch, ticks), on, off, start.
const LOOP_A: Loop = ((1, 10), (1, 20), 100, 20, 80);

/// The adversarial graphs: (name, what, line).
pub fn adversarial() -> Vec<(&'static str, &'static str, Line)> {
    let mut out = vec![
        (
            "narrow_band",
            "switch band of 1 under a tick's flow of 8: press 8 a tick, consumer 4 a tick, on 21, off 20",
            switched_line(D1, (8, 1), (4, 1), 21, 20, 20),
        ),
        (
            "no_band",
            "switch on and off at the same level (100): press 1/10, consumer 1/20",
            switched_line(D1, (1, 10), (1, 20), 100, 100, 80),
        ),
    ];
    // Two switches on one store, each gating its own press (bands that overlap, not nested).
    let mut l = switched_line(D1, (1, 10), (1, 13), 100, 20, 80);
    let r2 = l.recipe(&[(BRASS, 1)], &[(SHEET, 1)]);
    let p2 = l.machine(model::PRESS, r2, 1, 7);
    l.edge(0, p2);
    l.edge(p2, 1);
    l.switch(1, p2, 77, 33, CONTROL_STOP_WHEN_ON);
    out.push((
        "two_switches_one_store",
        "two switches on one store gating two presses (1/10 at 100..20, 1/7 at 77..33), consumer 1/13",
        l,
    ));
    // Two switches pulling one press opposite ways.
    let mut l = switched_line(D1, (1, 10), (1, 20), 60, 40, 55);
    l.switch(1, 2, 50, 45, CONTROL_RUN_WHEN_ON);
    out.push((
        "opposed_switches",
        "one press stopped above 60 (off at 40) and run only above 50 (off at 45), consumer 1/20",
        l,
    ));
    out.push((
        "two_loops_same",
        "two independent switched loops of one period (3,200 ticks) in other phases",
        loops(D1, &[LOOP_A, ((1, 10), (1, 20), 100, 20, 30)]),
    ));
    out.push((
        "two_loops_2to1",
        "two independent switched loops, periods 3,200 and 6,400 ticks (bands 80 and 160)",
        loops(D1, &[LOOP_A, ((1, 10), (1, 20), 180, 20, 80)]),
    ));
    out.push((
        "two_loops_odd",
        "two independent switched loops, periods 3,200 and about 2,786 ticks",
        loops(D1, &[LOOP_A, ((1, 7), (1, 13), 90, 30, 60)]),
    ));
    out.push((
        "five_loops",
        "five independent switched loops, incommensurate periods",
        loops(
            D1,
            &[
                LOOP_A,
                ((1, 7), (1, 13), 90, 30, 60),
                ((1, 11), (1, 17), 70, 10, 40),
                ((1, 5), (1, 9), 120, 50, 100),
                ((1, 3), (1, 8), 64, 16, 30),
            ],
        ),
    ));
    out
}

/// `glass_oscillation`: each adversarial graph replayed for `days`, read once a day.
pub fn oscillation(days: u64) -> Vec<OscillationCase> {
    adversarial()
        .into_iter()
        .map(|(name, what, line)| {
            let mut e = engine();
            add(&mut e, &line, 0);
            // From before the replay started: a fallback at its start shows.
            let c0 = cpu();
            for day in 1..=days {
                e.advance_to(Tick(UNLOAD + day * TICKS_PER_DAY));
                std::hint::black_box(e.sim_stock(D1));
            }
            let c1 = cpu();
            OscillationCase {
                name,
                what,
                days,
                counters: Counters::of(&e, [D1]),
                fallbacks: fallbacks(&e, [D1]),
                cpu_ns_per_day: (c1 - c0) as f64 / days as f64,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock(e: &mut Engine, d: DistrictId) -> GoodVec {
        e.sim_stock(d).expect("replaying")
    }

    /// Every line the slope sweeps use replays on the glass box from its unload, and makes what
    /// its graph says.
    #[test]
    fn every_sweep_line_replays_on_the_glass_box() {
        for line in [
            scaled_switched_line(D1, 1),
            scaled_switched_line(D1, 1000),
            factory_cell(D1),
            export_line(D1),
        ] {
            let mut e = engine();
            add(&mut e, &line, 0);
            e.advance_to(Tick(UNLOAD + 10 * PERIOD));
            let k = Counters::of(&e, [D1]);
            assert_eq!(k.model, replay_model::GLASS, "{line:?}");
            assert!(fallbacks(&e, [D1]).is_empty());
        }
        // The factory cell: ten days of sheets at the mixer's 2/17 a tick, nothing stopped.
        let mut e = engine();
        add(&mut e, &factory_cell(D1), 0);
        e.advance_to(Tick(UNLOAD + 10 * TICKS_PER_DAY));
        let made = stock(&mut e, D1).get(SHEET) as f64;
        let want = 10.0 * TICKS_PER_DAY as f64 * 2.0 / 17.0;
        assert!((made - want).abs() < want * 0.001, "{made} for {want}");
    }

    /// The scaled switched line keeps its period: the closed-form cycle is found at every scale
    /// in the same few breakpoints, and the coal made scales with the items.
    #[test]
    fn the_scaled_switched_line_cycles_alike_at_every_scale() {
        let mut first = None;
        for scale in [1, 10, 100, 1000] {
            let mut e = engine();
            add(&mut e, &scaled_switched_line(D1, scale), 0);
            e.advance_to(Tick(UNLOAD + 100 * PERIOD));
            let k = Counters::of(&e, [D1]);
            assert_eq!(k.model, replay_model::GLASS);
            assert!(k.cycles >= 1, "{scale}: {k:?}");
            let coal = stock(&mut e, D1).get(COAL) as f64 / f64::from(scale);
            let (bps, c) = *first.get_or_insert((k.breakpoints, coal));
            assert_eq!(k.breakpoints, bps, "{scale}");
            assert!((coal - c).abs() <= c * 0.01, "{scale}: {coal} for {c}");
        }
    }

    /// The export line breaks a fixed number of times per export (the store frees a stack and
    /// fills again; a latency hold may add its timer), so breakpoints go as exports.
    #[test]
    fn each_export_costs_the_same_breakpoints() {
        let (a, b) = (regimes(10, 2), regimes(20, 2));
        for p in [&a, &b] {
            assert_eq!(p.counters.model, replay_model::GLASS);
            assert_eq!(p.counters.oscillations, 0);
        }
        let per = a.counters.breakpoints / 20;
        assert!((1..=2).contains(&per), "{per} breakpoints an export");
        assert_eq!(a.counters.breakpoints, 20 * per);
        assert_eq!(b.counters.breakpoints, 40 * per);
        assert_eq!(b.counters.regimes, 2 * a.counters.regimes);
    }

    /// The quiet cells neither break nor change regime over the measured days: twice as many
    /// add nothing to the busy lines' breakpoints and regimes.
    #[test]
    fn quiet_cells_do_nothing() {
        let (a, b) = (quiet(20, 3), quiet(40, 3));
        assert_eq!(a.counters.glass, BUSY + 20);
        assert_eq!(b.counters.glass, BUSY + 40);
        assert_eq!(a.counters.breakpoints, b.counters.breakpoints);
        assert_eq!(a.counters.regimes, b.counters.regimes);
        // Each busy line breaks the same, every day.
        assert_eq!(a.counters.breakpoints % (BUSY * 3), 0);
        assert!(a.counters.breakpoints > 0);
    }

    fn show(name: &str, p: &ReplayPoint) {
        println!(
            "{name:<16} n {:>7} cpu {:>12.0} ns setup {:>8.1} ms {:?} settles {} {:?} heap {:?} engine heap {:?} save {:?} B {:?} ns",
            p.n,
            p.cpu_ns,
            p.setup_ms,
            p.counters,
            p.settles,
            p.fallbacks,
            p.heap_bytes,
            p.engine_heap_bytes,
            p.save_bytes,
            p.save_cpu_ns
        );
    }

    /// `cargo test --release -p rtsim-cli heap_by_stage -- --ignored --nocapture`: what a
    /// replaying factory cell keeps resident, by the step that added it (bytes per district over
    /// 1,000 districts, the bench's counting allocator).
    #[test]
    #[ignore]
    fn heap_by_stage() {
        let n = 1_000u64;
        let lines: Vec<Line> = (0..n)
            .map(|i| factory_cell(DistrictId(1_000 + i)))
            .collect();
        let mut e = engine();
        let h0 = sys::heap_live();
        let mut marks = Vec::new();
        for (slot, l) in lines.iter().enumerate() {
            let mut bare = l.clone();
            bare.nodes.clear();
            bare.edges.clear();
            bare.recipes.clear();
            bare.domains.clear();
            // The district, its inventories and a live load, without the compile.
            let d = l.d;
            send(
                &mut e,
                0,
                Obs::District(DistrictDecl {
                    district: d,
                    revision: 1,
                    dim: 0,
                    name: 0,
                    not_learnable: None,
                    inventory_count: l.invs.len() as u32,
                }),
            );
            for (index, &(slots, _)) in l.invs.iter().enumerate() {
                send(
                    &mut e,
                    0,
                    Obs::Inventory(InventoryDecl {
                        district: d,
                        revision: 1,
                        index: index as u32,
                        pos: BlockPos::new(slot as i32 * 64 + index as i32 * 2, 64, 0),
                        kind: InvKind(InvKind::ITEMS),
                        item_slots: slots,
                        fluid_capacity: 0,
                        energy_capacity: 0,
                    }),
                );
            }
            send(&mut e, 0, load_state(d, LoadState::Live));
        }
        marks.push(("district, inventories, live", sys::heap_live()));
        for l in &lines {
            for r in l.records() {
                send(&mut e, 0, Obs::Glass(r));
            }
        }
        marks.push(("compile accepted, first solve", sys::heap_live()));
        for l in &lines {
            sweep(&mut e, l, 100);
            pass(&mut e, l.d);
        }
        marks.push(("a live sweep", sys::heap_live()));
        for l in &lines {
            send(&mut e, UNLOAD, load_state(l.d, LoadState::Unloaded));
        }
        e.advance_to(Tick(UNLOAD));
        marks.push(("unloaded, replay started", sys::heap_live()));
        e.advance_to(Tick(UNLOAD + 10 * TICKS_PER_DAY));
        marks.push(("ten days on", sys::heap_live()));
        let mut prev = h0;
        for (what, h) in marks {
            println!(
                "{what:<32} {:>8} bytes per district (+{})",
                (h - h0) as u64 / n,
                (h - prev) as u64 / n
            );
            prev = h;
        }
        println!(
            "Engine::heap_bytes {} per district",
            e.heap_bytes() as u64 / n
        );
        let c = e.glass.districts[&DistrictId(1_000)]
            .compiled
            .as_ref()
            .expect("compiled");
        println!(
            "graph: {} nodes, {} edges; regime cache and solution not measured apart",
            c.graph.nodes.len(),
            c.graph.edges.len()
        );
    }

    /// `cargo test --release -p rtsim-cli glass_replay_table -- --ignored --nocapture`: the
    /// sweeps alone, once each.
    #[test]
    #[ignore]
    fn glass_replay_table() {
        for k in [1, 10, 100, 1000] {
            show("glass_items", &items(k, 10));
        }
        for d in [1, 10, 100, 1000] {
            show("glass_elapsed", &elapsed(d));
        }
        for p in [10, 100, 1000, 10_000, 100_000] {
            show("glass_periodic", &periodic(p));
        }
        for m in [1, 10, 100, 1000] {
            show("glass_regimes", &regimes(m, 3));
        }
        for n in [10, 100, 1000] {
            show("glass_quiet", &quiet(n, 10));
        }
        for c in oscillation(10) {
            println!("{c:?}");
        }
    }
}
