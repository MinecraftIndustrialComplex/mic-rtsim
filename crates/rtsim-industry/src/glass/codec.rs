// SPDX-License-Identifier: AGPL-3.0-or-later
//! The byte layouts of the glass-box records (docs/industry-boundary.md §9), on the reference
//! codec's writer and reader (`crate::wire`). Latest-wins records carry `i64 gameTime` at 8; the
//! journaled IND_CHUNK_TICKING carries `u64 obsSeq` at 8 and `i64 gameTime` at 16; the command
//! carries `u64 cmdSeq` at 8.
//!
//! Every graph record (0x0048 to 0x004B) starts `16 u64 district, 24 u32 compile, 28 u32 index`.

use rtsim_kernel::clock::Tick;

use super::records::{
    ChunkTicking, DomainLevel, EdgeKind, GlassObs, GraphDomain, GraphEdge, GraphEnd, GraphHead,
    GraphNode, GraphRecipe, Member, NONE, NodeKind, ProbeSample, ProbeSet, RateEntry, Term,
};
use crate::ids::{ChunkPos, Good};
use crate::wire::{R, W, WireError, check, n32, sized};

pub const IND_GRAPH_NODE: u16 = 0x0048;
pub const IND_GRAPH_EDGE: u16 = 0x0049;
pub const IND_GRAPH_RECIPE: u16 = 0x004A;
pub const IND_GRAPH_DOMAIN: u16 = 0x004B;
pub const IND_GRAPH_END: u16 = 0x004C;
pub const IND_PROBE_SAMPLE: u16 = 0x004D;
pub const IND_CHUNK_TICKING: u16 = 0x040B;
pub const IND_PROBE_SET: u16 = 0x8406;

pub const OBS_KINDS: [u16; 7] = [
    IND_GRAPH_NODE,
    IND_GRAPH_EDGE,
    IND_GRAPH_RECIPE,
    IND_GRAPH_DOMAIN,
    IND_GRAPH_END,
    IND_PROBE_SAMPLE,
    IND_CHUNK_TICKING,
];

/// Fixed sizes and entry sizes.
pub const NODE_FIXED: usize = 112;
pub const RATE24: usize = 24;
pub const EDGE_FIXED: usize = 72;
pub const RECIPE_FIXED: usize = 56;
pub const TERM24: usize = 24;
pub const DOMAIN_FIXED: usize = 72;
pub const LEVEL24: usize = 24;
pub const MEMBER24: usize = 24;
pub const END_SIZE: usize = 80;
pub const PROBE_FIXED: usize = 40;
pub const TICKING_SIZE: usize = 48;
pub const PROBE_SET_FIXED: usize = 32;

fn node_code(k: NodeKind) -> u32 {
    match k {
        NodeKind::Store => 1,
        NodeKind::Buffer => 2,
        NodeKind::Machine => 3,
        NodeKind::Mover => 4,
        NodeKind::Junction => 5,
        NodeKind::Heater => 6,
        NodeKind::Source => 7,
        NodeKind::Sink => 8,
        NodeKind::BlackBox => 9,
        NodeKind::Controller => 10,
        NodeKind::Port => 11,
        NodeKind::Boiler => 12,
    }
}

fn node_from(kind: u16, code: u32) -> Result<NodeKind, WireError> {
    Ok(match code {
        1 => NodeKind::Store,
        2 => NodeKind::Buffer,
        3 => NodeKind::Machine,
        4 => NodeKind::Mover,
        5 => NodeKind::Junction,
        6 => NodeKind::Heater,
        7 => NodeKind::Source,
        8 => NodeKind::Sink,
        9 => NodeKind::BlackBox,
        10 => NodeKind::Controller,
        11 => NodeKind::Port,
        12 => NodeKind::Boiler,
        v => {
            return Err(WireError::BadCode {
                kind,
                field: "node_kind",
                value: v.into(),
            });
        }
    })
}

fn edge_code(k: EdgeKind) -> u32 {
    match k {
        EdgeKind::Adjacency => 1,
        EdgeKind::Belt => 2,
        EdgeKind::Chute => 3,
        EdgeKind::Pipe => 4,
        EdgeKind::Chain => 5,
        EdgeKind::AirZone => 6,
        EdgeKind::ArmReach => 7,
        EdgeKind::Heat => 8,
        EdgeKind::Control => 9,
        EdgeKind::Observe => 10,
        EdgeKind::Line => 11,
        EdgeKind::Drop => 12,
    }
}

fn edge_from(kind: u16, code: u32) -> Result<EdgeKind, WireError> {
    Ok(match code {
        1 => EdgeKind::Adjacency,
        2 => EdgeKind::Belt,
        3 => EdgeKind::Chute,
        4 => EdgeKind::Pipe,
        5 => EdgeKind::Chain,
        6 => EdgeKind::AirZone,
        7 => EdgeKind::ArmReach,
        8 => EdgeKind::Heat,
        9 => EdgeKind::Control,
        10 => EdgeKind::Observe,
        11 => EdgeKind::Line,
        12 => EdgeKind::Drop,
        v => {
            return Err(WireError::BadCode {
                kind,
                field: "edge_kind",
                value: v.into(),
            });
        }
    })
}

fn opt(v: u32) -> Option<u32> {
    (v != NONE).then_some(v)
}

fn unopt(v: Option<u32>) -> u32 {
    v.unwrap_or(NONE)
}

fn head(w: &mut W<'_>, h: &GraphHead) {
    w.put_u64(h.district.0);
    w.put_u32(h.compile);
    w.put_u32(h.index);
}

fn read_head(r: &R<'_>) -> GraphHead {
    GraphHead {
        district: r.district(16),
        compile: r.get_u32(24),
        index: r.get_u32(28),
    }
}

/// Appends one glass observation. `obs_seq` is written only for the journaled kind.
pub fn encode_obs(
    obs_seq: u64,
    at: Tick,
    o: &GlassObs,
    out: &mut Vec<u8>,
) -> Result<(), WireError> {
    match o {
        GlassObs::Node(n) => {
            let mut w = W::obs(out, IND_GRAPH_NODE, node_code(n.kind), None, at);
            head(&mut w, &n.head);
            w.put_u32(n.model);
            w.put_u32(unopt(n.domain));
            w.put_u32(unopt(n.inventory));
            w.put_u32(n.flags);
            w.pos3(n.pos);
            w.zero(4);
            w.put_u32(n.rule);
            w.put_u32(n32(n.rates.len()));
            w.put_i64(n.capacity);
            for p in n.params {
                w.put_i64(p);
            }
            for e in &n.rates {
                w.put_u32(e.level);
                w.put_u32(unopt(e.recipe));
                w.put_u32(e.batch);
                w.put_u32(e.ticks_num);
                w.put_u32(e.ticks_den);
                w.put_u32(e.flags);
            }
            w.finish()
        }
        GlassObs::Edge(e) => {
            let mut w = W::obs(out, IND_GRAPH_EDGE, edge_code(e.kind), None, at);
            head(&mut w, &e.head);
            w.put_u32(e.from);
            w.put_u32(e.to);
            w.put_u32(e.order);
            w.put_u32(e.flags);
            let (num, den) = e.capacity.unwrap_or((0, 0));
            w.put_u32(num);
            w.put_u32(den);
            w.put_u32(e.latency);
            w.put_u32(n32(e.filter.len()));
            w.put_i64(e.param);
            for g in &e.filter {
                w.put_u32(g.0);
            }
            w.finish()
        }
        GlassObs::Recipe(r) => {
            let n = r.inputs.len() + r.outputs.len();
            let mut w = W::obs(out, IND_GRAPH_RECIPE, n32(n), None, at);
            head(&mut w, &r.head);
            w.put_u32(r.recipe_type);
            w.put_u32(r.heat);
            w.put_u32(n32(r.inputs.len()));
            w.put_u32(n32(r.outputs.len()));
            w.put_u32(r.duration);
            w.put_u32(r.name);
            for t in r.inputs.iter().chain(&r.outputs) {
                w.put_u32(t.good.0);
                w.put_u32(t.flags);
                w.put_i64(t.num);
                w.put_i64(t.den);
            }
            w.finish()
        }
        GlassObs::Domain(d) => {
            let mut w = W::obs(out, IND_GRAPH_DOMAIN, n32(d.members.len()), None, at);
            head(&mut w, &d.head);
            w.put_u64(d.network);
            w.put_u32(n32(d.levels.len()));
            w.put_u32(d.current);
            w.put_u32(unopt(d.driver));
            w.put_u32(d.flags);
            w.put_f64(d.live_stress);
            w.put_f64(d.live_capacity);
            for l in &d.levels {
                w.put_u32(l.selector);
                w.put_i32(l.speed_num);
                w.put_u32(l.speed_den);
                w.zero(4);
                w.put_f64(l.capacity);
            }
            for m in &d.members {
                w.put_u32(unopt(m.node));
                w.put_u32(m.count);
                w.put_i32(m.ratio_num);
                w.put_u32(m.ratio_den);
                w.put_f64(m.impact);
            }
            w.finish()
        }
        GlassObs::End(e) => {
            let mut w = W::obs(out, IND_GRAPH_END, e.flags, None, at);
            w.put_u64(e.district.0);
            w.put_u32(e.compile);
            w.put_u32(e.window);
            w.put_u32(e.records);
            w.put_u32(e.nodes);
            w.put_u32(e.edges);
            w.put_u32(e.recipes);
            w.put_u32(e.domains);
            w.put_u32(e.reason);
            w.put_u64(e.fingerprint);
            w.put_u64(e.frame);
            w.put_u32(e.revision);
            w.put_u32(e.compile_ticks);
            w.finish()
        }
        GlassObs::ProbeSample(p) => {
            let mut w = W::obs(out, IND_PROBE_SAMPLE, n32(p.goods.len()), None, at);
            w.put_u64(p.district.0);
            w.put_u32(p.sweep);
            w.put_u32(p.node);
            w.put_u32(p.compile);
            w.zero(4);
            w.goods(&p.goods);
            w.finish()
        }
        GlassObs::ChunkTicking(c) => {
            let mut w = W::obs(
                out,
                IND_CHUNK_TICKING,
                u32::from(c.ticking),
                Some(obs_seq),
                at,
            );
            w.put_u64(c.district.0);
            w.put_u32(c.chunk.dim);
            w.put_i32(c.chunk.x);
            w.put_i32(c.chunk.z);
            w.zero(4);
            w.finish()
        }
    }
}

/// Decodes one glass observation of `kind` (the caller split the header off; `rec` is trimmed to
/// its size). Returns `obsSeq` (0 for latest-wins kinds) and the observation.
pub fn decode_obs(kind: u16, aux: u32, rec: &[u8]) -> Result<(u64, Tick, GlassObs), WireError> {
    let r = R(rec);
    let size = rec.len();
    let n = aux as usize;
    let bad = WireError::BadSize { kind, size };
    if kind != IND_CHUNK_TICKING && size < 32 {
        return Err(bad);
    }
    let at = Tick(r.get_i64(8) as u64);
    let o = match kind {
        IND_GRAPH_NODE => {
            if size < NODE_FIXED {
                return Err(bad);
            }
            let m = r.get_u32(68) as usize;
            check(kind, size, pad8(sized(NODE_FIXED, m, RATE24)))?;
            GlassObs::Node(GraphNode {
                head: read_head(&r),
                kind: node_from(kind, aux)?,
                model: r.get_u32(32),
                domain: opt(r.get_u32(36)),
                inventory: opt(r.get_u32(40)),
                flags: r.get_u32(44),
                pos: r.pos3(48),
                rule: r.get_u32(64),
                capacity: r.get_i64(72),
                params: [r.get_i64(80), r.get_i64(88), r.get_i64(96), r.get_i64(104)],
                rates: (0..m)
                    .map(|i| {
                        let e = NODE_FIXED + RATE24 * i;
                        RateEntry {
                            level: r.get_u32(e),
                            recipe: opt(r.get_u32(e + 4)),
                            batch: r.get_u32(e + 8),
                            ticks_num: r.get_u32(e + 12),
                            ticks_den: r.get_u32(e + 16),
                            flags: r.get_u32(e + 20),
                        }
                    })
                    .collect(),
            })
        }
        IND_GRAPH_EDGE => {
            if size < EDGE_FIXED {
                return Err(bad);
            }
            let m = r.get_u32(60) as usize;
            check(kind, size, pad8(sized(EDGE_FIXED, m, 4)))?;
            let (num, den) = (r.get_u32(48), r.get_u32(52));
            GlassObs::Edge(GraphEdge {
                head: read_head(&r),
                kind: edge_from(kind, aux)?,
                from: r.get_u32(32),
                to: r.get_u32(36),
                order: r.get_u32(40),
                flags: r.get_u32(44),
                capacity: (den != 0).then_some((num, den)),
                latency: r.get_u32(56),
                param: r.get_i64(64),
                filter: (0..m)
                    .map(|i| Good(r.get_u32(EDGE_FIXED + 4 * i)))
                    .collect(),
            })
        }
        IND_GRAPH_RECIPE => {
            if size < RECIPE_FIXED {
                return Err(bad);
            }
            let (ni, no) = (r.get_u32(40) as usize, r.get_u32(44) as usize);
            if ni.saturating_add(no) != n {
                return Err(bad);
            }
            check(kind, size, sized(RECIPE_FIXED, n, TERM24))?;
            let term = |i: usize| {
                let e = RECIPE_FIXED + TERM24 * i;
                Term {
                    good: Good(r.get_u32(e)),
                    flags: r.get_u32(e + 4),
                    num: r.get_i64(e + 8),
                    den: r.get_i64(e + 16),
                }
            };
            GlassObs::Recipe(GraphRecipe {
                head: read_head(&r),
                recipe_type: r.get_u32(32),
                heat: r.get_u32(36),
                inputs: (0..ni).map(term).collect(),
                outputs: (ni..n).map(term).collect(),
                duration: r.get_u32(48),
                name: r.get_u32(52),
            })
        }
        IND_GRAPH_DOMAIN => {
            if size < DOMAIN_FIXED {
                return Err(bad);
            }
            let l = r.get_u32(40) as usize;
            check(
                kind,
                size,
                sized(sized(DOMAIN_FIXED, l, LEVEL24), n, MEMBER24),
            )?;
            let m0 = DOMAIN_FIXED + LEVEL24 * l;
            GlassObs::Domain(GraphDomain {
                head: read_head(&r),
                network: r.get_u64(32),
                current: r.get_u32(44),
                driver: opt(r.get_u32(48)),
                flags: r.get_u32(52),
                live_stress: r.get_f64(56),
                live_capacity: r.get_f64(64),
                levels: (0..l)
                    .map(|i| {
                        let e = DOMAIN_FIXED + LEVEL24 * i;
                        DomainLevel {
                            selector: r.get_u32(e),
                            speed_num: r.get_i32(e + 4),
                            speed_den: r.get_u32(e + 8),
                            capacity: r.get_f64(e + 16),
                        }
                    })
                    .collect(),
                members: (0..n)
                    .map(|i| {
                        let e = m0 + MEMBER24 * i;
                        Member {
                            node: opt(r.get_u32(e)),
                            count: r.get_u32(e + 4),
                            ratio_num: r.get_i32(e + 8),
                            ratio_den: r.get_u32(e + 12),
                            impact: r.get_f64(e + 16),
                        }
                    })
                    .collect(),
            })
        }
        IND_GRAPH_END => {
            check(kind, size, END_SIZE)?;
            GlassObs::End(GraphEnd {
                district: r.district(16),
                compile: r.get_u32(24),
                flags: aux,
                window: r.get_u32(28),
                records: r.get_u32(32),
                nodes: r.get_u32(36),
                edges: r.get_u32(40),
                recipes: r.get_u32(44),
                domains: r.get_u32(48),
                reason: r.get_u32(52),
                fingerprint: r.get_u64(56),
                frame: r.get_u64(64),
                revision: r.get_u32(72),
                compile_ticks: r.get_u32(76),
            })
        }
        IND_PROBE_SAMPLE => {
            check(kind, size, sized(PROBE_FIXED, n, 16))?;
            GlassObs::ProbeSample(ProbeSample {
                district: r.district(16),
                sweep: r.get_u32(24),
                node: r.get_u32(28),
                compile: r.get_u32(32),
                goods: r.goods(PROBE_FIXED, n),
            })
        }
        IND_CHUNK_TICKING => {
            check(kind, size, TICKING_SIZE)?;
            let ticking = match aux {
                0 => false,
                1 => true,
                v => {
                    return Err(WireError::BadCode {
                        kind,
                        field: "ticking",
                        value: v.into(),
                    });
                }
            };
            let o = GlassObs::ChunkTicking(ChunkTicking {
                district: r.district(24),
                chunk: ChunkPos {
                    dim: r.get_u32(32),
                    x: r.get_i32(36),
                    z: r.get_i32(40),
                },
                ticking,
            });
            return Ok((r.get_u64(8), Tick(r.get_i64(16) as u64), o));
        }
        _ => unreachable!("decode_obs is called for glass kinds only"),
    };
    Ok((0, at, o))
}

/// Appends IND_PROBE_SET.
pub fn encode_probe_set(cmd_seq: u64, p: &ProbeSet, out: &mut Vec<u8>) -> Result<(), WireError> {
    let mut w = W::cmd(out, IND_PROBE_SET, n32(p.nodes.len()), cmd_seq);
    w.put_u64(p.district.0);
    w.put_u32(p.compile);
    w.zero(4);
    for n in &p.nodes {
        w.put_u32(*n);
    }
    w.finish()
}

/// Decodes IND_PROBE_SET (header split off by the caller).
pub fn decode_probe_set(kind: u16, aux: u32, rec: &[u8]) -> Result<(u64, ProbeSet), WireError> {
    let r = R(rec);
    let n = aux as usize;
    check(kind, rec.len(), pad8(sized(PROBE_SET_FIXED, n, 4)))?;
    Ok((
        r.get_u64(8),
        ProbeSet {
            district: r.district(16),
            compile: r.get_u32(24),
            nodes: (0..n).map(|i| r.get_u32(PROBE_SET_FIXED + 4 * i)).collect(),
        },
    ))
}

/// Rounded up to a multiple of 8, as `W::finish` pads.
fn pad8(n: usize) -> usize {
    n.saturating_add(7) & !7
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::Cmd;
    use crate::ids::{BlockPos, DistrictId, GoodVec};
    use crate::obs::{Obs, Timed};
    use crate::wire;

    fn head(i: u32) -> GraphHead {
        GraphHead {
            district: DistrictId(7),
            compile: 3,
            index: i,
        }
    }

    fn samples() -> Vec<Timed> {
        let at = Tick(1234);
        let node = GraphNode {
            head: head(0),
            kind: NodeKind::Machine,
            model: super::super::records::model::MIXER,
            domain: Some(0),
            inventory: None,
            flags: 2,
            pos: BlockPos::new(-5, 64, 9),
            rule: 0,
            capacity: 0,
            params: [1, -2, 3, -4],
            rates: vec![RateEntry {
                level: 0,
                recipe: Some(0),
                batch: 1,
                ticks_num: 17,
                ticks_den: 1,
                flags: 0,
            }],
        };
        let edge = GraphEdge {
            head: head(0),
            kind: EdgeKind::Belt,
            from: 0,
            to: 1,
            order: 0,
            flags: 0,
            capacity: Some((8, 15)),
            latency: 37,
            param: 64,
            filter: vec![Good(5)],
        };
        let recipe = GraphRecipe {
            head: head(0),
            recipe_type: 4,
            heat: 2,
            inputs: vec![
                Term {
                    good: Good(1),
                    flags: 0,
                    num: 1,
                    den: 1,
                },
                Term {
                    good: Good(2),
                    flags: 0,
                    num: 1,
                    den: 1,
                },
            ],
            outputs: vec![Term {
                good: Good(3),
                flags: 0,
                num: 2,
                den: 1,
            }],
            duration: 100,
            name: 9,
        };
        let domain = GraphDomain {
            head: head(0),
            network: 99,
            current: 0,
            driver: None,
            flags: 0,
            live_stress: 1024.0,
            live_capacity: 4096.0,
            levels: vec![DomainLevel {
                selector: 0,
                speed_num: 256,
                speed_den: 1,
                capacity: 4096.0,
            }],
            members: vec![Member {
                node: Some(0),
                count: 1,
                ratio_num: -1,
                ratio_den: 2,
                impact: 4.0,
            }],
        };
        let end = GraphEnd {
            district: DistrictId(7),
            compile: 3,
            flags: 1,
            window: 2_400,
            records: 4,
            nodes: 1,
            edges: 1,
            recipes: 1,
            domains: 1,
            reason: 0,
            fingerprint: 0xDEAD_BEEF,
            frame: 0,
            revision: 2,
            compile_ticks: 3,
        };
        let probe = ProbeSample {
            district: DistrictId(7),
            sweep: 4,
            node: 1,
            compile: 3,
            goods: GoodVec::from_pairs(vec![(Good(1), 5)]),
        };
        let ticking = ChunkTicking {
            district: DistrictId(7),
            chunk: ChunkPos {
                dim: 1,
                x: -2,
                z: 3,
            },
            ticking: false,
        };
        [
            GlassObs::Node(node),
            GlassObs::Edge(edge),
            GlassObs::Recipe(recipe),
            GlassObs::Domain(domain),
            GlassObs::End(end),
            GlassObs::ProbeSample(probe),
            GlassObs::ChunkTicking(ticking),
        ]
        .into_iter()
        .map(|g| Timed {
            at,
            obs: Obs::Glass(g),
        })
        .collect()
    }

    #[test]
    fn every_glass_record_round_trips_through_the_industry_codec() {
        for t in samples() {
            let mut b = Vec::new();
            wire::encode_obs(42, &t, &mut b).expect("encode");
            assert_eq!(b.len() % 8, 0);
            assert_eq!(usize::from(u16::from_le_bytes([b[2], b[3]])), b.len());
            let (seq, back) = wire::decode_obs(&b).expect("decode").expect("known");
            assert_eq!(back, t);
            assert_eq!(seq, if t.obs.is_journaled() { 42 } else { 0 });
            // One byte short of its size is truncated; a size that disagrees with the counts is bad.
            assert!(wire::decode_obs(&b[..b.len() - 1]).is_err());
            let mut grown = b.clone();
            grown.extend_from_slice(&[0; 8]);
            let size = (b.len() + 8) as u16;
            grown[2..4].copy_from_slice(&size.to_le_bytes());
            assert!(
                matches!(wire::decode_obs(&grown), Err(WireError::BadSize { .. })),
                "{:?}",
                t.obs
            );
        }
    }

    #[test]
    fn unknown_kind_codes_are_errors_not_guesses() {
        let t = &samples()[0];
        let mut b = Vec::new();
        wire::encode_obs(0, t, &mut b).unwrap();
        b[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(
            wire::decode_obs(&b),
            Err(WireError::BadCode {
                field: "node_kind",
                ..
            })
        ));
    }

    #[test]
    fn probe_set_round_trips() {
        let c = Cmd::ProbeSet(ProbeSet {
            district: DistrictId(3),
            compile: 2,
            nodes: vec![4, 5, 6],
        });
        let mut b = Vec::new();
        wire::encode_cmd(77, &c, &mut b).unwrap();
        assert_eq!(b.len(), 48);
        assert_eq!(wire::decode_cmd(&b), Ok(Some((77, c))));
    }
}
