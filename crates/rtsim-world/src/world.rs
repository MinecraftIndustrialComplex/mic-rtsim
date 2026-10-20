// SPDX-License-Identifier: AGPL-3.0-or-later
//! The world: tables, building, joining, and the step loop.

use std::collections::BTreeMap;
use std::sync::Arc;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::lazy::{LazyTable, RegionId};
use rtsim_kernel::rng::{StreamKey, SystemId};
use rtsim_kernel::wheel::TimingWheel;
use rtsim_knowledge::{ActorId, ActorKind, HolderKind, Knowledge, VanillaGossip};
use rtsim_settle::{NodeKind, Pos, Settlement, SettlementId, Settlements};
use rtsim_trade::build::{
    Built, LinkSpec, NodeSpec, build_markets, costs_for_newcomer, make_market,
};
use rtsim_trade::labour::{Costs, Lint, Local, RecipeBook, ResourceId};
use rtsim_trade::{Amount, GoodId, Goods, Market, MarketKind};
use rtsim_travel::caravan::Caravan;
use rtsim_travel::edge::{DirRef, Edge};
use rtsim_travel::legs::LegIndex;
use rtsim_travel::news::{NewsRing, Snapshot};

use crate::config::WorldConfig;

/// The markets' system id: keys their lazy table (they draw nothing, but the table wants one).
pub const SYS_MARKET: SystemId = SystemId::named("trade.market");
/// Edge seeds.
pub const SYS_EDGE: SystemId = SystemId::named("travel.edge");

/// A settlement to add, as data: what the bridge's census and structure observation report.
#[derive(Clone, Debug)]
pub struct NodeData {
    pub key: u64,
    pub kind: NodeKind,
    pub pos: Pos,
    pub pop: u32,
    /// Resource flows: the most output per day each resource supports, thousandths.
    pub resources: BTreeMap<ResourceId, Amount>,
}

/// A market node and everything the world keeps about it besides the market itself.
pub struct Node {
    pub settlement: SettlementId,
    pub key: u64,
    pub kind: NodeKind,
    /// The currency its market keeps, if any.
    pub currency: Option<GoodId>,
    pub pos: Pos,
    pub actor: ActorId,
    pub region: RegionId<Market>,
    pub news: NewsRing,
    /// Directions leaving this node.
    pub out: Vec<DirRef>,
    /// Directions whose raids land here (this node is their haven).
    pub raids_in: Vec<DirRef>,
    /// Neighbour node indices, never evicted from the news ring.
    pub neighbours: Vec<usize>,
    /// The last snapshot this node sent, to tell whether the next one is news.
    pub last_sent: Option<Arc<Snapshot>>,
    pub costs: Costs,
    pub local: Local,
}

/// A news packet waiting for its caravan, or on the road.
#[derive(Clone, Debug)]
pub struct PacketData {
    pub dir: DirRef,
    pub from: usize,
    pub to: usize,
    pub departs: u64,
    pub own: Option<Arc<Snapshot>>,
    pub relays: Vec<Arc<Snapshot>>,
    pub reports: rtsim_knowledge::Packet,
}

pub type CaravanId = Handle<Caravan>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ev {
    /// A pipeline's arrivals change at the next boundary: the market must re-read its inflows.
    Wake(usize),
    /// A news packet reaches its destination.
    Deliver(u32),
    /// An individual caravan reaches the end of its leg.
    Arrive { caravan: CaravanId, generation: u32 },
}

/// Conserved quantities outside markets and pipelines: what left the world to players or was
/// destroyed, and what players put in other than by trading at a market.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outside {
    pub player_in: BTreeMap<GoodId, i128>,
    pub player_out: BTreeMap<GoodId, i128>,
    pub destroyed: BTreeMap<GoodId, i128>,
}

/// Counters for the profiler, the bench and the quiescence test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Market events of every kind.
    pub market_events: u64,
    pub breakpoints: u64,
    pub wakes: u64,
    pub deliveries: u64,
    pub packets_sent: u64,
    pub segments: u64,
    pub caravan_arrivals: u64,
    pub promotions: u64,
    pub player_trades: u64,
}

/// What the engine asks Minecraft to do (the bridge's command buffer, later).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Write a report into a vanilla village's gossip (C20).
    VanillaGossip(VanillaGossip),
}

pub struct World {
    pub cfg: WorldConfig,
    pub goods: Goods,
    pub book: RecipeBook,
    pub settlements: Settlements,
    pub nodes: Vec<Node>,
    pub markets: LazyTable<Market>,
    pub edges: Vec<Edge>,
    pub knowledge: Knowledge,
    pub(crate) wheel: TimingWheel<Ev>,
    pub(crate) packets: BTreeMap<u32, PacketData>,
    /// Wakes already booked, so two pipeline changes landing at one boundary wake a market once.
    pub(crate) wakes: std::collections::BTreeSet<(Tick, usize)>,
    pub(crate) next_packet: u32,
    pub caravans: Arena<Caravan>,
    pub legs: LegIndex,
    pub(crate) leg_ids: BTreeMap<CaravanId, rtsim_travel::legs::LegId>,
    pub outside: Outside,
    pub stats: Stats,
    pub outbox: Vec<Command>,
    pub lints: Vec<Lint>,
    /// Each culture's mint cost, worker-days per coin.
    pub mint_cost: [f64; 2],
    pub(crate) now: Tick,
}

impl World {
    /// Builds a world from its settlements at once: markets, edges by the distance rule, actors
    /// and holders, and a first round of news from every market so trade can start.
    pub fn build(cfg: WorldConfig, goods: Goods, book: RecipeBook, data: Vec<NodeData>) -> World {
        let mut settlements = Settlements::new();
        let ids: Vec<SettlementId> = data
            .iter()
            .map(|d| {
                settlements.insert(Settlement {
                    key: d.key,
                    kind: d.kind,
                    pos: d.pos,
                    pop: d.pop,
                    joined: Tick::ZERO,
                })
            })
            .collect();
        let index_of: BTreeMap<SettlementId, usize> =
            ids.iter().enumerate().map(|(i, &s)| (s, i)).collect();
        let specs = settlements.edges(&cfg.edge_rule);
        let speed = cfg.caravan_blocks_per_tick;
        let q = cfg.trade.quantum;
        let links: Vec<LinkSpec> = specs
            .iter()
            .map(|e| LinkSpec {
                a: index_of[&e.a],
                b: index_of[&e.b],
                d_quanta: q.covering((e.length_blocks as f64 / speed).ceil() as u64),
            })
            .collect();
        let nodes_spec: Vec<NodeSpec> = data
            .iter()
            .map(|d| NodeSpec {
                key: d.key,
                kind: if d.kind.is_storehouse() {
                    MarketKind::Storehouse
                } else {
                    MarketKind::Base
                },
                culture: d.kind.culture(),
                local: Local {
                    pop: d.pop,
                    resources: d.resources.clone(),
                },
            })
            .collect();
        let Built {
            markets,
            costs,
            mint_cost,
            lints,
        } = build_markets(&goods, &book, &cfg.trade, &nodes_spec, &links, cfg.genesis);

        let mut w = World::empty(cfg, goods, book, settlements, mint_cost);
        w.lints = lints;
        for (i, (m, c)) in markets.into_iter().zip(costs).enumerate() {
            w.add_node(ids[i], &data[i], m, c, nodes_spec[i].local.clone());
        }
        for (k, l) in links.iter().enumerate() {
            w.add_edge(
                l.a,
                l.b,
                specs[k].length_blocks,
                l.d_quanta,
                specs[k].border,
            );
        }
        for i in 0..w.nodes.len() {
            w.emit(i, Tick::ZERO, true);
        }
        w
    }

    fn empty(
        cfg: WorldConfig,
        goods: Goods,
        book: RecipeBook,
        settlements: Settlements,
        mint_cost: [f64; 2],
    ) -> World {
        let knowledge = Knowledge::with_tunables(cfg.seed, cfg.knowledge.clone());
        let window = cfg.trade.quantum.len();
        World {
            markets: LazyTable::new(cfg.seed, SYS_MARKET, Tick::ZERO),
            wheel: TimingWheel::new(Tick::ZERO),
            cfg,
            goods,
            book,
            settlements,
            nodes: Vec::new(),
            edges: Vec::new(),
            knowledge,
            packets: BTreeMap::new(),
            wakes: std::collections::BTreeSet::new(),
            next_packet: 1,
            caravans: Arena::new(),
            legs: LegIndex::new(window),
            leg_ids: BTreeMap::new(),
            outside: Outside::default(),
            stats: Stats::default(),
            outbox: Vec::new(),
            lints: Vec::new(),
            mint_cost,
            now: Tick::ZERO,
        }
    }

    fn add_node(&mut self, sid: SettlementId, d: &NodeData, m: Market, costs: Costs, local: Local) {
        let actor = self
            .knowledge
            .actors_mut()
            .register(ActorKind::Settlement, d.key);
        let hk = match d.kind {
            NodeKind::Village(rtsim_settle::Culture::Human) => HolderKind::VanillaVillage,
            NodeKind::Village(rtsim_settle::Culture::Ribbit) => HolderKind::RibbitVillage,
            NodeKind::PlayerBase { .. } => HolderKind::PlayerBase,
            // A haven keeps no gossip of its own culture's kind; it remembers like a ribbit
            // village (no vanilla gossip to write).
            NodeKind::Haven(_) => HolderKind::RibbitVillage,
        };
        self.knowledge.add_holder(actor, hk);
        let region = self.markets.insert(d.key, m, self.now);
        let currency = d.kind.culture().and_then(|c| self.goods.currency(c));
        self.nodes.push(Node {
            settlement: sid,
            key: d.key,
            kind: d.kind,
            currency,
            pos: d.pos,
            actor,
            region,
            news: NewsRing::new(self.cfg.news_cap),
            out: Vec::new(),
            raids_in: Vec::new(),
            neighbours: Vec::new(),
            last_sent: None,
            costs,
            local,
        });
    }

    fn add_edge(&mut self, a: usize, b: usize, length: u64, d: u64, border: bool) {
        let e = self.edges.len() as u32;
        let seed = StreamKey::new(self.cfg.seed, SYS_EDGE, 0).block(
            self.nodes[a].key.min(self.nodes[b].key),
            self.nodes[a].key.max(self.nodes[b].key),
        )[0];
        let idle = self.cfg.trade.quantum.days(self.cfg.scout_days);
        self.edges
            .push(Edge::new(a, b, length, d, seed, border, idle));
        self.nodes[a].out.push(DirRef { edge: e, dir: 0 });
        self.nodes[b].out.push(DirRef { edge: e, dir: 1 });
        self.nodes[a].neighbours.push(b);
        self.nodes[b].neighbours.push(a);
    }

    /// A village joins when its structure start generates (C22): its market is built from its own
    /// recipes and its new neighbours' costs, and it links to the world by the edge rule. Existing
    /// markets keep their anchors (overnight choice E15); their `T` bound does not move either, so
    /// a longer new edge than they had is a known gap until anchors are recomputed.
    pub fn join(&mut self, d: NodeData, at: Tick) -> usize {
        self.run_until(at);
        let sid = self.settlements.insert(Settlement {
            key: d.key,
            kind: d.kind,
            pos: d.pos,
            pop: d.pop,
            joined: at,
        });
        let specs = self.settlements.edges_for(sid, &self.cfg.edge_rule);
        let q = self.cfg.trade.quantum;
        let speed = self.cfg.caravan_blocks_per_tick;
        let local = Local {
            pop: d.pop,
            resources: d.resources.clone(),
        };
        let index_of = |w: &World, s: SettlementId| w.nodes.iter().position(|n| n.settlement == s);
        let mut links = Vec::new();
        for e in &specs {
            let other = if e.a == sid { e.b } else { e.a };
            if let Some(j) = index_of(self, other) {
                let dq = q.covering((e.length_blocks as f64 / speed).ceil() as u64);
                links.push((j, e.length_blocks, dq, e.border));
            }
        }
        let neighbour_costs: Vec<(&Costs, u64)> = links
            .iter()
            .map(|&(j, _, dq, _)| (&self.nodes[j].costs, dq))
            .collect();
        let costs = costs_for_newcomer(
            &self.goods,
            &self.book,
            &self.cfg.trade,
            &local,
            &neighbour_costs,
        );
        let spec = NodeSpec {
            key: d.key,
            kind: if d.kind.is_storehouse() {
                MarketKind::Storehouse
            } else {
                MarketKind::Base
            },
            culture: d.kind.culture(),
            local: local.clone(),
        };
        let max_d = links.iter().map(|l| l.2).max().unwrap_or(0);
        let border = links
            .iter()
            .filter(|l| {
                let c = self.nodes[l.0].kind.culture();
                c.is_some() && c != d.kind.culture()
            })
            .map(|l| l.2)
            .min();
        let m = make_market(
            &self.goods,
            &self.book,
            &self.cfg.trade,
            &spec,
            &costs,
            self.mint_cost,
            max_d,
            border,
            self.cfg.genesis,
        );
        let i = self.nodes.len();
        // Genesis stock is ledgered in the line books, as at build.
        self.add_node(sid, &d, m, costs, local);
        for (j, len, dq, border) in links {
            self.add_edge(i, j, len, dq, border);
        }
        self.emit(i, at, true);
        for j in self.nodes[i].neighbours.clone() {
            self.emit(j, at, true);
        }
        i
    }

    /// Raiders from the haven at node `haven` take `share_milli` thousandths of everything moving
    /// from `from` to `to` (C24: predator havens). Their haul walks to the haven at caravan speed
    /// and is stocked there, so it stays in the world's totals. Takes effect at the next
    /// allocation on that road. Returns false when the two are not linked.
    pub fn set_predation(
        &mut self,
        from: usize,
        to: usize,
        haven: usize,
        share_milli: i64,
    ) -> bool {
        let Some(d) = self.nodes[from]
            .out
            .iter()
            .copied()
            .find(|d| self.edges[d.edge as usize].dirs[d.dir as usize].to == to)
        else {
            return false;
        };
        let dist = self.nodes[from]
            .pos
            .dist(self.nodes[haven].pos)
            .unwrap_or(0);
        let delay = self
            .cfg
            .trade
            .quantum
            .covering((dist as f64 / self.cfg.caravan_blocks_per_tick).ceil() as u64);
        self.edges[d.edge as usize].dirs[d.dir as usize].predation =
            Some(rtsim_travel::Predation {
                haven,
                share_milli,
                delay,
            });
        if !self.nodes[haven].raids_in.contains(&d) {
            self.nodes[haven].raids_in.push(d);
        }
        true
    }

    pub fn now(&self) -> Tick {
        self.now
    }

    pub fn node_of_key(&self, key: u64) -> Option<usize> {
        self.nodes.iter().position(|n| n.key == key)
    }

    /// The market at node `i` as it stands at the world's time (a projection; changes nothing).
    pub fn market(&self, i: usize) -> Market {
        let r = self
            .markets
            .get(self.nodes[i].region)
            .expect("every node has a market");
        r.cell().view_without_catch_up(self.now)
    }

    pub(crate) fn schedule(&mut self, at: Tick, ev: Ev) {
        let at = at.max(self.wheel.now());
        if let Ev::Wake(i) = ev
            && !self.wakes.insert((at, i))
        {
            return;
        }
        self.wheel.schedule(at, ev);
    }

    /// Runs every event up to and including `to`, in time order: market breakpoints first at
    /// equal times (the table's own rule: drain them before anything acts at that time), then
    /// world events in the order they were booked, then knowledge housekeeping.
    pub fn run_until(&mut self, to: Tick) {
        loop {
            // A lower bound on the world wheel's next event, in O(levels): an exact peek would
            // scan a whole wheel slot, which in a big world holds thousands of wakes. Popping up
            // to the bound cascades that slot, so the bound tightens as the loop goes.
            let tw = self.wheel.next_due_lower_bound().filter(|&t| t <= to);
            let tk = [self.knowledge.next_expiry(), self.knowledge.next_death()]
                .into_iter()
                .flatten()
                .filter(|&t| t <= to)
                .min();
            let limit = [tw, tk].into_iter().flatten().min().unwrap_or(to);
            if let Some((at, h)) = self.markets.next_due(limit) {
                self.now = self.now.max(at);
                let i = self.node_of_region(h);
                self.stats.breakpoints += 1;
                self.market_event(i, at, true);
                continue;
            }
            if tw == Some(limit) {
                if let Some((at, ev)) = self.wheel.pop_until(limit) {
                    self.now = self.now.max(at);
                    self.handle(at, ev);
                }
                continue;
            }
            if let Some(b) = tk.filter(|&b| b == limit) {
                self.now = self.now.max(b);
                self.knowledge.expire_due(b);
                while let Some(n) = self.knowledge.death_due(b) {
                    self.knowledge.die(n, b);
                }
                self.flush_gossip();
                continue;
            }
            break;
        }
        // Move the world's wheel to `to` so later bookings are never in its past (the market
        // table's wheel got there in the last `next_due`).
        let _ = self.wheel.pop_until(to);
        self.now = self.now.max(to);
    }

    pub fn run_days(&mut self, days: u64) {
        let to = self.now + days * TICKS_PER_DAY;
        self.run_until(to);
    }

    fn node_of_region(&self, h: RegionId<Market>) -> usize {
        // Regions are inserted in node order and never removed, so the index is the node.
        let i = h.index() as usize;
        debug_assert_eq!(self.nodes[i].region, h);
        i
    }

    fn handle(&mut self, at: Tick, ev: Ev) {
        match ev {
            Ev::Wake(i) => {
                self.wakes.remove(&(at, i));
                self.stats.wakes += 1;
                self.market_event(i, at, false);
            }
            Ev::Deliver(p) => self.deliver(p, at),
            Ev::Arrive {
                caravan,
                generation,
            } => self.caravan_arrives(caravan, generation, at),
        }
    }

    /// Commands for the bridge: vanilla gossip writes and the like, oldest first.
    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.outbox)
    }

    pub(crate) fn flush_gossip(&mut self) {
        for g in self.knowledge.take_vanilla_gossip() {
            self.outbox.push(Command::VanillaGossip(g));
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.markets.heap_bytes()
            + self
                .nodes
                .iter()
                .map(|n| n.news.heap_bytes() + n.out.capacity() * 8 + 256)
                .sum::<usize>()
            + self
                .edges
                .iter()
                .map(|e| e.dirs[0].heap_bytes() + e.dirs[1].heap_bytes() + 64)
                .sum::<usize>()
            + self.knowledge.heap_bytes()
            + self.legs.heap_bytes()
            + self.caravans.heap_bytes()
            + self.settlements.heap_bytes()
            + self.packets.len() * 128
    }
}
