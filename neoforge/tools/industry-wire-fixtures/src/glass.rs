// SPDX-License-Identifier: AGPL-3.0-or-later
//! The glass-box fixtures (docs/industry-boundary.md §9, GLASSBOX.md §1.4, G-B19), printed by
//! `cargo run --release -- glass` into `neoforge/core/src/test/resources/glass-wire-fixtures.txt`
//! for `GlassWireContractTest`. Same rules as the industry fixtures: every field a different,
//! non-zero-byte value derived from the record's name, each record decoded again by the reference
//! decoder before it is printed.

use rtsim_industry::glass::codec as gc;
use rtsim_industry::glass::records::{
    ChunkTicking, DomainLevel, EdgeKind, GlassObs, GraphDomain, GraphEdge, GraphEnd, GraphHead,
    GraphNode, GraphRecipe, Member, NodeKind, ProbeSample, ProbeSet, RateEntry, Term,
};
use rtsim_industry::ids::{ChunkPos, Good};
use rtsim_industry::obs::{Obs, Timed};
use rtsim_industry::cmd::Cmd;

use super::{Fields, Gen, Sign, begin, cmd, district, game_time, goods, obs, pos, table};

const HEADER: &str = "\
# Generated from crates/rtsim-industry/src/glass/codec.rs by neoforge/tools/industry-wire-fixtures (the `glass` argument). Do not edit by hand.
# Regenerate: cd neoforge/tools/industry-wire-fixtures && nix develop ../../..# -c cargo run --release --quiet -- glass > ../../core/src/test/resources/glass-wire-fixtures.txt
# Lines: '<NAME> <hex>' is one encoded record (little-endian, header included); '<NAME>.fields k=v ...' the values it was built from;
# 'CODES.<group> Variant=code ...' an enum code table. f64 values are whole eighths, printed exactly. Latest-wins records carry
# game_time at 8; IND_CHUNK_TICKING obs_seq at 8 and game_time at 16; IND_PROBE_SET cmd_seq at 8.
";

/// A double that prints and parses exactly: a whole number of eighths, not zero.
fn f64v(g: &mut Gen) -> f64 {
    ((g.next() % 10_000_000) as f64 + 1.0) / 8.0
}

fn f64f(f: &mut Fields, name: &str, v: f64) -> f64 {
    f.num(name, v);
    v
}

fn head(f: &mut Fields, g: &mut Gen) -> GraphHead {
    let district = district(f, g);
    let compile = f.u32("compile", g.u32());
    let index = f.u32("index", g.u32());
    GraphHead {
        district,
        compile,
        index,
    }
}

fn glass_obs(out: &mut String, name: &str, kind: u16, seq: u64, at: rtsim_kernel::clock::Tick, o: GlassObs, f: &Fields) {
    obs(
        out,
        name,
        kind,
        seq,
        Timed {
            at,
            obs: Obs::Glass(o),
        },
        f,
    );
}

fn rates(f: &mut Fields, g: &mut Gen, n: usize) -> Vec<RateEntry> {
    f.sym("n_rates", n);
    (0..n)
        .map(|i| RateEntry {
            level: f.u32(&format!("rate{i}.level"), g.u32()),
            recipe: Some(f.u32(&format!("rate{i}.recipe"), g.u32() & 0x7FFF_FFFF)),
            batch: f.u32(&format!("rate{i}.batch"), g.u32()),
            ticks_num: f.u32(&format!("rate{i}.ticks_num"), g.u32()),
            ticks_den: f.u32(&format!("rate{i}.ticks_den"), g.u32()),
            flags: f.u32(&format!("rate{i}.flags"), g.u32()),
        })
        .collect()
}

fn terms(f: &mut Fields, g: &mut Gen, prefix: &str, n: usize) -> Vec<Term> {
    (0..n)
        .map(|i| Term {
            good: Good(f.u32(&format!("{prefix}{i}.good"), g.u32())),
            flags: f.u32(&format!("{prefix}{i}.flags"), g.u32()),
            num: f.i64(&format!("{prefix}{i}.num"), g.i64(Sign::Pos)),
            den: f.i64(&format!("{prefix}{i}.den"), g.i64(Sign::Pos)),
        })
        .collect()
}

pub fn fixtures() -> String {
    let mut out = String::from(HEADER);

    {
        let name = "IND_GRAPH_NODE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        f.sym("kind", "Machine");
        let model = f.u32("model", g.u32());
        let domain = Some(f.u32("domain", g.u32() & 0x7FFF_FFFF));
        let inventory = Some(f.u32("inventory", g.u32() & 0x7FFF_FFFF));
        let flags = f.u32("flags", g.u32());
        let p = pos(&mut f, &mut g, "");
        let rule = f.u32("rule", g.u32());
        let capacity = f.i64("capacity", g.i64(Sign::Pos));
        let params = [
            f.i64("param0", g.i64(Sign::Pos)),
            f.i64("param1", g.i64(Sign::Neg)),
            f.i64("param2", g.i64(Sign::Pos)),
            f.i64("param3", g.i64(Sign::Neg)),
        ];
        let rates = rates(&mut f, &mut g, 2);
        let o = GlassObs::Node(GraphNode {
            head,
            kind: NodeKind::Machine,
            model,
            domain,
            inventory,
            flags,
            pos: p,
            rule,
            capacity,
            params,
            rates,
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_NODE, 0, at, o, &f);
    }

    {
        // No domain, no inventory, no recipe: the NONE index on the wire.
        let name = "IND_GRAPH_NODE_NONE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        f.sym("kind", "Store");
        let model = f.u32("model", g.u32());
        f.sym("domain", "none");
        f.sym("inventory", "none");
        let flags = f.u32("flags", g.u32());
        let p = pos(&mut f, &mut g, "");
        let rule = f.u32("rule", g.u32());
        let capacity = f.i64("capacity", g.i64(Sign::Pos));
        let params = [
            f.i64("param0", g.i64(Sign::Pos)),
            f.i64("param1", g.i64(Sign::Pos)),
            f.i64("param2", g.i64(Sign::Pos)),
            f.i64("param3", g.i64(Sign::Pos)),
        ];
        f.sym("n_rates", 1);
        let level = f.u32("rate0.level", g.u32());
        f.sym("rate0.recipe", "none");
        let rates = vec![RateEntry {
            level,
            recipe: None,
            batch: f.u32("rate0.batch", g.u32()),
            ticks_num: f.u32("rate0.ticks_num", g.u32()),
            ticks_den: f.u32("rate0.ticks_den", g.u32()),
            flags: f.u32("rate0.flags", g.u32()),
        }];
        let o = GlassObs::Node(GraphNode {
            head,
            kind: NodeKind::Store,
            model,
            domain: None,
            inventory: None,
            flags,
            pos: p,
            rule,
            capacity,
            params,
            rates,
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_NODE, 0, at, o, &f);
    }

    {
        let name = "IND_GRAPH_EDGE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        f.sym("kind", "Belt");
        let from = f.u32("from", g.u32());
        let to = f.u32("to", g.u32());
        let order = f.u32("order", g.u32());
        let flags = f.u32("flags", g.u32());
        let cap_num = f.u32("cap_num", g.u32());
        let cap_den = f.u32("cap_den", g.u32());
        let latency = f.u32("latency", g.u32());
        let param = f.i64("param", g.i64(Sign::Neg));
        f.sym("n_filter", 3);
        let filter: Vec<Good> = (0..3)
            .map(|i| Good(f.u32(&format!("filter{i}"), g.u32())))
            .collect();
        let o = GlassObs::Edge(GraphEdge {
            head,
            kind: EdgeKind::Belt,
            from,
            to,
            order,
            flags,
            capacity: Some((cap_num, cap_den)),
            latency,
            param,
            filter,
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_EDGE, 0, at, o, &f);
    }

    {
        // Unlimited capacity is 0/0; no filter.
        let name = "IND_GRAPH_EDGE_UNLIMITED";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        f.sym("kind", "Adjacency");
        let from = f.u32("from", g.u32());
        let to = f.u32("to", g.u32());
        let order = f.u32("order", g.u32());
        let flags = f.u32("flags", g.u32());
        f.sym("capacity", "none");
        let latency = f.u32("latency", g.u32());
        let param = f.i64("param", g.i64(Sign::Pos));
        f.sym("n_filter", 0);
        let o = GlassObs::Edge(GraphEdge {
            head,
            kind: EdgeKind::Adjacency,
            from,
            to,
            order,
            flags,
            capacity: None,
            latency,
            param,
            filter: vec![],
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_EDGE, 0, at, o, &f);
    }

    {
        let name = "IND_GRAPH_RECIPE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        let recipe_type = f.u32("recipe_type", g.u32());
        let heat = f.u32("heat", g.u32());
        f.sym("n_inputs", 2);
        f.sym("n_outputs", 1);
        let duration = f.u32("duration", g.u32());
        let name_id = f.u32("name", g.u32());
        let inputs = terms(&mut f, &mut g, "input", 2);
        let outputs = terms(&mut f, &mut g, "output", 1);
        let o = GlassObs::Recipe(GraphRecipe {
            head,
            recipe_type,
            heat,
            inputs,
            outputs,
            duration,
            name: name_id,
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_RECIPE, 0, at, o, &f);
    }

    {
        let name = "IND_GRAPH_DOMAIN";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let head = head(&mut f, &mut g);
        let network = f.u64("network", g.u64());
        let current = f.u32("current", g.u32());
        let driver = Some(f.u32("driver", g.u32() & 0x7FFF_FFFF));
        let flags = f.u32("flags", g.u32());
        let live_stress = f64f(&mut f, "live_stress", f64v(&mut g));
        let live_capacity = f64f(&mut f, "live_capacity", f64v(&mut g));
        f.sym("n_levels", 2);
        let levels: Vec<DomainLevel> = (0..2)
            .map(|i| DomainLevel {
                selector: f.u32(&format!("level{i}.selector"), g.u32()),
                speed_num: f.i32(
                    &format!("level{i}.speed_num"),
                    g.i32(if i == 0 { Sign::Pos } else { Sign::Neg }),
                ),
                speed_den: f.u32(&format!("level{i}.speed_den"), g.u32()),
                capacity: f64f(&mut f, &format!("level{i}.capacity"), f64v(&mut g)),
            })
            .collect();
        f.sym("n_members", 2);
        let members: Vec<Member> = (0..2)
            .map(|i| Member {
                node: if i == 0 {
                    Some(f.u32("member0.node", g.u32() & 0x7FFF_FFFF))
                } else {
                    f.sym("member1.node", "none");
                    None
                },
                count: f.u32(&format!("member{i}.count"), g.u32()),
                ratio_num: f.i32(&format!("member{i}.ratio_num"), g.i32(Sign::Neg)),
                ratio_den: f.u32(&format!("member{i}.ratio_den"), g.u32()),
                impact: f64f(&mut f, &format!("member{i}.impact"), f64v(&mut g)),
            })
            .collect();
        let o = GlassObs::Domain(GraphDomain {
            head,
            network,
            current,
            driver,
            flags,
            live_stress,
            live_capacity,
            levels,
            members,
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_DOMAIN, 0, at, o, &f);
    }

    {
        let name = "IND_GRAPH_END";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let compile = f.u32("compile", g.u32());
        let flags = f.u32("flags", g.u32());
        let o = GlassObs::End(GraphEnd {
            district,
            compile,
            flags,
            records: f.u32("records", g.u32()),
            nodes: f.u32("nodes", g.u32()),
            edges: f.u32("edges", g.u32()),
            recipes: f.u32("recipes", g.u32()),
            domains: f.u32("domains", g.u32()),
            reason: f.u32("reason", g.u32()),
            fingerprint: f.u64("fingerprint", g.u64()),
            frame: f.u64("frame", g.u64()),
            revision: f.u32("revision", g.u32()),
            compile_ticks: f.u32("compile_ticks", g.u32()),
            window: f.u32("window", g.u32()),
        });
        glass_obs(&mut out, name, gc::IND_GRAPH_END, 0, at, o, &f);
    }

    {
        let name = "IND_PROBE_SAMPLE";
        let (mut g, mut f) = begin(name);
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let sweep = f.u32("sweep", g.u32());
        let node = f.u32("node", g.u32());
        let compile = f.u32("compile", g.u32());
        f.sym("n_entries", 2);
        let goods = goods(&mut f, &mut g, "entry", 2);
        let o = GlassObs::ProbeSample(ProbeSample {
            district,
            sweep,
            node,
            compile,
            goods,
        });
        glass_obs(&mut out, name, gc::IND_PROBE_SAMPLE, 0, at, o, &f);
    }

    {
        let name = "IND_CHUNK_TICKING";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("obs_seq", g.u64());
        let at = game_time(&mut f, &mut g);
        let district = district(&mut f, &mut g);
        let dim = f.u32("dim", g.u32());
        let x = f.i32("chunk_x", g.i32(Sign::Neg));
        let z = f.i32("chunk_z", g.i32(Sign::Pos));
        f.sym("ticking", true);
        let o = GlassObs::ChunkTicking(ChunkTicking {
            district,
            chunk: ChunkPos { dim, x, z },
            ticking: true,
        });
        glass_obs(&mut out, name, gc::IND_CHUNK_TICKING, seq, at, o, &f);
    }

    {
        let name = "IND_PROBE_SET";
        let (mut g, mut f) = begin(name);
        let seq = f.u64("cmd_seq", g.u64());
        let district = district(&mut f, &mut g);
        let compile = f.u32("compile", g.u32());
        f.sym("n_nodes", 3);
        let nodes: Vec<u32> = (0..3)
            .map(|i| f.u32(&format!("node{i}"), g.u32()))
            .collect();
        let c = Cmd::ProbeSet(ProbeSet {
            district,
            compile,
            nodes,
        });
        cmd(&mut out, name, gc::IND_PROBE_SET, seq, c, &f);
    }

    code_tables(&mut out);
    out
}

/// The aux codes the codec writes for node and edge kinds.
fn code_tables(out: &mut String) {
    let node_kinds = [
        NodeKind::Store,
        NodeKind::Buffer,
        NodeKind::Machine,
        NodeKind::Mover,
        NodeKind::Junction,
        NodeKind::Heater,
        NodeKind::Source,
        NodeKind::Sink,
        NodeKind::BlackBox,
        NodeKind::Controller,
        NodeKind::Port,
        NodeKind::Boiler,
    ];
    let rows: Vec<_> = node_kinds
        .into_iter()
        .map(|kind| {
            let mut b = Vec::new();
            gc::encode_obs(
                0,
                rtsim_kernel::clock::Tick(1),
                &GlassObs::Node(GraphNode {
                    head: GraphHead::default(),
                    kind,
                    model: 0,
                    domain: None,
                    inventory: None,
                    flags: 0,
                    pos: Default::default(),
                    rule: 0,
                    capacity: 0,
                    params: [0; 4],
                    rates: vec![],
                }),
                &mut b,
            )
            .expect("encode");
            (format!("{kind:?}"), u32::from_le_bytes(b[4..8].try_into().unwrap()))
        })
        .collect();
    table(out, "node_kind", &rows);
    let edge_kinds = [
        EdgeKind::Adjacency,
        EdgeKind::Belt,
        EdgeKind::Chute,
        EdgeKind::Pipe,
        EdgeKind::Chain,
        EdgeKind::AirZone,
        EdgeKind::ArmReach,
        EdgeKind::Heat,
        EdgeKind::Control,
        EdgeKind::Observe,
        EdgeKind::Line,
        EdgeKind::Drop,
    ];
    let rows: Vec<_> = edge_kinds
        .into_iter()
        .map(|kind| {
            let mut b = Vec::new();
            gc::encode_obs(
                0,
                rtsim_kernel::clock::Tick(1),
                &GlassObs::Edge(GraphEdge {
                    head: GraphHead::default(),
                    kind,
                    from: 0,
                    to: 0,
                    order: 0,
                    flags: 0,
                    capacity: None,
                    latency: 0,
                    param: 0,
                    filter: vec![],
                }),
                &mut b,
            )
            .expect("encode");
            (format!("{kind:?}"), u32::from_le_bytes(b[4..8].try_into().unwrap()))
        })
        .collect();
    table(out, "edge_kind", &rows);
}
