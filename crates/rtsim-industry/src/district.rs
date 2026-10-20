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
use serde::Serialize;

use crate::cluster::{MemberSpec, PoolKey, ProcSpec, ProcSpecBody};
use crate::cmd::{BlockDestroy, ChunkReconcile};
use crate::ids::{ChannelId, ChunkPos, DistrictId, Good, GoodClass, GoodVec, SLOT_UNITS, StationId, VecMap};
use crate::learn::{Learned, LearnedTarget, Learner, LinkKey};
use crate::obs::{DepositSample, InvKind, InventoryDecl, NotLearnable};
use crate::reconcile::{GoodLookup, InvState};
use crate::threat::{Defence, Outcome, ThreatClass, ThreatSource};
use crate::tunables::Tunables;

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
    pub fn from_learned(learned: Learned, targets: Vec<LearnedTarget>, season: u32, batches: BTreeMap<(u32, u32), GoodVec>) -> Model {
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
        self.rates.iter().filter(|r| r.1.rate_fixed() > 0).map(|r| r.0)
    }
}

/// One sweep being assembled (docs/industry-boundary.md §3.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepBuf {
    pub sweep: u32,
    pub records: u32,
    pub contents: BTreeMap<u32, GoodVec>,
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
}

/// A chunk's reconcile waiting for the chunk and then for Java's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingChunk {
    pub cmd: ChunkReconcile,
    /// Removals to trim outputs already applied (their shortfalls are untracked mints).
    pub trim: bool,
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
    pub read_only: bool,
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
            Phase::Learned if !self.models.contains_key(&season) => Some("no model for this season"),
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

    pub fn push_attack(&mut self, a: AttackReport) {
        self.attacks.push_back(a);
        if self.attacks.len() > 16 {
            self.attacks.pop_front();
        }
    }

    /// The replay member for this district under `model`, starting from `base` (I6, I13).
    /// `channels` gives each channel's known level and capacity.
    pub fn member_spec(
        &self,
        model: &Model,
        base: &GoodVec,
        channels: &BTreeMap<ChannelId, (GoodVec, i64)>,
        goods: &impl GoodLookup,
        tun: &Tunables,
    ) -> MemberSpec {
        let d = self.id;
        let invs: Vec<&Inventory> = self.inventories.values().filter(|i| !i.destroyed && !i.doomed).collect();
        let produced: BTreeSet<Good> = model.produced().collect();
        let mut pools: Vec<(PoolKey, i64, Option<i64>)> = Vec::new();
        let mut goods_here: BTreeSet<Good> = base.goods().collect();
        goods_here.extend(model.rates.iter().map(|r| r.0));
        for &g in &goods_here {
            let cap = match goods.class(g) {
                GoodClass::Fluid if produced.contains(&g) => {
                    let c: i64 = invs
                        .iter()
                        .filter(|i| i.decl.kind.has(InvKind::FLUIDS) && (i.received.contains(&g) || i.known.get(g) > 0))
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
        let mut targets = Vec::new();
        for t in &model.targets {
            if produced.contains(&t.good) {
                targets.push((PoolKey::Stock(d, t.good), t.stop_at, t.restart_at));
            }
        }
        let mut procs = vec![ProcSpec {
            id: 0,
            body: ProcSpecBody::Box(model.rates.iter().map(|&(g, f)| (PoolKey::Stock(d, g), f)).collect()),
            targets,
            requires: self.power_source.map(|s| (s, 0)),
            latency: model.learned.latency_ticks,
        }];
        let mut next_id = 1u16;
        for &(k, flow) in &model.links {
            let rate = flow.rate_fixed();
            let unit = Diffuser::from_fixed(rate.abs());
            match k {
                LinkKey::Channel(ch, g) => {
                    let (known, cap) = channels.get(&ch).cloned().unwrap_or_default();
                    let ck = PoolKey::Channel(ch, g);
                    pools.push((ck, known.get(g).max(0), (cap > 0).then_some(cap)));
                    let sk = PoolKey::Stock(d, g);
                    if !pools.iter().any(|p| p.0 == sk) {
                        pools.push((sk, 0, None));
                    }
                    let (from, to) = if rate > 0 { (ck, sk) } else { (sk, ck) };
                    procs.push(ProcSpec {
                        id: next_id,
                        body: ProcSpecBody::Link { from, to, flow: unit, batch: None },
                        targets: vec![],
                        requires: None,
                        latency: 0,
                    });
                    next_id += 1;
                }
                LinkKey::Mail { inventory, address, good } if rate < 0 => {
                    let batch = model
                        .batches
                        .get(&(inventory, address))
                        .map(|b| b.get(good))
                        .filter(|&b| b > 0)
                        .unwrap_or_else(|| goods.max_stack(good));
                    let mk = PoolKey::Mail(d, inventory, good);
                    pools.push((mk, 0, Some(OFFLINE_BUFFER_PACKAGES * batch)));
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
                    });
                    next_id += 1;
                }
                LinkKey::Mail { .. } => {}
            }
        }
        // The shared item space of the inventories that received outputs (I6). Every item good
        // held there counts, so imports fill it and consumption frees it; a good also held
        // elsewhere is over-counted, which only stops the replay early (F12).
        let produced_items: Vec<Good> = produced
            .iter()
            .copied()
            .filter(|&g| goods.class(g) == GoodClass::Item)
            .collect();
        // An output whose every receiver was destroyed has nowhere to go: the line jams (F20).
        let jammed = produced_items.iter().any(|g| {
            let mut rec = self.inventories.values().filter(|i| i.received.contains(g)).peekable();
            rec.peek().is_some() && rec.all(|i| i.destroyed || i.doomed)
        });
        let space = if produced_items.is_empty() {
            None
        } else {
            let mut pool_invs: Vec<&Inventory> = invs
                .iter()
                .copied()
                .filter(|i| i.decl.kind.has(InvKind::ITEMS) && produced_items.iter().any(|g| i.received.contains(g)))
                .collect();
            if pool_invs.is_empty() {
                pool_invs = invs.iter().copied().filter(|i| i.decl.kind.has(InvKind::ITEMS)).collect();
            }
            let slots: i64 = pool_invs.iter().map(|i| i64::from(i.decl.item_slots)).sum();
            let margin = tun.replay.fragment_margin_slots * produced_items.len() as i64 * pool_invs.len() as i64;
            let cap = if jammed { 0 } else { (slots - margin).max(0) * SLOT_UNITS };
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
            let fixed = used - weights.iter().map(|(k, w)| w * base.get(k.good()).max(0)).sum::<i64>();
            // A jam must stop the box even when the space is empty.
            let cap = if jammed { fixed.min(used) - 1 } else { cap };
            Some((weights, fixed, cap))
        };
        MemberSpec {
            district: d,
            pools,
            procs,
            space,
        }
    }
}

fn weight(goods: &impl GoodLookup, g: Good) -> i64 {
    let ms = goods.max_stack(g);
    (SLOT_UNITS + ms - 1) / ms
}

// ---- persistence (src/persist.rs) -------------------------------------------------------------

mod persist_impls {
    use rtsim_kernel::codec::{Reader, Result, Writer};

    use super::*;
    use crate::persist::{Persist, invalid, persist_struct, persist_unit_enum};

    persist_struct!(Inventory { decl, chunk, known, known_at, pending, received, doomed, destroyed });
    persist_unit_enum!(Presence, [Presence::Unloaded, Presence::Partial, Presence::Live]);

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

    persist_struct!(Model { learned, targets, rates, links, batches, season });
    persist_struct!(SimSpan { started, base, season });
    persist_struct!(PendingChunk { cmd, trim });
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
        read_only,
    });
    persist_struct!(ExchangeRule { unload, load, observed });
    persist_struct!(SweepBuf { sweep, records, contents, deposits });

    /// `skipped` is one of a few fixed words.
    const SKIPS: [&str; 3] = ["watched", "cooldown", "read-only phase"];

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
            let code = self.skipped.map(|s| SKIPS.iter().position(|x| *x == s).map_or(u8::MAX, |i| i as u8));
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
                skipped: Option::<u8>::get(r)?.map(|c| SKIPS.get(c as usize).copied().unwrap_or("skipped")),
            })
        }
    }

    impl Persist for District {
        fn put(&self, w: &mut Writer) {
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
        }
        fn get(r: &mut Reader<'_>) -> Result<Self> {
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
            })
        }
    }
}
