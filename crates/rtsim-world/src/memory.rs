// SPDX-License-Identifier: AGPL-3.0-or-later
//! Where a world's memory goes (overnight choice E58): an estimate per component from sizes and
//! capacities, for the bench and for slimming the layout. `rtsim-cli sim memory` checks the sum
//! against a counting allocator.

use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::sync::Arc;

use rtsim_trade::line::Line;
use rtsim_trade::market::GoodState;
use rtsim_travel::news::{GoodNews, Snapshot};

use crate::world::World;

/// A B-tree map's rough cost per entry beyond the entry itself (node headers, slack in the 11-key
/// nodes): ESTIMATE.
const BTREE_ENTRY: usize = 16;

impl World {
    /// Heap bytes by component, estimated from sizes and capacities.
    pub fn memory(&self) -> BTreeMap<&'static str, usize> {
        let mut m: BTreeMap<&'static str, usize> = BTreeMap::new();
        // Markets: the lazy table's arena of regions, each owning its market's vectors.
        let mut lines = 0;
        let mut goods = 0;
        let mut other = 0;
        for i in 0..self.nodes.len() {
            let r = self.markets.get(self.nodes[i].region).expect("market");
            let mk = r.cell().model();
            lines += mk.lines.capacity() * size_of::<Line>();
            goods += mk.goods.capacity() * size_of::<GoodState>()
                + mk.goods
                    .iter()
                    .map(|g| g.inputs.capacity() * 24)
                    .sum::<usize>();
            other += mk.order.capacity() * 2 + mk.base_prices.capacity() * 24;
        }
        m.insert("market lines (stock, rates, books)", lines);
        m.insert("market goods (curves, recipes)", goods);
        m.insert("market other (order, base prices)", other);
        // The table's heap holds the resident rows' measured heap too: the rest is its index,
        // region headers and wheel.
        m.insert(
            "market table (regions, wheel)",
            self.markets
                .heap_bytes()
                .saturating_sub(lines + goods + other),
        );
        // News: the snapshots alive (shared), and the rings that point at them.
        let mut seen: BTreeSet<*const Snapshot> = BTreeSet::new();
        let mut snaps = 0;
        let count = |s: &Arc<Snapshot>, seen: &mut BTreeSet<*const Snapshot>| {
            if seen.insert(Arc::as_ptr(s)) {
                size_of::<Snapshot>()
                    + 16
                    + s.goods.capacity() * size_of::<GoodNews>()
                    + s.inflows.capacity() * 16
            } else {
                0
            }
        };
        let mut rings = 0;
        for n in &self.nodes {
            for (_, s) in n.news.iter() {
                snaps += count(s, &mut seen);
                rings += 24 + BTREE_ENTRY;
            }
            if let Some(s) = &n.last_sent {
                snaps += count(s, &mut seen);
            }
        }
        let mut packets = 0;
        for p in self.packets.values() {
            for s in p.own.iter().chain(p.relays.iter()) {
                snaps += count(s, &mut seen);
            }
            packets += size_of::<crate::world::PacketData>()
                + BTREE_ENTRY
                + p.relays.capacity() * 8
                + p.reports.len() * 6;
        }
        m.insert("news snapshots", snaps);
        m.insert("news rings", rings);
        m.insert("news packets", packets);
        m.insert(
            "nodes (rows, edge lists)",
            self.nodes.capacity() * size_of::<crate::world::Node>()
                + self
                    .nodes
                    .iter()
                    .map(|n| {
                        n.out.capacity() * 8
                            + n.neighbours.capacity() * 8
                            + n.raids_in.capacity() * 8
                            + n.bands.capacity() * 8
                            + n.costs.labour.capacity() * 8
                            + n.costs.recipe.capacity() * 16
                            + n.costs.capacity_per_day.capacity() * 8
                    })
                    .sum::<usize>(),
        );
        // Edges: the rows, and each direction's pipelines and lattice.
        let mut pipes = 0;
        let mut pipe_maps = 0;
        let mut lattices = 0;
        for e in &self.edges {
            for d in &e.dirs {
                for (_, p) in d.pipelines() {
                    pipes += p.heap_bytes();
                }
                pipe_maps += d.goods.heap_bytes() + d.pay.heap_bytes() + d.raid.heap_bytes();
                lattices += d.lattice.heap_bytes();
            }
        }
        m.insert(
            "edges (rows)",
            self.edges.capacity() * size_of::<rtsim_travel::Edge>(),
        );
        m.insert("pipelines (segments, holes)", pipes);
        m.insert("pipeline maps", pipe_maps);
        m.insert("caravan lattices", lattices);
        m.insert("knowledge", self.knowledge.heap_bytes());
        m.insert("leg index", self.legs.heap_bytes());
        m.insert("road index", self.roads.heap_bytes());
        m.insert(
            "world wheel and wakes",
            self.wheel.heap_bytes() + self.wakes.len() * 32,
        );
        m.insert(
            "caravans, bands, settlements",
            self.caravans.heap_bytes()
                + self
                    .bands
                    .iter()
                    .map(|b| b.route.capacity() * 8 * 6)
                    .sum::<usize>()
                + self.settlements.heap_bytes(),
        );
        m
    }
}
