// SPDX-License-Identifier: AGPL-3.0-or-later
//! The TOPOLOGY record (docs/boundary.md §11): how the bridge tells the engine a frame's
//! topology. This is the reference codec, mirrored by the core's `topology/TopologyRecords.java`:
//! the two change together.
//!
//! Latest-wins, kind 0x00B0, `aux` = the dimension (Java's dictionary id). The bridge sends one
//! per level at every session start, before anything journaled, and again when a level's
//! topology changes (the wrap engine installing a layout ends its edge_stop).
//!
//! | Offset | Type | Field |
//! |---|---|---|
//! | 8 | u8 | kind: 0 flat, 1 x wrap, 2 glued |
//! | 9 | u8 | n, the edges that follow (at most 4) |
//! | 10 | u8 | flags: 1 the rectangle below is set |
//! | 11 | u8 | source: 0 the bridge's default, 1 Toroidal World, 2 Deep Time's layout, 3 a plugin |
//! | 12 | u32 | reserved |
//! | 16, 24, 32, 40 | i64 | x0, z0, x1, z1: flat, the world border; x wrap, the lap in x and the border in z; glued, the domain (cells, half-open) |
//! | 48 | | n edges of 24 bytes: 0 u8 side (0 west, 1 east, 2 north, 3 south), 1 u8 quarter turns, 2 u8 flags (1 stopped by edge_stop), 3 to 7 reserved, 8 i64 tx, 16 i64 tz (the motion beyond → inside) |
//!
//! Size 48 + 24n.

use super::Kind;
use super::glued::Side;
use super::iso::{Iso, Rect};
use super::layout::{Spec, SpecEdge};

/// The record's kind.
pub const TOPOLOGY: u16 = 0x00B0;
/// Bytes before the edges.
pub const HEAD: usize = 48;
/// Bytes per edge.
pub const EDGE: usize = 24;
/// Flag: the rectangle is set.
pub const FLAG_RECT: u8 = 1;
/// Edge flag: stopped by edge_stop.
pub const EDGE_STOPPED: u8 = 1;

/// Who described the topology (informational; the engine logs it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Source {
    Default = 0,
    ToroidalWorld = 1,
    DeepTime = 2,
    Plugin = 3,
}

impl Source {
    pub fn from_u8(v: u8) -> Source {
        match v {
            1 => Source::ToroidalWorld,
            2 => Source::DeepTime,
            3 => Source::Plugin,
            _ => Source::Default,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::Default => "the bridge's default",
            Source::ToroidalWorld => "Toroidal World",
            Source::DeepTime => "Deep Time's layout",
            Source::Plugin => "a plugin",
        }
    }
}

/// The record's size for `n` edges.
pub fn size(n: usize) -> usize {
    HEAD + EDGE * n
}

fn i64_at(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

/// Reads a record's body (the whole record, header included). `None` when it is malformed.
pub fn decode(b: &[u8]) -> Option<(Spec, Source)> {
    if b.len() < HEAD {
        return None;
    }
    let kind = match b[8] {
        0 => Kind::Flat,
        1 => Kind::XWrap,
        2 => Kind::Glued,
        _ => return None,
    };
    let n = usize::from(b[9]);
    if n > 4 || b.len() < size(n) {
        return None;
    }
    let rect = (b[10] & FLAG_RECT != 0)
        .then(|| Rect::new(i64_at(b, 16), i64_at(b, 24), i64_at(b, 32), i64_at(b, 40)));
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let o = HEAD + EDGE * i;
        edges.push(SpecEdge {
            side: Side::from_u8(b[o])?,
            iso: Iso::new(i64::from(b[o + 1]), i64_at(b, o + 8), i64_at(b, o + 16)),
            stopped: b[o + 2] & EDGE_STOPPED != 0,
        });
    }
    Some((Spec { kind, rect, edges }, Source::from_u8(b[11])))
}

/// Writes a record (header included: kind, size, `aux` = dimension) for `spec`.
pub fn encode(spec: &Spec, source: Source, dimension: u32) -> Vec<u8> {
    let n = spec.edges.len().min(4);
    let mut b = vec![0u8; size(n)];
    b[0..2].copy_from_slice(&TOPOLOGY.to_le_bytes());
    b[2..4].copy_from_slice(&(size(n) as u16).to_le_bytes());
    b[4..8].copy_from_slice(&dimension.to_le_bytes());
    b[8] = spec.kind as u8;
    b[9] = n as u8;
    b[11] = source as u8;
    if let Some(r) = spec.rect {
        b[10] = FLAG_RECT;
        for (at, v) in [(16, r.x0), (24, r.z0), (32, r.x1), (40, r.z1)] {
            b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        }
    }
    for (i, e) in spec.edges.iter().take(n).enumerate() {
        let o = HEAD + EDGE * i;
        b[o] = e.side as u8;
        b[o + 1] = e.iso.k;
        b[o + 2] = if e.stopped { EDGE_STOPPED } else { 0 };
        b[o + 8..o + 16].copy_from_slice(&e.iso.tx.to_le_bytes());
        b[o + 16..o + 24].copy_from_slice(&e.iso.tz.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::super::layout;
    use super::*;

    #[test]
    fn round_trips() {
        for t in [
            super::super::Topology::flat(),
            super::super::Topology::x_wrap(-6656, 13312),
            layout::guyou(6656, true),
            layout::guyou(6656, false),
            layout::torus(Rect::new(-512, -512, 512, 512)),
        ] {
            let spec = Spec::of(&t);
            let b = encode(&spec, Source::DeepTime, 3);
            assert_eq!(b.len() % 8, 0);
            let (back, src) = decode(&b).expect("well-formed");
            assert_eq!(src, Source::DeepTime);
            assert_eq!(back, spec);
            assert_eq!(back.build().expect("builds"), t);
        }
        assert!(decode(&[0u8; 16]).is_none());
    }
}
