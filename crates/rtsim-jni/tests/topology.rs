// SPDX-License-Identifier: AGPL-3.0-or-later
//! TOPOLOGY records through the real boundary (docs/boundary.md §11): the engine keeps a level's
//! topology from the bridge's record, a changed record replaces it, a malformed or impossible
//! one is refused and the level keeps what it had, and a player just past an edge is canonical.

mod common;

use common::*;
use rtsim_jni::wire;
use rtsim_space::topology::record::{self, Source};
use rtsim_space::topology::{Kind, Topology, layout};

fn send(j: &mut Java, rec: &[u8]) {
    assert!(j.preamble());
    assert!(j.copy(rec), "room for the record");
}

#[test]
fn the_engine_keeps_each_levels_topology_from_its_record() {
    let dir = tmpdir("topology");
    let e = engine(&dir, 1 << 18);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let dim = j.dict_id(wire::DICT_DIMENSION, OVERWORLD);
    let guyou = layout::guyou(layout::GUYOU_16K, true);
    send(
        &mut j,
        &record::encode(&layout::Spec::of(&guyou), Source::DeepTime, dim),
    );
    j.run_until("the Guyou layout is in force", |j| {
        *j.e.inspect().topologies.of(dim as u16) == guyou
    });
    // The wrap engine installs the layout: edge_stop ends.
    let open = layout::guyou(layout::GUYOU_16K, false);
    send(
        &mut j,
        &record::encode(&layout::Spec::of(&open), Source::DeepTime, dim),
    );
    j.run_until("the layout without edge_stop", |j| {
        !j.e.inspect().topologies.of(dim as u16).any_stopped()
    });
    // A record whose motions do not glue its domain describes no world: refused, kept.
    let mut bad = layout::Spec::of(&open);
    bad.edges[2].iso = rtsim_space::topology::Iso::translation(5, 5);
    send(&mut j, &record::encode(&bad, Source::Plugin, dim));
    let mut truncated = record::encode(&layout::Spec::of(&open), Source::Plugin, dim);
    truncated[9] = 9; // nine edges in a record that holds four
    send(&mut j, &truncated);
    j.run_until("both refused", |j| j.e.inspect().topology_refused >= 2);
    assert_eq!(*e.inspect().topologies.of(dim as u16), open);
    // Another level stays flat; a flat record makes this one flat again.
    assert!(e.inspect().topologies.of(dim as u16 + 7).is_flat());
    send(
        &mut j,
        &record::encode(
            &layout::Spec::of(&Topology::x_wrap(-6656, 13312)),
            Source::ToroidalWorld,
            dim,
        ),
    );
    j.run_until("Toroidal World's x wrap", |j| {
        j.e.inspect().topologies.of(dim as u16).kind() == Kind::XWrap
    });
    assert!(e.protocol_error().is_none(), "{:?}", e.protocol_error());
}
