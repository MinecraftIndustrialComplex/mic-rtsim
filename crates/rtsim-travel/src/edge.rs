// SPDX-License-Identifier: AGPL-3.0-or-later
//! An edge between two markets and what flows along it, each way (A13, SCALE.md §3, PLAN.md §8.4
//! "Flows").
//!
//! Each direction holds a pipeline per good it carries, a pipeline per currency it pays in (money
//! flows back at the quoted price), its caravan lattice, and its news state: the gossip cursor into
//! the origin's noticeboard and the packet waiting for the next caravan. A direction may be preyed
//! on: a share of every consignment goes to a predator haven's market instead (C24), so stolen
//! goods stay in the world's totals.

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_knowledge::BoardCursor;
use rtsim_trade::{Amount, GoodId, Goods, MILLI};

use crate::lattice::Lattice;
use crate::pipeline::Pipeline;

/// Raiders on a road (C24): `share` thousandths of every consignment are taken and carried to the
/// haven's market, arriving after `delay` quanta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Predation {
    pub haven: usize,
    pub share_milli: i64,
    pub delay: u64,
}

#[derive(Clone, Debug)]
pub struct Dir {
    pub from: usize,
    pub to: usize,
    /// Goods from `from` to `to`.
    pub goods: BTreeMap<GoodId, Pipeline>,
    /// Payments from `from` to `to`, per currency, for goods flowing the other way.
    pub pay: BTreeMap<GoodId, Pipeline>,
    /// What raiders take from this direction's goods, on its way to the haven.
    pub raid: BTreeMap<GoodId, Pipeline>,
    pub predation: Option<Predation>,
    pub lattice: Lattice,
    /// Where the origin's noticeboard was last read for this direction's packets.
    pub board_cursor: BoardCursor,
    /// News relayed on this direction so far: snapshots the origin received after this.
    pub relayed_through: Tick,
    /// The packet waiting for the next caravan: (departure boundary, packet index in the world).
    pub pending: Option<(u64, u32)>,
}

impl Dir {
    pub fn new(from: usize, to: usize, idle_period: u64) -> Self {
        Dir {
            from,
            to,
            goods: BTreeMap::new(),
            pay: BTreeMap::new(),
            raid: BTreeMap::new(),
            predation: None,
            lattice: Lattice::new(0, idle_period),
            board_cursor: BoardCursor::default(),
            relayed_through: Tick::ZERO,
            pending: None,
        }
    }

    /// Volume per quantum in chest slots (thousandths of a slot), from the current rates: what
    /// sets how often caravans leave.
    /// Any flow at all counts as at least one thousandth of a slot: a caravan carrying only
    /// payments is still a caravan, and news rides it.
    pub fn volume_slot_milli(&self, goods: &Goods) -> i64 {
        let mut v: i128 = 0;
        let mut any = false;
        for (g, p) in self.goods.iter().chain(self.pay.iter()) {
            let r = p.current_rate();
            any |= r > 0;
            v += i128::from(r) / i128::from(goods.get(*g).stack.max(1));
        }
        if any { (v as i64).max(1) } else { 0 }
    }

    /// Every pipeline of the direction (goods, payments and raids), for sums and trims.
    pub fn pipelines(&self) -> impl Iterator<Item = (GoodId, &Pipeline)> {
        self.goods
            .iter()
            .chain(self.pay.iter())
            .chain(self.raid.iter())
            .map(|(g, p)| (*g, p))
    }

    /// Per good, what is on the road once boundary `b` is booked (raids included: they are on
    /// their way to the haven).
    pub fn in_transit(&self, b: u64, out: &mut BTreeMap<GoodId, i128>) {
        for (g, p) in self.pipelines() {
            *out.entry(g).or_default() += p.in_transit(b);
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.pipelines()
            .map(|(_, p)| p.heap_bytes() + 48)
            .sum::<usize>()
            + 128
    }
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub length_blocks: u64,
    /// Travel time in quanta, at least 1.
    pub d: u64,
    /// The longest a consignment waits at the dock for its caravan, in quanta (overnight choice
    /// E53). Every pipeline's delay is [`Edge::lag`]: the wait plus the walk.
    pub dock: u64,
    /// Keys the travellers sampled on it.
    pub seed: u64,
    /// The ends keep different currencies.
    pub border: bool,
    /// `dirs[0]` is a to b, `dirs[1]` b to a.
    pub dirs: [Dir; 2],
    /// Player bases near the road that its caravans stop at (C26, P30's recommendation), as node
    /// indices.
    pub stops: Vec<usize>,
}

impl Edge {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        a: usize,
        b: usize,
        length_blocks: u64,
        d: u64,
        dock: u64,
        seed: u64,
        border: bool,
        idle_period: u64,
    ) -> Self {
        Edge {
            a,
            b,
            length_blocks,
            d: d.max(1),
            dock,
            seed,
            border,
            dirs: [Dir::new(a, b, idle_period), Dir::new(b, a, idle_period)],
            stops: Vec::new(),
        }
    }

    /// A pipeline's delay: the longest dock wait plus the walk. A consignment that left at `q`
    /// rides the caravan that leaves at most `dock` later and is counted in transit until
    /// `q + lag`, so the caravan's whole load stays on the road while it walks.
    pub fn lag(&self) -> u64 {
        self.d + self.dock
    }

    /// The direction leaving `from`.
    pub fn dir_from(&self, from: usize) -> usize {
        if from == self.a { 0 } else { 1 }
    }

    pub fn other(&self, end: usize) -> usize {
        if end == self.a { self.b } else { self.a }
    }
}

/// A reference to one direction of one edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirRef {
    pub edge: u32,
    pub dir: u8,
}

/// The caravan period for a volume: the quanta it takes the dock to fill a caravan (its slots
/// over the volume per quantum, rounded up), at least 1 and at most `max_wait`, the longest
/// goods wait at a dock (overnight choice E53); `idle` when nothing moves.
pub fn caravan_period(volume_slot_milli: i64, slots: u32, max_wait: u64, idle: u64) -> u64 {
    if volume_slot_milli <= 0 {
        return idle.max(1);
    }
    let full = i64::from(slots) * MILLI;
    let p = (full + volume_slot_milli - 1) / volume_slot_milli;
    (p.max(1) as u64).min(max_wait.max(1))
}

/// Splits a rate between the destination and raiders: (to the destination, to the haven).
pub fn split_for_raid(rate: Amount, predation: Option<Predation>) -> (Amount, Amount) {
    match predation {
        Some(p) if rate > 0 => {
            let taken =
                (i128::from(rate) * i128::from(p.share_milli.clamp(0, 1000)) / 1000) as Amount;
            (rate - taken, taken)
        }
        _ => (rate, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods() {
        // 6 slots a caravan, 1.5 slots a quantum: one caravan every 4 quanta, full.
        assert_eq!(caravan_period(1_500, 6, 24, 24), 4);
        // 1.4 slots a quantum: the fifth quantum fills it.
        assert_eq!(caravan_period(1_400, 6, 24, 24), 5);
        // More than a caravan's worth a quantum: one every quantum (a bigger caravan).
        assert_eq!(caravan_period(60_000, 6, 24, 24), 1);
        // A trickle: never more than the longest dock wait apart.
        assert_eq!(caravan_period(1, 6, 24, 48), 24);
        assert_eq!(caravan_period(0, 6, 24, 48), 48);
        assert_eq!(
            split_for_raid(
                1000,
                Some(Predation {
                    haven: 0,
                    share_milli: 125,
                    delay: 3
                })
            ),
            (875, 125)
        );
    }
}
