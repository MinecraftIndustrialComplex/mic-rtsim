// SPDX-License-Identifier: AGPL-3.0-or-later
//! The world: tables, building, joining, and the step loop.

use std::collections::BTreeMap;
use std::sync::Arc;

use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::handle::{Arena, Handle};
use rtsim_kernel::lazy::RegionId;
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

/// How the market table writes a market: rtsim-trade's save form.
pub struct MarketCodec;

impl rtsim_kernel::lazy::ModelCodec<Market> for MarketCodec {
    fn put(m: &Market, w: &mut rtsim_kernel::codec::Writer) {
        rtsim_trade::persist::save_market(w, m);
    }

    fn get(r: &mut rtsim_kernel::codec::Reader<'_>) -> rtsim_kernel::codec::Result<Market> {
        rtsim_trade::persist::load_market(r)
    }

    fn heap_bytes(m: &Market) -> usize {
        m.heap_bytes()
    }
}

/// The market table: lazy regions in row pages.
pub type MarketTable = rtsim_kernel::lazy::PagedLazyTable<Market, MarketCodec>;

/// The page table of the market rows (docs/choices-paging-modules.md, "Page tables").
pub const TABLE_MARKETS: u16 = 0x5010;
/// Markets per page. PLACEHOLDER 1: a market row is 1 to 7 KB resident (its goods), about a
/// page already, and a market touched dirties only itself (docs/choices-paging-followups.md PF3).
pub const MARKETS_PER_PAGE: u64 = 1;

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
    /// Touring bands that stop here: (band, stop index on its circuit).
    pub bands: Vec<(u32, u32)>,
    /// The last snapshot this node sent, to tell whether the next one is news.
    pub last_sent: Option<Arc<Snapshot>>,
    pub costs: Costs,
    pub local: Local,
}

/// How a news packet travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// On this edge: with its next messenger (a road, overnight choice PR6), or its next ship (a
    /// ships' lane, which has no messengers).
    Road(DirRef),
    /// With a touring band, leaving this stop of its circuit (P16).
    Band { band: u32, stop: u32 },
}

/// A news packet waiting for its caravan, or on the road.
#[derive(Clone, Debug)]
pub struct PacketData {
    pub via: Via,
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
    /// A ribbit village's growth clock runs out (P17).
    Grow { node: usize, stamp: u32 },
    /// A touring band begins its stay at a stop of its circuit (P16).
    BandPlays { band: u32, stop: u32, stamp: u32 },
    /// A village's purse reaches its next work's price (SINK.md §4.4).
    Works { node: usize, stamp: u32 },
}

/// Conserved quantities outside markets and pipelines: what left the world to players or was
/// destroyed, and what players put in other than by trading at a market.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outside {
    pub player_in: BTreeMap<GoodId, i128>,
    pub player_out: BTreeMap<GoodId, i128>,
    pub destroyed: BTreeMap<GoodId, i128>,
    /// Goods another module's rows hold, still in the world (the ships engine's ships and
    /// pirates, `rtsim-ships`, overnight choice T6): counted as seen, so moving goods between a
    /// market or a flow and such a row is internal.
    pub held: BTreeMap<GoodId, i128>,
}

/// Why a market event ran (for the trace and the quiescence diagnostics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cause {
    /// One of the market's own breakpoints.
    Breakpoint,
    /// A pipeline's arrivals change.
    Wake,
    /// A news packet arrived.
    Delivery,
    /// An individual caravan arrived.
    Arrival,
    /// A player traded (at a storehouse or a base).
    Player,
    /// The village grew a house and its population stepped up (P17).
    Growth,
    /// A predator with no haven took stock (PLAN.md §9: a dragon raided the village).
    Predator,
    /// Something outside the market changed what its flows may carry: a lane's hazard or a
    /// ship's cargo (rtsim-ships). Sends news only when it changes in kind.
    Lane,
}

impl Cause {
    /// Whether the event sends news even when nothing changed in kind.
    pub fn forces_news(self) -> bool {
        matches!(
            self,
            Cause::Breakpoint | Cause::Player | Cause::Growth | Cause::Predator
        )
    }
}

/// One market event, as the trace records it ([`World::trace`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRow {
    pub at: Tick,
    pub node: usize,
    pub cause: Cause,
    /// Pipelines whose rate changed.
    pub changed: u32,
    /// Which: (destination, good, pipeline, rate before, rate after).
    pub flows: Vec<(usize, GoodId, crate::flows::PipeKind, Amount, Amount)>,
    /// Whether a snapshot went out.
    pub news: bool,
    /// Goods whose level or advertised rate differs from the last snapshot sent, with the levels
    /// (last sent, now) and advertised rates (last sent, now).
    pub diffs: Vec<(
        GoodId,
        rtsim_trade::Level,
        rtsim_trade::Level,
        Amount,
        Amount,
    )>,
    /// Whether `can_pay` or `refuses_cash` changed.
    pub cash_changed: bool,
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
    /// Houses ribbit villages booked (P17).
    pub houses: u64,
    /// News packets touring bands carried (P16).
    pub band_packets: u64,
    /// Stays at which a touring band played for its host (P16).
    pub band_plays: u64,
    /// Works villages bought from their purses (SINK.md).
    pub works: u64,
    /// Works events that ran (bought or rebooked).
    pub works_events: u64,
    /// Bought works the bridge found no place for (MS-11).
    pub works_refused: u64,
    /// Tributes villages paid to dragons' lairs (MS-5).
    pub tributes: u64,
}

/// What the engine asks Minecraft to do (the bridge's command buffer, later).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Write a report into a vanilla village's gossip (C20).
    VanillaGossip(VanillaGossip),
    /// A ribbit village's feelings toward players (the sorcerers' filter, PLAN.md §8.8).
    Sentiment(crate::ribbits::SentimentSnapshot),
    /// Book a house (P17).
    PlaceHouse(crate::ribbits::PlaceHouse),
    /// Spawn a grown house's resident.
    SpawnResident(crate::ribbits::SpawnResident),
    /// Place a bought work (SINK.md §4.4): PLACE_HOUSE with the work's template.
    PlaceWork(crate::works::PlaceWork),
    /// Spawn a bought work's body (a golem): SPAWN_RESIDENT at the meeting point.
    SpawnWork(crate::works::SpawnWork),
}

impl Command {
    /// A bought work's op id, which the bridge answers ([`World::work_result`]).
    pub fn work_op(&self) -> Option<u64> {
        match self {
            Command::PlaceWork(p) => Some(p.op),
            Command::SpawnWork(s) => Some(s.op),
            _ => None,
        }
    }
}

pub struct World {
    pub cfg: WorldConfig,
    pub goods: Goods,
    pub book: RecipeBook,
    pub settlements: Settlements,
    pub nodes: Vec<Node>,
    /// The markets, as lazy regions in row pages (docs/choices-paging-modules.md).
    pub markets: MarketTable,
    pub edges: Vec<Edge>,
    pub knowledge: Knowledge,
    pub(crate) wheel: TimingWheel<Ev>,
    pub packets: BTreeMap<u32, PacketData>,
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
    /// Every market event, when enabled ([`World::enable_trace`]). Diagnostics only: it grows
    /// with time.
    pub trace: Option<Vec<TraceRow>>,
    /// The event archive's recorder (VIEWER.md §10.1, [`crate::archive`]): off until the host
    /// turns it on; not state.
    pub tape: rtsim_archive::Tape,
    /// Each market's price index at its last trade shift row (archive only, not state).
    pub(crate) shift_seen: BTreeMap<usize, f64>,
    /// Players the bridge reported (UUIDs, positions, interest areas).
    pub players: BTreeMap<ActorId, crate::interest::PlayerState>,
    /// Which roads pass near a place.
    pub roads: rtsim_travel::RoadIndex,
    /// Touring bands (P16).
    pub bands: Vec<crate::ribbits::Band>,
    /// Ribbit villages' growth clocks (P17).
    pub growth: BTreeMap<usize, crate::ribbits::Growth>,
    /// Booked houses not placed yet: PLACE_HOUSE op to node.
    pub(crate) houses_pending: BTreeMap<u64, usize>,
    /// Ribbit villages whose feelings may have changed since the last commands.
    pub(crate) sentiment_dirty: std::collections::BTreeSet<usize>,
    /// The engine's operation id counter.
    pub(crate) op_counter: u64,
    /// Player bases' markers (P30).
    pub bases: BTreeMap<usize, crate::bases::BaseState>,
    /// Travellers made individual rows, by key, while their departure could still be sampled.
    pub(crate) promoted: BTreeMap<rtsim_travel::CaravanKey, CaravanId>,
    /// The works catalogue (SINK.md §4.2), sorted by id.
    pub works_catalog: Vec<crate::works::WorkDef>,
    /// Villages' works purses and pointers, by node.
    pub(crate) works: BTreeMap<usize, crate::works::WorksState>,
    /// Bought works the bridge has not answered: op to (node, kind key).
    pub(crate) works_pending: BTreeMap<u64, (usize, u64)>,
}

impl World {
    /// Starts recording every market event in [`World::trace`].
    pub fn enable_trace(&mut self) {
        self.trace.get_or_insert_with(Vec::new);
    }

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
            w.maybe_band(i, Tick::ZERO);
        }
        for i in 0..w.nodes.len() {
            w.emit(i, Tick::ZERO, true);
        }
        // The tables are built: no slack from doubling (overnight choice E58).
        w.edges.shrink_to_fit();
        w.nodes.shrink_to_fit();
        for n in &mut w.nodes {
            n.out.shrink_to_fit();
            n.neighbours.shrink_to_fit();
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
            // Wakes staggered by page over the quantum (docs/choices-paging-followups.md PF1).
            markets: MarketTable::new(
                cfg.seed,
                SYS_MARKET,
                Tick::ZERO,
                TABLE_MARKETS,
                MARKETS_PER_PAGE,
            )
            .staggered(window),
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
            trace: None,
            tape: rtsim_archive::Tape::new(),
            shift_seen: BTreeMap::new(),
            players: BTreeMap::new(),
            roads: rtsim_travel::RoadIndex::new(),
            bands: Vec::new(),
            growth: BTreeMap::new(),
            houses_pending: BTreeMap::new(),
            sentiment_dirty: std::collections::BTreeSet::new(),
            op_counter: 0,
            bases: BTreeMap::new(),
            promoted: BTreeMap::new(),
            works_catalog: Vec::new(),
            works: BTreeMap::new(),
            works_pending: BTreeMap::new(),
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
            bands: Vec::new(),
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
        let dock = self.cfg.trade.dock_wait_quanta;
        // Scouts never wait longer at a dock than goods do.
        let idle = self
            .cfg
            .trade
            .quantum
            .days(self.cfg.scout_days)
            .min(dock.max(1));
        let mut edge = Edge::new(a, b, length, d, dock, seed, border, idle);
        // Messengers carry news on their own schedule, at their own pace (PR6).
        let msg_speed = self.cfg.caravan_blocks_per_tick
            * f64::from(self.cfg.messenger_speed_milli.max(1))
            / 1000.0;
        let walk = self
            .cfg
            .trade
            .quantum
            .covering((length as f64 / msg_speed).ceil() as u64);
        edge.set_messengers(self.cfg.messenger_quanta, walk);
        self.edges.push(edge);
        let (pa, pb) = (self.nodes[a].pos, self.nodes[b].pos);
        if pa.frame == pb.frame {
            self.roads.add_on(
                self.legs.topo(pa.frame),
                e,
                pa.frame,
                (i64::from(pa.x), i64::from(pa.z)),
                (i64::from(pb.x), i64::from(pb.z)),
                i64::from(self.cfg.view_blocks),
            );
        }
        self.nodes[a].out.push(DirRef { edge: e, dir: 0 });
        self.nodes[b].out.push(DirRef { edge: e, dir: 1 });
        self.nodes[a].neighbours.push(b);
        self.nodes[b].neighbours.push(a);
    }

    /// A village joins when its structure start generates (C22): its market is built from its own
    /// recipes and its new neighbours' costs, and it links to the world by the edge rule. Existing
    /// markets keep their anchors (overnight choice E15); their `T` bound does not move either, so
    /// a longer new edge than they had is a known gap until anchors are recomputed.
    pub fn join(&mut self, mut d: NodeData, at: Tick) -> usize {
        self.run_until(at);
        // Where it is on the world (topology choice TP4), so its edges and its key are one place's.
        d.pos = self.canonical_pos(d.pos);
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
        if !matches!(d.kind, NodeKind::PlayerBase { .. }) {
            let (place, id) = (self.node_at(i, at), self.node_id(i));
            self.tape.event(
                rtsim_archive::kinds::SETTLE_JOINED,
                place,
                id,
                0,
                i64::from(d.pop),
            );
        }
        for &(j, len, dq, border) in &links {
            self.add_edge(i, j, len, dq, border);
            if border {
                self.open_exchange(j, i, dq, at);
            }
        }
        // A ribbit village with ribbit neighbours gets a band; so may a neighbour that had none.
        self.maybe_band(i, at);
        for j in self.nodes[i].neighbours.clone() {
            self.maybe_band(j, at);
        }
        self.emit(i, at, true);
        for j in self.nodes[i].neighbours.clone() {
            self.emit(j, at, true);
        }
        i
    }

    /// Node `j` just gained a border edge to `newcomer` (C22): if it kept no exchange line for
    /// the newcomer's currency, it opens one now, at mint parity within the gold points of this
    /// edge, as a border market built at once would have (overnight choice E54). Without it the
    /// foreign coin it is paid in would sink at once (its line's target is zero).
    pub(crate) fn open_exchange(&mut self, j: usize, newcomer: usize, d: u64, at: Tick) {
        let (Some(their), Some(mine)) = (
            self.nodes[newcomer].kind.culture(),
            self.nodes[j].kind.culture(),
        ) else {
            return;
        };
        let Some(g) = self.goods.currency(their) else {
            return;
        };
        self.release(j, at);
        let region = self.nodes[j].region;
        let l_mint = self.mint_cost[mine.index()];
        let (goods, tun, mint_cost) = (&self.goods, &self.cfg.trade, self.mint_cost);
        self.markets.apply(region, at, |m, _| {
            if m.line(g).target > 0 || m.currency.is_none() {
                return;
            }
            let mbar = m.line(m.currency.expect("checked")).target;
            let (target, curve) = rtsim_trade::build::exchange_line(
                goods,
                tun,
                g,
                their,
                l_mint,
                mint_cost,
                mbar,
                Some(d),
            );
            m.line_mut(g).target = target;
            m.goods[g.index()].curve = curve;
            m.recompute();
        });
        // Its foreign coin now sinks at a new ceiling and counts at a new parity.
        self.works_touch(j, at);
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
        let dist = self
            .pos_dist(self.nodes[from].pos, self.nodes[haven].pos)
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

    /// Entries in the world's own wheel (deliveries, wakes, arrivals, growth checks), for
    /// diagnostics.
    pub fn world_wheel_len(&self) -> usize {
        self.wheel.len()
    }

    pub fn node_of_key(&self, key: u64) -> Option<usize> {
        self.nodes.iter().position(|n| n.key == key)
    }

    /// Market `i` as [`World::market`] reads it, or `None` when there is no such node or its page
    /// is not resident inside the kernel's resident-only reads (`rtsim_kernel::rows::resident_only`,
    /// the viewer's tap), where [`World::market`] would panic.
    pub fn try_market(&self, i: usize) -> Option<Market> {
        let r = self.markets.get(self.nodes.get(i)?.region)?;
        Some(r.cell().view_without_catch_up(self.now))
    }

    /// The market at node `i` as it stands at the world's time (a projection; changes nothing).
    pub fn market(&self, i: usize) -> Market {
        let r = self
            .markets
            .get(self.nodes[i].region)
            .expect("every node has a market");
        r.cell().view_without_catch_up(self.now)
    }

    /// When market `i` acts on work due at the boundary `at`: its page's phase later, inside the
    /// same quantum (docs/choices-paging-followups.md PF1). Markets decide per quantum, so the
    /// phase moves no decision; it spreads the markets' boundary work, and so their changed
    /// pages, over the quantum.
    pub(crate) fn phased(&self, at: Tick, i: usize) -> Tick {
        at + self.markets.phase(self.nodes[i].region)
    }

    /// Delivers market `i`'s breakpoint now if it is due and only its page's phase holds it back
    /// (PF1), so that anything else reaching the market at `at` finds it acted on, as it would
    /// have at the boundary. Call it before anything acts on or reads market `i` at `at`.
    pub(crate) fn release(&mut self, i: usize, at: Tick) {
        if self.markets.take_held(self.nodes[i].region, at) {
            self.stats.breakpoints += 1;
            self.market_event(i, at, Cause::Breakpoint);
        }
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
                self.market_event(i, at, Cause::Breakpoint);
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
        // The leg index every module books in is the world's: its past windows' stale bookings go
        // once a game day (docs/choices-leg-purge.md LP1, LP2), exactly, so at any stride.
        self.legs.purge_due(self.now);
        self.markets.rows_mut().enforce_budget();
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
                self.market_event(i, at, Cause::Wake);
            }
            Ev::Deliver(p) => self.deliver(p, at),
            Ev::Arrive {
                caravan,
                generation,
            } => self.caravan_arrives(caravan, generation, at),
            Ev::Grow { node, stamp } => self.grow(node, stamp, at),
            Ev::BandPlays { band, stop, stamp } => {
                self.band_plays(band as usize, stop as usize, stamp, at)
            }
            Ev::Works { node, stamp } => self.works_due(node, stamp, at),
        }
    }

    /// Commands for the bridge: vanilla gossip writes and the like, oldest first.
    pub fn take_commands(&mut self) -> Vec<Command> {
        self.flush_sentiments();
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
