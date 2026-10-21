// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every module's layers (VIEWER.md §7): their descriptors, and the marks a save draws, built
//! once at load into one scene per frame. V1 writes scenes directly from the decoded modules
//! instead of through the part format (§4.3), which the live tap and the host source need and a
//! save does not.
//!
//! Each builder runs under `catch_unwind`: a paged module's reads panic when a page cannot be read
//! (a writer's compaction removed it under the viewer), and such a layer is reported, not fatal.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use mic_viewer::api::{Group, LayerDesc, Layers, Mark};
use mic_viewer::scene::SceneWriter;
use rtsim_conflict::engine::Target as PartyTarget;
use rtsim_ecology::dragon::{Purpose, State, Target};
use rtsim_kernel::clock::Tick;
use rtsim_settle::{Culture, NodeKind, Pos};
use rtsim_ships::engine::Role;
use rtsim_travel::edge::Medium;
use rtsim_world::Via;

use crate::load::{Saved, Status};

/// Layer groups, in panel order.
pub const GROUPS: [(&str, &str); 7] = [
    ("settlements", "Settlements"),
    ("trade", "Markets"),
    ("travel", "Roads and travel"),
    ("ecology", "Dragons and herds"),
    ("ships", "Ships"),
    ("conflict", "Conflict"),
    ("industry", "Industry"),
];

type Build = fn(&Ctx, &mut Out) -> Option<String>;

/// One layer: its descriptor, the module it reads, and its builder.
pub struct Spec {
    pub desc: LayerDesc,
    pub module: &'static str,
    build: Build,
}

/// Every layer.
pub fn specs() -> Vec<Spec> {
    let s = |desc: LayerDesc, module: &'static str, build: Build| Spec {
        desc,
        module,
        build,
    };
    vec![
        s(
            LayerDesc::new(
                "settle.villages",
                "settlements",
                "Villages",
                "point",
                "#e0b04a",
            )
            .describe("Villages, player bases and havens, sized by population")
            .priority(10)
            .legend(&[
                ("Human village", "#e0b04a"),
                ("Ribbit village", "#9b6bd6"),
                ("Player base", "#4ab0e0"),
                ("Haven", "#d0453a"),
            ]),
            "phase1",
            villages,
        ),
        s(
            LayerDesc::new("trade.prices", "trade", "Price index", "point", "#e0b04a")
                .describe(
                    "Each storehouse market's mean price over its goods' anchors (1 is at anchor)",
                )
                .priority(9)
                .off()
                .legend(&[
                    ("Cheap (under 0.9)", "#3a9e5f"),
                    ("Near anchor", "#e0b04a"),
                    ("Dear (over 1.1)", "#d0453a"),
                ]),
            "phase1",
            prices,
        ),
        s(
            LayerDesc::new(
                "travel.roads",
                "travel",
                "Roads and lanes",
                "edge",
                "#8a7f6a",
            )
            .describe("Edges between markets, wider with more goods in transit")
            .priority(2)
            .legend(&[
                ("Road", "#8a7f6a"),
                ("Sea lane", "#4a90c0"),
                ("Sky lane", "#a0a0d8"),
            ]),
            "phase1",
            roads,
        ),
        s(
            LayerDesc::new("travel.caravans", "travel", "Caravans", "leg", "#c98a3a")
                .describe("Caravans on their legs between markets")
                .priority(6)
                .zoom(0.0, 256.0)
                .glyph("circle", 4.0)
                .legend(&[("Outbound", "#c98a3a"), ("Returning", "#7a5a2a")]),
            "phase1",
            caravans,
        ),
        s(
            LayerDesc::new(
                "travel.messengers",
                "travel",
                "Messengers",
                "leg",
                "#6aa0d0",
            )
            .describe("News packets on the road: waiting at their origin until they leave")
            .priority(3)
            .zoom(0.0, 64.0)
            .glyph("circle", 3.0),
            "phase1",
            messengers,
        ),
        s(
            LayerDesc::new(
                "travel.bands",
                "travel",
                "Touring bands",
                "point",
                "#9b6bd6",
            )
            .describe("Ribbit bands: staying at a stop, or walking to the next")
            .kinds(&["point", "leg"])
            .priority(5)
            .glyph("star", 5.0)
            .legend(&[("Staying", "#9b6bd6"), ("Walking", "#c7a8ef")]),
            "phase1",
            bands,
        ),
        s(
            LayerDesc::new(
                "ecology.lairs",
                "ecology",
                "Dragon lairs",
                "point",
                "#b04a4a",
            )
            .describe("Roosts and caves")
            .priority(8)
            .glyph("triangle", 6.0)
            .legend(&[
                ("Roost", "#b04a4a"),
                ("Cave", "#7a3a3a"),
                ("Vacant", "#999999"),
            ]),
            "dragons",
            lairs,
        ),
        s(
            LayerDesc::new("ecology.dragons", "ecology", "Dragons", "point", "#d0453a")
                .describe("Dragons where they are, and their flight legs")
                .kinds(&["point", "leg"])
                .priority(9)
                .glyph("diamond", 6.0)
                .legend(&[
                    ("Roosting", "#d0453a"),
                    ("Flying", "#ff7b54"),
                    ("Hunting", "#ffb347"),
                    ("Near a player", "#e5484d"),
                    ("Leaving", "#a05050"),
                ]),
            "dragons",
            dragons,
        ),
        s(
            LayerDesc::new("ecology.hunts", "ecology", "Hunts", "edge", "#ffb347")
                .describe("A line from each hunting dragon to its target")
                .priority(4)
                .width(1.5)
                .legend(&[
                    ("Road", "#ffb347"),
                    ("Village", "#e0b04a"),
                    ("Home", "#e5484d"),
                    ("Herd", "#7fb069"),
                    ("District", "#b48ead"),
                ]),
            "dragons",
            hunts,
        ),
        s(
            LayerDesc::new("ecology.herds", "ecology", "Herds", "area", "#7fb069")
                .describe("Wild herds, a disc of their footprint")
                .priority(7),
            "herds",
            herds,
        ),
        s(
            LayerDesc::new(
                "ecology.herd_routes",
                "ecology",
                "Herd routes",
                "path",
                "#a8d08d",
            )
            .describe("Where each herd is walking")
            .priority(1)
            .zoom(0.0, 256.0)
            .width(1.0),
            "herds",
            herd_routes,
        ),
        s(
            LayerDesc::new("ships.docks", "ships", "Docks", "point", "#4a90c0")
                .describe("Sea and sky docks")
                .priority(7)
                .glyph("square", 5.0)
                .legend(&[("Sea dock", "#4a90c0"), ("Sky dock", "#a0a0d8")]),
            "ships",
            docks,
        ),
        s(
            LayerDesc::new("ships.lanes", "ships", "Lanes", "path", "#4a90c0")
                .describe("Ship lanes, tinted by the pirates' share")
                .priority(2)
                .width(2.0)
                .legend(&[
                    ("Safe", "#4a90c0"),
                    ("Threatened", "#e0b04a"),
                    ("Dangerous", "#d0453a"),
                ]),
            "ships",
            lanes,
        ),
        s(
            LayerDesc::new("ships.ships", "ships", "Ships", "point", "#2a6fa0")
                .describe("Active ships where they are")
                .priority(8)
                .glyph("triangle", 5.0)
                .legend(&[
                    ("Merchant", "#2a6fa0"),
                    ("Pirate", "#d0453a"),
                    ("Navy", "#e5e5e5"),
                ]),
            "ships",
            ships,
        ),
        s(
            LayerDesc::new(
                "conflict.factions",
                "conflict",
                "Factions",
                "area",
                "#c0504d",
            )
            .describe("Each faction's camp and reach")
            .priority(6),
            "conflict",
            factions,
        ),
        s(
            LayerDesc::new("conflict.feuds", "conflict", "Feuds", "edge", "#e5484d")
                .describe("From a faction to what it feuds with, coloured by step")
                .priority(5)
                .width(2.0)
                .legend(&[
                    ("Notice", "#a0a0a0"),
                    ("Envoy", "#e0b04a"),
                    ("Scouts", "#ffb347"),
                    ("Raid", "#ff7b54"),
                    ("Invasion", "#e5484d"),
                    ("Siege", "#8b1a1a"),
                ]),
            "conflict",
            feuds,
        ),
        s(
            LayerDesc::new(
                "conflict.parties",
                "conflict",
                "War parties",
                "leg",
                "#e5484d",
            )
            .describe("Parties out: their route, and where they are on it")
            .kinds(&["leg", "path"])
            .priority(9)
            .glyph("cross", 6.0)
            .legend(&[
                ("Envoy", "#e0b04a"),
                ("Scouts", "#ffb347"),
                ("Raid", "#ff7b54"),
                ("Invasion", "#e5484d"),
                ("Siege", "#8b1a1a"),
                ("Plunder", "#b04ab0"),
            ]),
            "conflict",
            parties,
        ),
        s(
            LayerDesc::new(
                "industry.districts",
                "industry",
                "Districts",
                "area",
                "#5ab4ac",
            )
            .describe("Industrial districts, a box of their chunks")
            .priority(8)
            .legend(&[
                ("Running", "#5ab4ac"),
                ("Learning", "#e0b04a"),
                ("Live", "#3a9e5f"),
                ("Suspended", "#a0a0a0"),
                ("Stopped", "#d0453a"),
                ("Out of season", "#7a7a9a"),
            ]),
            "industry",
            districts,
        ),
        s(
            LayerDesc::new(
                "industry.stations",
                "industry",
                "Stations",
                "point",
                "#5ab4ac",
            )
            .describe("Create train stations")
            .priority(7)
            .glyph("square", 5.0),
            "industry",
            stations,
        ),
        s(
            LayerDesc::new(
                "industry.lines",
                "industry",
                "Train lines",
                "edge",
                "#5ab4ac",
            )
            .describe("Station pairs trains ran between, wider with more trips")
            .priority(3),
            "industry",
            lines,
        ),
        s(
            LayerDesc::new("industry.trains", "industry", "Trains", "leg", "#2f8f87")
                .describe("Booked train legs")
                .priority(6)
                .glyph("square", 4.0),
            "industry",
            trains,
        ),
    ]
}

/// The layer list `/api/layers` answers. A layer empty in this save says why through its marks
/// (`/api/marks` `empty`, from the scene), which the page shows under the layer's name.
pub fn layers(specs: &[Spec]) -> Layers {
    Layers {
        groups: GROUPS
            .iter()
            .map(|(id, label)| Group {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect(),
        layers: specs.iter().map(|s| s.desc.clone()).collect(),
    }
}

/// What a builder reads.
pub struct Ctx<'a> {
    pub saved: &'a Saved,
    /// The save's world tick (the phase 1 clock the modules share).
    pub wnow: Tick,
}

impl Ctx<'_> {
    pub fn new(saved: &Saved) -> Ctx<'_> {
        let wnow = saved
            .phase1
            .as_ref()
            .map_or_else(|| Tick(saved.now().max(0) as u64), |p| p.world.now());
        Ctx { saved, wnow }
    }

    pub fn game(&self, t: Tick) -> i64 {
        self.saved.game(t)
    }
}

/// One layer's marks, by frame (Java's dimension id).
#[derive(Default)]
pub struct Out {
    marks: Vec<(u32, Mark)>,
}

impl Out {
    fn push(&mut self, frame: u32, m: Mark) {
        self.marks.push((frame, m));
    }
}

/// What the build drew: a scene writer per frame, the bounds of each frame's marks, marks per
/// layer, and why each empty layer is empty.
#[derive(Default)]
pub struct Drawn {
    pub frames: BTreeMap<u32, SceneWriter>,
    pub bounds: BTreeMap<u32, [f64; 4]>,
    pub counts: BTreeMap<String, usize>,
    pub reasons: BTreeMap<String, String>,
}

impl Drawn {
    fn take(&mut self, layer: &str, out: Out) {
        for (frame, m) in out.marks {
            let b = m.bbox();
            if b.iter().all(|v| v.is_finite()) {
                let e =
                    self.bounds
                        .entry(frame)
                        .or_insert([f64::MAX, f64::MAX, f64::MIN, f64::MIN]);
                *e = [
                    e[0].min(b[0]),
                    e[1].min(b[1]),
                    e[2].max(b[2]),
                    e[3].max(b[3]),
                ];
            }
            self.frames.entry(frame).or_default().mark(layer, m);
        }
    }
}

/// Runs every builder over the save. A layer whose builder panics draws nothing at all.
pub fn build(saved: &Saved, specs: &[Spec]) -> Drawn {
    let cx = Ctx::new(saved);
    let mut drawn = Drawn::default();
    for s in specs {
        let id = s.desc.id.clone();
        let why = match saved.module(s.module).map(|m| &m.status) {
            None | Some(Status::Absent) => Some(format!("no {} pages in this save", s.module)),
            Some(Status::Unreadable(e)) => {
                Some(format!("the {} pages did not read: {e}", s.module))
            }
            Some(Status::Loaded) => None,
        };
        if let Some(why) = why {
            drawn.counts.insert(id.clone(), 0);
            drawn.reasons.insert(id, why);
            continue;
        }
        let mut out = Out::default();
        let r = catch_unwind(AssertUnwindSafe(|| (s.build)(&cx, &mut out)));
        let n = out.marks.len();
        match r {
            Ok(why) => {
                drawn.counts.insert(id.clone(), n);
                if n == 0 {
                    let why = why.map_or_else(
                        || "nothing of this kind in the save".into(),
                        |w| saved.scrub(&w),
                    );
                    drawn.reasons.insert(id.clone(), why);
                }
                drawn.take(&id, out);
            }
            Err(p) => {
                let msg = saved.scrub(&crate::load::panic_text(p));
                drawn.counts.insert(id.clone(), 0);
                drawn
                    .reasons
                    .insert(id, format!("reading its pages failed ({msg})"));
            }
        }
    }
    drawn
}

fn xz(p: Pos) -> [f64; 2] {
    [f64::from(p.x), f64::from(p.z)]
}

fn node_ci(k: NodeKind) -> u16 {
    match k {
        NodeKind::Village(Culture::Human) => 0,
        NodeKind::Village(Culture::Ribbit) => 1,
        NodeKind::PlayerBase { .. } => 2,
        NodeKind::Haven(_) => 3,
    }
}

fn villages(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    for (i, n) in w.nodes.iter().enumerate() {
        let pop = w.settlements.get(n.settlement).map_or(0, |s| s.pop);
        let s = (f64::from(pop.max(1)).sqrt() / 4.0).clamp(0.6, 3.0);
        out.push(
            u32::from(n.pos.frame),
            Mark::point(i, f64::from(n.pos.x), f64::from(n.pos.z))
                .with_ci(node_ci(n.kind))
                .with_size(s),
        );
    }
    None
}

/// A market's mean price over its goods' anchors (rtsim-trade's, which the archive's trade shifts
/// follow too).
pub fn price_index(m: &rtsim_trade::Market) -> Option<f64> {
    m.price_index()
}

fn prices(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    for (i, n) in w.nodes.iter().enumerate() {
        let Some(ix) = price_index(&w.market(i)) else {
            continue;
        };
        let ci = if ix < 0.9 {
            0
        } else if ix > 1.1 {
            2
        } else {
            1
        };
        out.push(
            u32::from(n.pos.frame),
            Mark::point(i, f64::from(n.pos.x), f64::from(n.pos.z))
                .with_ci(ci)
                .with_label(format!("{ix:.2}")),
        );
    }
    Some("no storehouse market has priced goods".into())
}

fn medium_ci(m: Medium) -> u16 {
    match m {
        Medium::Road => 0,
        Medium::Sea => 1,
        Medium::Sky => 2,
    }
}

fn roads(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let b = w.cfg.trade.quantum.index(cx.wnow);
    for (i, e) in w.edges.iter().enumerate() {
        let (pa, pb) = (w.nodes[e.a].pos, w.nodes[e.b].pos);
        if pa.frame != pb.frame {
            continue;
        }
        let mut t = BTreeMap::new();
        for d in &e.dirs {
            d.in_transit(b, &mut t);
        }
        let items: i128 = t.values().sum::<i128>() / i128::from(rtsim_trade::MILLI);
        let width = 1.0 + (1.0 + items.max(0) as f64).log10();
        out.push(
            u32::from(pa.frame),
            Mark::Edge {
                id: Some(i.to_string()),
                a: xz(pa),
                b: xz(pb),
                w: Some(width),
                ci: Some(medium_ci(e.medium)),
                c: None,
            },
        );
    }
    None
}

fn caravans(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    for (h, c) in w.caravans.iter() {
        let (a, b) = (w.nodes[c.leg.from].pos, w.nodes[c.leg.to].pos);
        let ci = match c.stage {
            rtsim_travel::caravan::Stage::Outbound => 0,
            rtsim_travel::caravan::Stage::Returning => 1,
        };
        out.push(
            u32::from(a.frame),
            Mark::leg(
                h.to_bits(),
                xz(a),
                xz(b),
                cx.game(c.leg.start),
                cx.game(c.leg.end),
            )
            .with_ci(ci),
        );
    }
    None
}

fn messengers(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let q = w.cfg.trade.quantum;
    for (id, p) in &w.packets {
        let Via::Road(r) = p.via else { continue };
        let Some(e) = w.edges.get(r.edge as usize) else {
            continue;
        };
        let d = &e.dirs[usize::from(r.dir) & 1];
        let (a, b) = (w.nodes[d.from].pos, w.nodes[d.to].pos);
        let t0 = q.at(p.departs);
        let t1 = q.at(p.departs + e.news_walk().max(1));
        out.push(
            u32::from(a.frame),
            Mark::leg(id, xz(a), xz(b), cx.game(t0), cx.game(t1)),
        );
    }
    Some("no news packet is on a road (packets with bands travel as the band)".into())
}

fn bands(cx: &Ctx, out: &mut Out) -> Option<String> {
    let w = &cx.saved.phase1.as_ref()?.world;
    let ql = w.cfg.trade.quantum.len();
    for (i, band) in w.bands.iter().enumerate() {
        let v = w.band_view(i, cx.wnow);
        let frame = u32::from(v.frame);
        match v.walking {
            Some((from, to, milli)) => {
                let stop = band
                    .route
                    .iter()
                    .enumerate()
                    .position(|(k, s)| *s == from && band.route[(k + 1) % band.route.len()] == to)
                    .unwrap_or(0);
                let walk = band.walks.get(stop).copied().unwrap_or(1).max(1) * ql;
                let gnow = cx.game(cx.wnow);
                let t0 = gnow - (walk as f64 * f64::from(milli) / 1000.0) as i64;
                out.push(
                    frame,
                    Mark::leg(
                        i,
                        xz(w.nodes[from].pos),
                        xz(w.nodes[to].pos),
                        t0,
                        t0 + walk as i64,
                    )
                    .with_ci(1),
                );
            }
            None => out.push(
                frame,
                Mark::point(i, v.x as f64, v.z as f64)
                    .with_ci(0)
                    .with_size(f64::from(band.members.max(1)).sqrt().clamp(0.8, 2.0)),
            ),
        }
    }
    None
}

fn lairs(cx: &Ctx, out: &mut Out) -> Option<String> {
    let d = cx.saved.dragons.as_ref()?;
    for (i, l) in d.lairs.iter().enumerate() {
        let ci = if l.occupants.is_empty() {
            2
        } else {
            match l.kind {
                rtsim_ecology::dragon::LairKind::Roost => 0,
                rtsim_ecology::dragon::LairKind::Cave => 1,
            }
        };
        out.push(
            u32::from(l.pos.frame),
            Mark::point(i, f64::from(l.pos.x), f64::from(l.pos.z))
                .with_ci(ci)
                .with_label(l.name.clone()),
        );
    }
    None
}

fn dragon_ci(s: &State) -> u16 {
    match s {
        State::Roosting => 0,
        State::Flying(_) => 1,
        State::Hunting { .. } => 2,
        State::Embodied { .. } => 3,
        State::Leaving { .. } => 4,
    }
}

fn dragons(cx: &Ctx, out: &mut Out) -> Option<String> {
    let d = cx.saved.dragons.as_ref()?;
    for (h, x) in d.dragons.iter() {
        let ci = dragon_ci(&x.state);
        if let (State::Flying(_), Some(l)) = (x.state, x.leg) {
            let p = l.path;
            out.push(
                u32::from(p.frame),
                Mark::leg(
                    h.to_bits(),
                    [f64::from(p.x0), f64::from(p.z0)],
                    [f64::from(p.x1), f64::from(p.z1)],
                    cx.game(p.t0),
                    cx.game(p.t1),
                )
                .with_ci(ci)
                .with_label(x.name.clone()),
            );
            continue;
        }
        let (pos, _) = x.position(&d.topo, &d.lairs, cx.wnow);
        out.push(
            u32::from(pos.frame),
            Mark::point(h.to_bits(), f64::from(pos.x), f64::from(pos.z))
                .with_ci(ci)
                .with_label(x.name.clone()),
        );
    }
    None
}

/// Where a hunt's target is, and its legend index.
pub fn target_pos(saved: &Saved, t: &Target) -> Option<(u16, [f64; 2], u16)> {
    Some(match *t {
        Target::Road { edge, x, z, .. } => {
            let w = &saved.phase1.as_ref()?.world;
            let e = w.edges.get(edge as usize)?;
            (w.nodes[e.a].pos.frame, [f64::from(x), f64::from(z)], 0)
        }
        Target::Village { node } => {
            let w = &saved.phase1.as_ref()?.world;
            let p = w.nodes.get(node as usize)?.pos;
            (p.frame, xz(p), 1)
        }
        Target::Home { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 2),
        Target::Herd { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 3),
        Target::District { frame, x, z, .. } => (frame, [f64::from(x), f64::from(z)], 4),
    })
}

/// The target of a dragon that is hunting or flying to a hunt.
pub fn hunt_target(s: &State) -> Option<Target> {
    match *s {
        State::Hunting { target, .. } => Some(target),
        State::Flying(Purpose::Hunt(t)) => Some(t),
        State::Embodied {
            purpose: Purpose::Hunt(t),
            ..
        }
        | State::Leaving {
            purpose: Purpose::Hunt(t),
            ..
        } => Some(t),
        _ => None,
    }
}

fn hunts(cx: &Ctx, out: &mut Out) -> Option<String> {
    let d = cx.saved.dragons.as_ref()?;
    for (h, x) in d.dragons.iter() {
        let Some(t) = hunt_target(&x.state) else {
            continue;
        };
        let Some((frame, b, ci)) = target_pos(cx.saved, &t) else {
            continue;
        };
        let (pos, _) = x.position(&d.topo, &d.lairs, cx.wnow);
        if pos.frame != frame {
            continue;
        }
        out.push(
            u32::from(frame),
            Mark::Edge {
                id: Some(h.to_bits().to_string()),
                a: xz(pos),
                b,
                w: None,
                ci: Some(ci),
                c: None,
            },
        );
    }
    Some("no dragon is hunting at the save".into())
}

fn herds(cx: &Ctx, out: &mut Out) -> Option<String> {
    let hs = cx.saved.herds.as_ref()?;
    for (h, x) in hs.herds.iter() {
        let (px, pz) = hs.position(h, cx.wnow);
        let r = f64::from(hs.footprint(h, cx.wnow).max(4));
        let n = hs.count(h, cx.wnow);
        out.push(
            u32::from(x.frame),
            Mark::disc(Some(h.to_bits().to_string()), px as f64, pz as f64, r)
                .with_label(format!("{} ({n})", x.name)),
        );
    }
    None
}

fn herd_routes(cx: &Ctx, out: &mut Out) -> Option<String> {
    let hs = cx.saved.herds.as_ref()?;
    for (h, x) in hs.herds.iter() {
        if x.route.points.len() < 2 {
            continue;
        }
        out.push(
            u32::from(x.frame),
            Mark::Path {
                id: Some(h.to_bits().to_string()),
                p: x.route
                    .points
                    .iter()
                    .map(|&(a, b)| [f64::from(a), f64::from(b)])
                    .collect(),
                ci: None,
                c: None,
                w: None,
            },
        );
    }
    Some("every herd is staying put".into())
}

fn docks(cx: &Ctx, out: &mut Out) -> Option<String> {
    let s = cx.saved.ships.as_ref()?;
    for (i, d) in s.docks.iter().enumerate() {
        out.push(
            u32::from(d.frame),
            Mark::point(i, f64::from(d.x), f64::from(d.z))
                .with_ci(u16::from(d.medium == Medium::Sky)),
        );
    }
    None
}

fn lanes(cx: &Ctx, out: &mut Out) -> Option<String> {
    let s = cx.saved.ships.as_ref()?;
    for (i, l) in s.lanes.iter().enumerate() {
        let share = s.lane_share(i as u32);
        let ci = if share >= 200 {
            2
        } else if share > 0 {
            1
        } else {
            0
        };
        out.push(
            u32::from(l.frame),
            Mark::Path {
                id: Some(i.to_string()),
                p: l.path
                    .pts
                    .iter()
                    .map(|&(a, b)| [f64::from(a), f64::from(b)])
                    .collect(),
                ci: Some(ci),
                c: None,
                w: None,
            },
        );
    }
    None
}

fn ships(cx: &Ctx, out: &mut Out) -> Option<String> {
    let s = cx.saved.ships.as_ref()?;
    let w = &cx.saved.phase1.as_ref()?.world;
    for (h, x) in s.ships.iter() {
        if !x.is_active() {
            continue;
        }
        let (px, pz) = s.position(w, h, cx.wnow);
        let ci = match x.role {
            Role::Merchant => 0,
            Role::Pirate { .. } => 1,
            Role::Navy { .. } => 2,
        };
        out.push(
            u32::from(x.frame),
            Mark::point(h.to_bits(), f64::from(px), f64::from(pz)).with_ci(ci),
        );
    }
    Some("no ship is active".into())
}

/// A culture's colour on the map.
pub fn culture_color(i: u16) -> &'static str {
    const C: [&str; 8] = [
        "#c0504d", "#8064a2", "#4bacc6", "#f79646", "#9bbb59", "#7f7f7f", "#c00000", "#1f497d",
    ];
    C[usize::from(i) % C.len()]
}

fn factions(cx: &Ctx, out: &mut Out) -> Option<String> {
    let c = cx.saved.conflict.as_ref()?;
    for f in 0..c.factions.len() as u32 {
        let Some(x) = c.factions.get(f) else { continue };
        out.push(
            u32::from(x.pos.frame),
            Mark::disc(
                Some(f.to_string()),
                f64::from(x.pos.x),
                f64::from(x.pos.z),
                f64::from(x.radius.max(16)),
            )
            .with_color(culture_color(x.culture))
            .with_label(c.faction_name(f)),
        );
    }
    None
}

fn feuds(cx: &Ctx, out: &mut Out) -> Option<String> {
    let c = cx.saved.conflict.as_ref()?;
    for f in 0..c.factions.len() as u32 {
        let Some(x) = c.factions.get(f) else { continue };
        for (actor, feud) in c.feuds_of(f) {
            let to = feud.focus.map(|fo| fo.pos).or_else(|| {
                let p = c.parties.get(feud.party?)?;
                party_target_pos(cx.saved, &p.target)
            });
            let Some(to) = to else { continue };
            if to.frame != x.pos.frame {
                continue;
            }
            let ci = (feud.step as u16).saturating_sub(1).min(5);
            out.push(
                u32::from(x.pos.frame),
                Mark::Edge {
                    id: Some(format!("{f}:{}", actor.to_bits())),
                    a: xz(x.pos),
                    b: xz(to),
                    w: None,
                    ci: Some(ci),
                    c: None,
                },
            );
        }
    }
    Some("no feud has a focus or a party out yet".into())
}

/// Where a party is headed.
pub fn party_target_pos(saved: &Saved, t: &PartyTarget) -> Option<Pos> {
    match *t {
        PartyTarget::District(_, p) | PartyTarget::Pos(p) => Some(p),
        PartyTarget::Node(n) => Some(saved.phase1.as_ref()?.world.nodes.get(n)?.pos),
    }
}

fn parties(cx: &Ctx, out: &mut Out) -> Option<String> {
    let c = cx.saved.conflict.as_ref()?;
    for (h, p) in c.parties.iter() {
        let frame = u32::from(p.frame);
        let ci = p.kind.index() as u16;
        let pts: Vec<[f64; 2]> = p
            .route
            .iter()
            .map(|&(a, b)| [f64::from(a), f64::from(b)])
            .collect();
        if pts.len() >= 2 {
            out.push(
                frame,
                Mark::Path {
                    id: Some(h.to_bits().to_string()),
                    p: pts.clone(),
                    ci: Some(ci),
                    c: None,
                    w: Some(1.0),
                },
            );
        }
        let seg = (p.seg as usize).min(pts.len().saturating_sub(2));
        match (
            pts.get(seg),
            pts.get(seg + 1),
            p.times.get(seg),
            p.times.get(seg + 1),
        ) {
            (Some(a), Some(b), Some(t0), Some(t1)) => out.push(
                frame,
                Mark::leg(h.to_bits(), *a, *b, cx.game(*t0), cx.game(*t1)).with_ci(ci),
            ),
            _ => {
                let (x, z) = c.party_pos(h, cx.wnow);
                out.push(
                    frame,
                    Mark::point(h.to_bits(), f64::from(x), f64::from(z)).with_ci(ci),
                );
            }
        }
    }
    Some("no party is out".into())
}

/// A district state's legend index.
pub fn district_ci(state: &str) -> u16 {
    match state {
        "running" => 0,
        "learning" => 1,
        "live" => 2,
        "suspended" => 3,
        "out of season" => 5,
        _ => 4,
    }
}

fn districts(cx: &Ctx, out: &mut Out) -> Option<String> {
    let v = cx.saved.industry.as_ref()?;
    for d in &v.districts {
        if d.chunks == 0 {
            continue;
        }
        let mut m = Mark::rect(Some(d.id.to_string()), d.block_box()).with_ci(district_ci(d.state));
        if let Some(n) = cx.saved.names.get(crate::keys::DICT_NAME, d.name) {
            m = m.with_label(n);
        }
        out.push(d.dim, m);
    }
    Some("no district has chunks".into())
}

fn stations(cx: &Ctx, out: &mut Out) -> Option<String> {
    let v = cx.saved.industry.as_ref()?;
    for s in &v.stations {
        let mut m = Mark::point(s.station.0, f64::from(s.pos.x), f64::from(s.pos.z));
        if let Some(n) = cx.saved.names.get(crate::keys::DICT_NAME, s.name) {
            m = m.with_label(n);
        }
        out.push(s.dim, m);
    }
    None
}

fn station_pos(v: &crate::industry::IndustryView, id: u64) -> Option<(u32, [f64; 2])> {
    let s = v.stations.iter().find(|s| s.station.0 == id)?;
    Some((s.dim, [f64::from(s.pos.x), f64::from(s.pos.z)]))
}

fn lines(cx: &Ctx, out: &mut Out) -> Option<String> {
    let v = cx.saved.industry.as_ref()?;
    for ((a, b), l) in &v.lines {
        let (Some((fa, pa)), Some((fb, pb))) = (station_pos(v, *a), station_pos(v, *b)) else {
            continue;
        };
        if fa != fb {
            continue;
        }
        out.push(
            fa,
            Mark::Edge {
                id: Some(format!("{a}-{b}")),
                a: pa,
                b: pb,
                w: Some(1.0 + (1.0 + l.trips as f64).log10()),
                ci: None,
                c: None,
            },
        );
    }
    Some("no train ran between two known stations".into())
}

fn trains(cx: &Ctx, out: &mut Out) -> Option<String> {
    let v = cx.saved.industry.as_ref()?;
    for (id, l) in &v.legs {
        let Some((fa, pa)) = station_pos(v, l.from.0) else {
            continue;
        };
        let pb =
            l.to.and_then(|t| station_pos(v, t.0))
                .filter(|(f, _)| *f == fa)
                .map_or(pa, |(_, p)| p);
        out.push(fa, Mark::leg(id, pa, pb, l.depart.0 as i64, l.eta.0 as i64));
    }
    Some("no train leg is booked".into())
}
