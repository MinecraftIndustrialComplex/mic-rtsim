// SPDX-License-Identifier: AGPL-3.0-or-later
//! One district's state in the engine (INDUSTRY.md §2 to §5): its boundary inventories, what the
//! engine knows they hold, its learner and models, its replay span, and its reconcile book.
//!
//! **Known and pending.** Each inventory carries `known` (its last complete sweep sample) and
//! `pending` (reconcile deltas sent but not yet reported applied). Its effective contents are the
//! sum. A district's replay starts from the sum of its inventories' effective contents (its
//! `base`), so the deltas at the next load are `replay level - base`, whatever happened in between.
//! Changes the world itself made while the district was unloaded (packages a train dropped into
//! the offline buffer, packages it took away, packages the engine wrote) move `known` and `base`
//! together, so they are never applied twice.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::conserve::Diffuser;
use rtsim_kernel::journal::OpId;
use rtsim_space::Topologies;
use serde::Serialize;

use crate::cluster::{MemberSpec, PoolKey, ProcSpec, ProcSpecBody};
use crate::cmd::{BlockDestroy, ChunkReconcile};
use crate::ids::{ChunkPos, DistrictId, Good, GoodClass, GoodVec, SLOT_UNITS, StationId, VecMap};
use crate::learn::{Learned, LearnedTarget, Learner, LinkKey};
use crate::model::{BlackBox, DistrictModel, ModelProcess};
use crate::obs::{DepositSample, InvKind, InventoryDecl, NotLearnable};
use crate::reconcile::{GoodLookup, InvState};
use crate::threat::{Defence, Outcome, ThreatClass, ThreatSource};
use crate::tunables::Tunables;

/// Replay process ids: a district's box processes (its learned coupling groups, I6) take 0, 1, 2
/// ...; its link processes (packagers) start here.
pub const LINK_PROC_BASE: u16 = 0x100;

/// The inventory index used for deposit deltas inside a chunk reconcile.
pub const DEPOSIT_INVENTORY: u32 = u32::MAX;

/// Create's `GlobalPackagePort.offlineBuffer` holds 18 packages (verified, INDUSTRY.md §0).
pub const OFFLINE_BUFFER_PACKAGES: i64 = 18;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inventory {
    pub decl: InventoryDecl,
    pub chunk: ChunkPos,
    pub known: GoodVec,
    pub known_at: Tick,
    pub pending: GoodVec,
    /// Deltas a partial reconcile planned for this inventory while it stayed in the replay (I7):
    /// not sent yet, counted in its contents, sent when its chunk leaves the replay.
    pub held: GoodVec,
    pub received: BTreeSet<Good>,
    /// An attack will break this block at the next load (I23): it still takes its reconcile
    /// deltas, then everything in it drops. The replay no longer counts its room.
    pub doomed: bool,
    /// The block is gone (Java reported the destroy).
    pub destroyed: bool,
}

impl Inventory {
    pub fn effective(&self) -> GoodVec {
        let mut v = self.known.clone();
        v.merge(&self.pending);
        v.merge(&self.held);
        v
    }

    pub fn state(&self) -> InvState {
        InvState {
            index: self.decl.index,
            pos: self.decl.pos,
            chunk: self.chunk,
            kind: self.decl.kind,
            item_slots: self.decl.item_slots,
            fluid_capacity: self.decl.fluid_capacity,
            energy_capacity: self.decl.energy_capacity,
            contents: self.effective(),
            received: self.received.clone(),
        }
    }
}

/// How far the district is in learning (INDUSTRY.md §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase {
    Learning,
    Learned,
    /// A player edited it since it was learned: frozen while unloaded (I5).
    Stale,
    NotLearnable(u32),
    /// An attack destroyed a machine: stopped until reloaded and relearned (I23).
    Damaged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Presence {
    Unloaded,
    /// Some chunks tick (I7: suspended).
    Partial,
    Live,
}

/// A learned box model for one season (I6, I12), with its fixed-point rates.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Model {
    pub learned: Learned,
    pub targets: Vec<LearnedTarget>,
    #[serde(skip)]
    pub rates: Vec<(Good, Diffuser)>,
    #[serde(skip)]
    pub links: Vec<(LinkKey, Diffuser)>,
    /// One package's contents per (postbox, address), from observed mail.
    pub batches: BTreeMap<(u32, u32), GoodVec>,
    pub season: u32,
}

impl Model {
    pub fn from_learned(
        learned: Learned,
        targets: Vec<LearnedTarget>,
        season: u32,
        batches: BTreeMap<(u32, u32), GoodVec>,
    ) -> Model {
        let rates = learned
            .rates
            .iter()
            .map(|&(g, r)| (g, Diffuser::from_f64_per_tick(r)))
            .filter(|(_, d)| d.rate_fixed() != 0)
            .collect();
        let links = learned
            .links
            .iter()
            .map(|&(k, r)| (k, Diffuser::from_f64_per_tick(r)))
            .filter(|(_, d)| d.rate_fixed() != 0)
            .collect();
        Model {
            learned,
            targets,
            rates,
            links,
            batches,
            season,
        }
    }

    pub fn produced(&self) -> impl Iterator<Item = Good> + '_ {
        self.rates
            .iter()
            .filter(|r| r.1.rate_fixed() > 0)
            .map(|r| r.0)
    }
}

/// One sweep being assembled (docs/industry-boundary.md §3.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepBuf {
    pub sweep: u32,
    pub records: u32,
    pub contents: BTreeMap<u32, GoodVec>,
    /// When each inventory was read: its first record's game time, not the sweep's end (a delta
    /// that landed in between is in the next sweep, not this one; review finding 13).
    pub read_at: BTreeMap<u32, Tick>,
    pub deposits: Vec<DepositSample>,
}

/// A replay in progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SimSpan {
    #[serde(serialize_with = "crate::ser::tick")]
    pub started: Tick,
    /// Stock pools' levels at the start, adjusted for physical changes since.
    pub base: GoodVec,
    pub season: u32,
    /// The inventories the replay covers (I7): those whose chunks were unloaded at its start.
    pub invs: BTreeSet<u32>,
    /// A glass replay (GLASSBOX.md §4): it books per store (`PoolKey::Store`); `base` is the
    /// stores' total at the start. Its `Stock` pools hold the cut's supply (I7: what the live
    /// part holds and the district owes, when goods cross from it) and the events (imports wait
    /// there, G2-14); their change from their start is owed.
    pub glass: bool,
}

/// A chunk's reconcile waiting for the chunk and then for Java's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingChunk {
    pub cmd: ChunkReconcile,
    /// Removals to trim outputs already applied (their shortfalls are untracked mints).
    pub trim: bool,
    /// Sent to Java: it may be applied at any moment, so it is never edited again (a trim makes
    /// a new epoch instead; review finding 3).
    pub sent: bool,
}

/// What one reconcile round did (the ledger's "last reconcile").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    pub epoch: u32,
    #[serde(serialize_with = "crate::ser::tick")]
    pub at: Tick,
    #[serde(serialize_with = "crate::ser::tick")]
    pub since: Tick,
    /// The district's net change per good while unloaded.
    pub net: GoodVec,
    pub chunks: u32,
    pub deltas: u32,
    pub unplaced: GoodVec,
    pub shortfalls: GoodVec,
    pub trims: GoodVec,
    pub untracked: GoodVec,
    pub destroyed: u32,
}

/// One attack's record for the ledger (INDUSTRY.md §10).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AttackReport {
    #[serde(serialize_with = "crate::ser::tick")]
    pub at: Tick,
    pub source: ThreatSource,
    pub class: ThreatClass,
    pub count: u32,
    pub outcome: Option<Outcome>,
    pub survivors: u32,
    pub stolen: GoodVec,
    pub destroyed: u32,
    pub machine_lost: bool,
    /// Why nothing happened: "cooldown", "watched".
    pub skipped: Option<&'static str>,
}

/// The emulated station exchange (I15): what this station unloads and loads.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ExchangeRule {
    pub unload: BTreeSet<Good>,
    pub load: BTreeSet<Good>,
    pub observed: u32,
    /// The inventories the station's interfaces serve (IND_STATION_PORTS, choice SG32): a
    /// simulated call books against these only, and moves nothing while there are none.
    pub ports: Vec<u32>,
    /// Per good, what loaded calls moved in its current direction: the total, the calls and the
    /// most one call moved (choice SG34). A simulated load takes at most that largest call.
    pub per_call: BTreeMap<Good, (i64, u32, i64)>,
}

impl ExchangeRule {
    /// The most a simulated call loads of `g`: the largest amount one real call moved (SG34).
    /// Not the mean: early calls find little made yet, so a mean sits below what the line moves
    /// once it runs, and a capped line then backs up.
    pub fn load_cap(&self, g: Good) -> i64 {
        self.per_call.get(&g).map_or(0, |e| e.2.max(0))
    }

    /// One loaded call's change of `g` (positive: unloaded into the district). Called before
    /// the unload and load sets take the call, so a change of direction starts afresh.
    pub fn learn_amount(&mut self, g: Good, n: i64) {
        let unload = n > 0;
        let same = if unload {
            self.unload.contains(&g)
        } else {
            self.load.contains(&g)
        };
        let e = self.per_call.entry(g).or_insert((0, 0, 0));
        if !same {
            *e = (0, 0, 0);
        }
        let moved = n.saturating_abs();
        e.0 = e.0.saturating_add(moved);
        e.1 = e.1.saturating_add(1);
        e.2 = e.2.max(moved);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct District {
    pub id: DistrictId,
    pub revision: u32,
    pub dim: u32,
    pub name: u32,
    pub not_learnable: Option<NotLearnable>,
    pub chunks: BTreeSet<(i32, i32)>,
    pub inventories: VecMap<u32, Inventory>,
    pub presence: Presence,
    /// A player is near enough to fight for real (IND_WATCHED); never set while unloaded.
    pub watched: bool,
    pub phase: Phase,
    pub learner: Learner,
    pub models: VecMap<u32, Model>,
    /// Cumulative port imports, any origin (learning uses differences).
    pub ports: GoodVec,
    pub links: BTreeMap<LinkKey, i64>,
    pub sweep: Option<SweepBuf>,
    pub deposits: VecMap<Good, DepositSample>,
    pub sim: Option<SimSpan>,
    pub epoch: u32,
    pub pending: BTreeMap<ChunkPos, Vec<PendingChunk>>,
    pub pending_ops: BTreeMap<OpId, ChunkPos>,
    /// What the replay made in the span each epoch reconciled, which trims are capped by.
    pub round_added: BTreeMap<u32, GoodVec>,
    /// Shortfalls reported so far per epoch, and the trims made for them: several results of one
    /// epoch trim the largest need once, not the sum (review finding 9).
    pub round_short: BTreeMap<u32, (GoodVec, GoodVec)>,
    /// Changes to apply at the next reconcile: additions that did not fit (positive), theft and
    /// corrections while frozen (negative).
    pub owed: GoodVec,
    /// Deposit depletion carried across a replay restart, written back at the next reconcile.
    pub deposit_owed: GoodVec,
    /// What the replay minted in spans ended by a restart, for the next reconcile's trim cap.
    pub minted_carried: GoodVec,
    pub untracked: GoodVec,
    pub loaded_chunks: BTreeSet<ChunkPos>,
    pub destroy_pending: BTreeMap<ChunkPos, Vec<BlockDestroy>>,
    pub reports: VecDeque<ReconcileReport>,
    pub attacks: VecDeque<AttackReport>,
    pub defence: Defence,
    pub pressure_stamp: u32,
    pub stations: BTreeSet<StationId>,
    pub exchange: BTreeMap<StationId, ExchangeRule>,
    pub power_source: Option<DistrictId>,
    /// One package's contents per (postbox, address), from the latest mail seen leaving.
    pub batches: BTreeMap<(u32, u32), GoodVec>,
    /// What the replay made (positive) and used (negative) over all spans.
    pub made_total: GoodVec,
    /// The last time the box's run state changed, and why it stopped (`None`: it started).
    pub last_run: Option<(Tick, Option<crate::cluster::Stop>)>,
    /// The latest IND_APPROACH (W16).
    pub approach: Option<Approach>,
    /// A player is on the way (IND_APPROACH, closing): the replay stopped and was reconciled
    /// ahead of arrival, and does not start again before this tick (the stale-tick answer).
    pub hold_until: Option<Tick>,
    /// Makes an older hold's wake stale.
    pub hold_stamp: u32,
    /// The players who built here lately (IND_EDIT: a block placed or broken, or a machine
    /// configured), each with its latest such edit, the latest last; at most [`EDITORS_KEPT`].
    /// The oil gate's dragons name them (choices-oilgate.md OGC54).
    pub editors: Vec<((u64, u64), Tick)>,
}

/// Builders a district keeps (OGC54). PLACEHOLDER.
pub const EDITORS_KEPT: usize = 8;

/// A district page starts with this (OGC54: "IDS2" is the first format with one; before it a
/// district page had no magic and no editors; "IDS3" adds the stations' ports and per-call
/// amounts, choices SG32 and SG34). No older page opens.
pub const DISTRICT_MAGIC: u32 = u32::from_le_bytes(*b"IDS3");

/// A player heading for an unloaded district, as the Java side last reported it (IND_APPROACH).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Approach {
    #[serde(serialize_with = "crate::ser::tick")]
    pub at: Tick,
    pub distance: u32,
    pub eta_ticks: Option<u32>,
    pub closing: bool,
}

impl District {
    pub fn new(id: DistrictId) -> Self {
        Self {
            id,
            revision: 0,
            dim: 0,
            name: 0,
            not_learnable: None,
            chunks: BTreeSet::new(),
            inventories: VecMap::new(),
            presence: Presence::Unloaded,
            watched: false,
            phase: Phase::Learning,
            learner: Learner::new(),
            models: VecMap::new(),
            ports: GoodVec::new(),
            links: BTreeMap::new(),
            sweep: None,
            deposits: VecMap::new(),
            sim: None,
            epoch: 0,
            pending: BTreeMap::new(),
            pending_ops: BTreeMap::new(),
            round_added: BTreeMap::new(),
            round_short: BTreeMap::new(),
            owed: GoodVec::new(),
            deposit_owed: GoodVec::new(),
            minted_carried: GoodVec::new(),
            untracked: GoodVec::new(),
            loaded_chunks: BTreeSet::new(),
            destroy_pending: BTreeMap::new(),
            reports: VecDeque::new(),
            attacks: VecDeque::new(),
            defence: Defence::default(),
            pressure_stamp: 0,
            stations: BTreeSet::new(),
            exchange: BTreeMap::new(),
            power_source: None,
            batches: BTreeMap::new(),
            made_total: GoodVec::new(),
            last_run: None,
            approach: None,
            hold_until: None,
            hold_stamp: 0,
            editors: Vec::new(),
        }
    }

    /// Boundary stock as the engine knows it: every live inventory's effective contents, plus
    /// finite deposits.
    pub fn effective_stock(&self) -> GoodVec {
        let mut s = GoodVec::new();
        for inv in self.inventories.values().filter(|i| !i.destroyed) {
            s.merge(&inv.effective());
        }
        for d in self.deposits.values().filter(|d| !d.infinite) {
            s.add(d.good, d.remaining);
        }
        s.merge(&self.owed);
        s.merge(&self.deposit_owed);
        s
    }

    pub fn cumulative_links(&self) -> Vec<(LinkKey, i64)> {
        self.links.iter().map(|(&k, &v)| (k, v)).collect()
    }

    /// Frozen while unloaded, and why (I5): `None` means it may run.
    pub fn freeze_reason(&self, season: u32) -> Option<&'static str> {
        match self.phase {
            Phase::Learning => Some("learning"),
            Phase::Stale => Some("edited since it was learned"),
            Phase::NotLearnable(code) => {
                Some(NotLearnable::from_code(code).map_or("not learnable", |n| n.words()))
            }
            Phase::Damaged => Some("damaged: a machine was destroyed"),
            Phase::Learned if !self.models.contains_key(&season) => {
                Some("no model for this season")
            }
            Phase::Learned => None,
        }
    }

    pub fn push_report(&mut self, r: ReconcileReport) {
        self.reports.push_back(r);
        // TUNABLE, PLACEHOLDER: the ledger keeps the last 8 reconciles and 16 attacks.
        if self.reports.len() > 8 {
            self.reports.pop_front();
        }
    }

    /// `player` built here at `at` (OGC54): it moves to the end of [`District::editors`], and the
    /// longest quiet one goes past [`EDITORS_KEPT`].
    pub fn note_editor(&mut self, player: (u64, u64), at: Tick) {
        self.editors.retain(|(p, _)| *p != player);
        self.editors.push((player, at));
        if self.editors.len() > EDITORS_KEPT {
            self.editors.remove(0);
        }
    }

    pub fn push_attack(&mut self, a: AttackReport) {
        self.attacks.push_back(a);
        if self.attacks.len() > 16 {
            self.attacks.pop_front();
        }
    }

    /// The district's centre in blocks: the average of its chunk centres
    /// `(cx * 16 + 8, cz * 16 + 8)`, taken through the frame's topology in the chart of its first
    /// chunk so a district across a seam is not averaged to the far side (the same arithmetic as
    /// rtsim-ecology's `DistrictIndex::refresh`). `None` while it has no chunks. The event archive
    /// places the district's rows here.
    pub fn centre(&self, topo: &Topologies) -> Option<(i32, i32)> {
        let t = topo.of(self.dim as u16);
        let first = self.chunks.iter().next()?;
        let origin = (i64::from(first.0) * 16 + 8, i64::from(first.1) * 16 + 8);
        let (sx, sz) = self.chunks.iter().fold((0i64, 0i64), |a, c| {
            let p = (i64::from(c.0) * 16 + 8, i64::from(c.1) * 16 + 8);
            let q = t.toward(origin, p);
            (a.0 + (q.0 - 8) / 16, a.1 + (q.1 - 8) / 16)
        });
        let k = self.chunks.len() as i64;
        let (x, z) = t.canonical(sx * 16 / k + 8, sz * 16 / k + 8);
        Some((x as i32, z as i32))
    }

    /// The inventories the replay covers now (I7): every live one whose chunk is not loaded.
    pub fn unloaded_inventories(&self) -> BTreeSet<u32> {
        self.inventories
            .values()
            .filter(|i| !i.destroyed && !self.loaded_chunks.contains(&i.chunk))
            .map(|i| i.decl.index)
            .collect()
    }

    /// Whether `part` is the whole district: every live inventory and every deposit unloaded.
    pub fn is_whole(&self, part: &BTreeSet<u32>) -> bool {
        self.inventories
            .values()
            .filter(|i| !i.destroyed)
            .all(|i| part.contains(&i.decl.index))
            && self
                .deposits
                .values()
                .all(|s| !self.loaded_chunks.contains(&s.chunk))
    }

    /// Stock the replay of `part` starts from: those inventories' effective contents, the
    /// unloaded finite deposits, and the virtual stock the district owes (I7: the change a partial
    /// reconcile left to the part still simulated rides there).
    pub fn stock_of(&self, part: &BTreeSet<u32>) -> GoodVec {
        let mut s = GoodVec::new();
        for i in part {
            if let Some(inv) = self.inventories.get(i).filter(|x| !x.destroyed) {
                s.merge(&inv.effective());
            }
        }
        for dep in self
            .deposits
            .values()
            .filter(|x| !x.infinite && !self.loaded_chunks.contains(&x.chunk))
        {
            s.add(dep.good, dep.remaining);
        }
        s.merge(&self.owed);
        s.merge(&self.deposit_owed);
        s
    }

    /// Whether good `g` lives only in `part` (I7): its deposit is unloaded, or every live
    /// inventory that received or holds it is in the part. A good that lives nowhere known does
    /// not (the safe side: its process waits for the whole district).
    pub fn lives_in(&self, g: Good, part: &BTreeSet<u32>) -> bool {
        if let Some(dep) = self.deposits.get(&g) {
            return !self.loaded_chunks.contains(&dep.chunk);
        }
        let mut homes = self
            .inventories
            .values()
            .filter(|i| !i.destroyed && (i.received.contains(&g) || i.effective().get(g) > 0))
            .peekable();
        homes.peek().is_some() && homes.all(|i| part.contains(&i.decl.index))
    }

    /// The replay member for this district under `model`, over the inventories of `part`,
    /// starting from `base` (I6, I7, I13). `None` when no process may run there.
    ///
    /// - **Processes** are the model's ([`DistrictModel`]): the black box's coupling groups.
    /// - **Placement (I7):** when only part of the district is unloaded, a process runs in the
    ///   replay only if every good it touches lives in that part. One that reaches a loaded
    ///   chunk is the boundary's: it may run for real there, and never in both places, so it does
    ///   not run in the replay (never better than reality). Its goods' pools keep their stock.
    /// - **Pools, caps and space** count only the part's inventories.
    pub fn member_spec(
        &self,
        model: &Model,
        base: &GoodVec,
        part: &BTreeSet<u32>,
        goods: &impl GoodLookup,
        tun: &Tunables,
    ) -> Option<MemberSpec> {
        let d = self.id;
        let whole = self.is_whole(part);
        let invs: Vec<&Inventory> = self
            .inventories
            .values()
            .filter(|i| !i.destroyed && !i.doomed && part.contains(&i.decl.index))
            .collect();
        let black = BlackBox {
            model,
            coupling: &self.learner.coupling,
            min_episodes: tun.learn.coupling_min_episodes,
        };
        let processes: Vec<ModelProcess> = black
            .processes()
            .into_iter()
            .filter(|p| whole || p.goods().all(|g| self.lives_in(g, part)))
            .collect();
        let produced: BTreeSet<Good> = processes
            .iter()
            .flat_map(|p| p.terms.iter().filter(|t| t.1.rate_fixed() > 0).map(|t| t.0))
            .collect();
        let mut pools: Vec<(PoolKey, i64, Option<i64>)> = Vec::new();
        let mut goods_here: BTreeSet<Good> = base.goods().collect();
        goods_here.extend(model.rates.iter().map(|r| r.0));
        for &g in &goods_here {
            let cap = match goods.class(g) {
                GoodClass::Fluid if produced.contains(&g) => {
                    let c: i64 = invs
                        .iter()
                        .filter(|i| {
                            i.decl.kind.has(InvKind::FLUIDS)
                                && (i.received.contains(&g) || i.known.get(g) > 0)
                        })
                        .map(|i| i.decl.fluid_capacity)
                        .sum();
                    Some(c.max(base.get(g)))
                }
                GoodClass::Energy if produced.contains(&g) => {
                    let c: i64 = invs
                        .iter()
                        .filter(|i| i.decl.kind.has(InvKind::ENERGY))
                        .map(|i| i.decl.energy_capacity)
                        .sum();
                    Some(c.max(base.get(g)))
                }
                _ => None,
            };
            pools.push((PoolKey::Stock(d, g), base.get(g).max(0), cap));
        }
        // An output whose every receiver was destroyed has nowhere to go: its process jams (F20).
        let jammed = |g: &Good| {
            let mut rec = self
                .inventories
                .values()
                .filter(|i| i.received.contains(g))
                .peekable();
            rec.peek().is_some() && rec.all(|i| i.destroyed || i.doomed)
        };
        let mut procs: Vec<ProcSpec> = Vec::new();
        for (id, p) in processes.iter().enumerate() {
            let dead = p.terms.iter().any(|t| {
                t.1.rate_fixed() > 0 && goods.class(t.0) == GoodClass::Item && jammed(&t.0)
            });
            procs.push(ProcSpec {
                id: id as u16,
                body: ProcSpecBody::Box(
                    p.terms
                        .iter()
                        .map(|&(g, f)| (PoolKey::Stock(d, g), f))
                        .collect(),
                ),
                targets: p
                    .targets
                    .iter()
                    .map(|&(g, stop, restart)| (PoolKey::Stock(d, g), stop, restart))
                    .collect(),
                requires: self.power_source,
                latency: p.latency,
                // A jammed line does not run at all, whatever its net use of space (F20).
                enabled: !dead,
                deadline: None,
                min_run: 0,
            });
        }
        let mut next_id = LINK_PROC_BASE;
        for &(k, flow) in &model.links {
            let rate = flow.rate_fixed();
            let unit = Diffuser::from_fixed(rate.abs());
            match k {
                LinkKey::Mail {
                    inventory,
                    address,
                    good,
                } if rate < 0 && part.contains(&inventory) => {
                    let batch = model
                        .batches
                        .get(&(inventory, address))
                        .map(|b| b.get(good))
                        .filter(|&b| b > 0)
                        .unwrap_or_else(|| goods.max_stack(good));
                    let mk = PoolKey::Mail(d, inventory, address, good);
                    // Room in the 18-slot buffer for packages not already there; the buffer's
                    // contents count as whole packages of this batch (review finding 10).
                    let held: i64 = self
                        .inventories
                        .get(&inventory)
                        .map_or(0, |i| i.effective().iter().map(|(_, n)| n.max(0)).sum());
                    let packages_there = (held + batch - 1) / batch;
                    let room = (OFFLINE_BUFFER_PACKAGES - packages_there).max(0);
                    pools.push((mk, 0, Some(room * batch)));
                    procs.push(ProcSpec {
                        id: next_id,
                        body: ProcSpecBody::Link {
                            from: PoolKey::Stock(d, good),
                            to: mk,
                            flow: unit,
                            batch: Some(batch),
                        },
                        targets: vec![],
                        requires: None,
                        latency: 0,
                        enabled: true,
                        deadline: None,
                        min_run: 0,
                    });
                    next_id += 1;
                }
                LinkKey::Mail { .. } => {}
            }
        }
        if procs.is_empty() {
            return None;
        }
        // The shared item space of the part's inventories that received outputs (I6). Every item
        // good held there counts, so imports fill it and consumption frees it; a good also held
        // elsewhere is over-counted, which only stops the replay early (F12).
        let produced_items: Vec<Good> = produced
            .iter()
            .copied()
            .filter(|&g| goods.class(g) == GoodClass::Item)
            .collect();
        let space = if produced_items.is_empty() {
            None
        } else {
            let mut pool_invs: Vec<&Inventory> = invs
                .iter()
                .copied()
                .filter(|i| {
                    i.decl.kind.has(InvKind::ITEMS)
                        && produced_items.iter().any(|g| i.received.contains(g))
                })
                .collect();
            if pool_invs.is_empty() {
                pool_invs = invs
                    .iter()
                    .copied()
                    .filter(|i| i.decl.kind.has(InvKind::ITEMS))
                    .collect();
            }
            let slots: i64 = pool_invs.iter().map(|i| i64::from(i.decl.item_slots)).sum();
            let margin = tun.replay.fragment_margin_slots
                * produced_items.len() as i64
                * pool_invs.len() as i64;
            let cap = (slots - margin).max(0) * SLOT_UNITS;
            let mut used: i64 = 0;
            let mut counted: BTreeSet<Good> = produced_items.iter().copied().collect();
            for i in &pool_invs {
                for (g, c) in i.effective().iter() {
                    if goods.class(g) == GoodClass::Item && c > 0 {
                        let ms = goods.max_stack(g);
                        used += (c + ms - 1) / ms * SLOT_UNITS;
                        counted.insert(g);
                    }
                }
            }
            let weights: Vec<(PoolKey, i64)> = counted
                .iter()
                .map(|&g| (PoolKey::Stock(d, g), weight(goods, g)))
                .collect();
            let fixed = used
                - weights
                    .iter()
                    .map(|(k, w)| w * base.get(k.good()).max(0))
                    .sum::<i64>();
            Some((weights, fixed, cap))
        };
        Some(MemberSpec {
            district: d,
            pools,
            procs,
            space,
            bounds: Vec::new(),
        })
    }
}

fn weight(goods: &impl GoodLookup, g: Good) -> i64 {
    let ms = goods.max_stack(g);
    (SLOT_UNITS + ms - 1) / ms
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    persist_struct!(Approach {
        at,
        distance,
        eta_ticks,
        closing
    });

    persist_struct!(Inventory {
        decl,
        chunk,
        known,
        known_at,
        pending,
        held,
        received,
        doomed,
        destroyed
    });
    persist_unit_enum!(
        Presence,
        [Presence::Unloaded, Presence::Partial, Presence::Live]
    );

    impl Persist for Phase {
        fn put(&self, w: &mut Writer) {
            match *self {
                Phase::Learning => w.u8(0),
                Phase::Learned => w.u8(1),
                Phase::Stale => w.u8(2),
                Phase::NotLearnable(c) => {
                    w.u8(3);
                    c.put(w);
                }
                Phase::Damaged => w.u8(4),
            }
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(match r.u8()? {
                0 => Phase::Learning,
                1 => Phase::Learned,
                2 => Phase::Stale,
                3 => Phase::NotLearnable(Persist::get(r)?),
                4 => Phase::Damaged,
                _ => return invalid("phase"),
            })
        }
    }

    persist_struct!(Model {
        learned,
        targets,
        rates,
        links,
        batches,
        season
    });
    persist_struct!(SimSpan {
        started,
        base,
        season,
        invs,
        glass
    });
    persist_struct!(PendingChunk { cmd, trim, sent });
    persist_struct!(ReconcileReport {
        epoch,
        at,
        since,
        net,
        chunks,
        deltas,
        unplaced,
        shortfalls,
        trims,
        untracked,
        destroyed,
    });
    persist_struct!(ExchangeRule {
        unload,
        load,
        observed,
        ports,
        per_call
    });
    persist_struct!(SweepBuf {
        sweep,
        records,
        contents,
        read_at,
        deposits
    });

    /// `skipped` is one of a few fixed words.
    const SKIPS: [&str; 2] = ["watched", "cooldown"];

    impl Persist for AttackReport {
        fn put(&self, w: &mut Writer) {
            self.at.put(w);
            self.source.put(w);
            self.class.put(w);
            self.count.put(w);
            self.outcome.put(w);
            self.survivors.put(w);
            self.stolen.put(w);
            self.destroyed.put(w);
            self.machine_lost.put(w);
            let code = self.skipped.map(|s| {
                SKIPS
                    .iter()
                    .position(|x| *x == s)
                    .map_or(u8::MAX, |i| i as u8)
            });
            code.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            Ok(AttackReport {
                at: Persist::get(r)?,
                source: Persist::get(r)?,
                class: Persist::get(r)?,
                count: Persist::get(r)?,
                outcome: Persist::get(r)?,
                survivors: Persist::get(r)?,
                stolen: Persist::get(r)?,
                destroyed: Persist::get(r)?,
                machine_lost: Persist::get(r)?,
                skipped: Option::<u8>::get(r)?
                    .map(|c| SKIPS.get(c as usize).copied().unwrap_or("skipped")),
            })
        }
    }

    impl Persist for District {
        fn put(&self, w: &mut Writer) {
            w.u32(DISTRICT_MAGIC);
            self.id.put(w);
            self.revision.put(w);
            self.dim.put(w);
            self.name.put(w);
            self.not_learnable.put(w);
            self.chunks.put(w);
            self.inventories.put(w);
            self.presence.put(w);
            self.watched.put(w);
            self.phase.put(w);
            self.learner.put(w);
            self.models.put(w);
            self.ports.put(w);
            self.links.put(w);
            self.sweep.put(w);
            self.deposits.put(w);
            self.sim.put(w);
            self.epoch.put(w);
            self.pending.put(w);
            self.pending_ops.put(w);
            self.round_added.put(w);
            self.round_short.put(w);
            self.owed.put(w);
            self.deposit_owed.put(w);
            self.minted_carried.put(w);
            self.untracked.put(w);
            self.loaded_chunks.put(w);
            self.destroy_pending.put(w);
            self.reports.put(w);
            self.attacks.put(w);
            self.defence.put(w);
            self.pressure_stamp.put(w);
            self.stations.put(w);
            self.exchange.put(w);
            self.power_source.put(w);
            self.batches.put(w);
            self.made_total.put(w);
            self.last_run.put(w);
            self.approach.put(w);
            self.hold_until.put(w);
            self.hold_stamp.put(w);
            self.editors.put(w);
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
            let magic = r.u32()?;
            if magic != DISTRICT_MAGIC {
                return Err(CodecError::Invalid(format!(
                    "district page magic {magic:#010x}, not {DISTRICT_MAGIC:#010x}: an older format"
                )));
            }
            Ok(District {
                id: Persist::get(r)?,
                revision: Persist::get(r)?,
                dim: Persist::get(r)?,
                name: Persist::get(r)?,
                not_learnable: Persist::get(r)?,
                chunks: Persist::get(r)?,
                inventories: Persist::get(r)?,
                presence: Persist::get(r)?,
                watched: Persist::get(r)?,
                phase: Persist::get(r)?,
                learner: Persist::get(r)?,
                models: Persist::get(r)?,
                ports: Persist::get(r)?,
                links: Persist::get(r)?,
                sweep: Persist::get(r)?,
                deposits: Persist::get(r)?,
                sim: Persist::get(r)?,
                epoch: Persist::get(r)?,
                pending: Persist::get(r)?,
                pending_ops: Persist::get(r)?,
                round_added: Persist::get(r)?,
                round_short: Persist::get(r)?,
                owed: Persist::get(r)?,
                deposit_owed: Persist::get(r)?,
                minted_carried: Persist::get(r)?,
                untracked: Persist::get(r)?,
                loaded_chunks: Persist::get(r)?,
                destroy_pending: Persist::get(r)?,
                reports: Persist::get(r)?,
                attacks: Persist::get(r)?,
                defence: Persist::get(r)?,
                pressure_stamp: Persist::get(r)?,
                stations: Persist::get(r)?,
                exchange: Persist::get(r)?,
                power_source: Persist::get(r)?,
                batches: Persist::get(r)?,
                made_total: Persist::get(r)?,
                last_run: Persist::get(r)?,
                approach: Persist::get(r)?,
                hold_until: Persist::get(r)?,
                hold_stamp: Persist::get(r)?,
                editors: Persist::get(r)?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::{from_bytes, to_bytes};

    /// A district keeps its latest builders, each at its latest edit, and saves them (OGC54); a
    /// page without the magic is an older format and does not open.
    #[test]
    fn a_district_keeps_and_saves_its_latest_builders() {
        let mut d = District::new(DistrictId(3));
        for k in 0..(EDITORS_KEPT as u64 + 2) {
            d.note_editor((k, k), Tick(k * 10));
        }
        d.note_editor((4, 4), Tick(500));
        assert_eq!(d.editors.len(), EDITORS_KEPT);
        assert_eq!(d.editors.last(), Some(&((4, 4), Tick(500))));
        assert!(
            !d.editors.iter().any(|(p, _)| *p == (0, 0) || *p == (1, 1)),
            "the two longest quiet went: {:?}",
            d.editors
        );
        let bytes = to_bytes(&d);
        let back: District = from_bytes(&bytes).unwrap();
        assert_eq!(back, d);
        assert!(from_bytes::<District>(&bytes[4..]).is_err());
    }
}
