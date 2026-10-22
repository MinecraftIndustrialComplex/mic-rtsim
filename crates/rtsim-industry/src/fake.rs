// SPDX-License-Identifier: AGPL-3.0-or-later
//! A fake world that stands in for Minecraft, Create and the Java sampler: physical inventories,
//! factories that run only while loaded, trains with interface stations, chunk attachments with
//! reconcile epochs, and its own books. **Scaffolding**, to be deleted once the Java side and its
//! GameTests exist (Ben allowed it; overnight choice F24).
//!
//! It plays the Java side of docs/industry-boundary.md faithfully enough to test the engine end to
//! end: every observation and command goes through the wire codec ([`crate::wire`]) and back;
//! commands are applied one tick late; chunk reconciles are held until the chunk loads, applied
//! removals first, and recorded with the epoch in the same tick; a machine may tick before the
//! reconcile lands (the shortfall case, INDUSTRY.md §19 risk 3).
//!
//! **Run modes.** In a simulated run the player's position decides which districts are loaded and
//! the engine replays the rest. In a control run every district is chunk-loaded all the time, so
//! the factories run for real: the I-B criterion compares the two (INDUSTRY.md §16).

use std::collections::{BTreeMap, BTreeSet};

use rtsim_kernel::clock::Tick;
use rtsim_kernel::journal::OpId;
use serde::Serialize;

use crate::cmd::Cmd;
use crate::engine::Engine;
use crate::ids::{
    BlockPos, ChunkPos, DistrictId, Good, GoodClass, GoodInfo, GoodVec, StationId, TrainId,
};
use crate::obs::{
    BlockFlags, Carriage, CarriageResult, CarriageResultEntry, DefenceSurvey, DepositPluginId,
    DepositSample, DistrictDecl, ExposedBlock, FightObserved, InvKind, InventoryDecl, LoadState,
    Obs, PortKind, PortTransfer, ReconcileResult, ResultEntry, SpawnSample, StationDecl,
    StationPorts, SweepEnd, Timed, TrainCall, TrainDeparture,
};
use crate::reconcile::{GoodLookup, InvState, room_for};
use crate::threat::{Threat, ThreatClass, ThreatFeed};
use crate::tunables::Tunables;
use crate::wire;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FInventory {
    pub index: u32,
    pub pos: BlockPos,
    pub kind: u32,
    pub slots: u32,
    pub fluid_cap: i64,
    pub energy_cap: i64,
    pub contents: GoodVec,
    pub destroyed: bool,
}

/// A batch machine: every `period` ticks, if every input is there, the outputs fit and the target
/// is not reached, it takes the inputs and makes the outputs. That is how a Create line behaves:
/// it runs, or it jams (I6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recipe {
    pub name: String,
    /// (good, inventory index, per batch).
    pub inputs: Vec<(Good, u32, i64)>,
    pub outputs: Vec<(Good, u32, i64)>,
    pub period: u64,
    /// A threshold switch: stop when the inventory holds `stop` of the good, resume at `restart`.
    pub target: Option<(Good, u32, i64, i64)>,
    /// Consumes this much of the district's deposit per batch; a negative amount adds to it (a
    /// deposit good that rises while the district runs: the Destroy plugin's attributed pollution,
    /// I14).
    pub deposit_per_batch: i64,
    /// Runs only while this (district index, recipe index) ran within its own period (PowerGrid).
    pub power_from: Option<(usize, usize)>,
    /// A machine block whose destruction stops this recipe.
    pub machine: BlockPos,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FDistrict {
    pub id: DistrictId,
    pub name: String,
    pub dim: u32,
    pub inventories: Vec<FInventory>,
    pub recipes: Vec<Recipe>,
    /// (good, remaining) of a finite deposit, read by the Ore Excavation plugin.
    pub deposit: Option<(Good, i64)>,
    pub deposit_chunk: ChunkPos,
    #[serde(skip)]
    pub exposed: Vec<ExposedBlock>,
    #[serde(skip)]
    pub survey: Option<DefenceSurvey>,
    /// Natural spawns per night tick per dark block, as the sampler would measure them.
    pub spawn_rate: f64,
    #[serde(skip)]
    pub loaded: bool,
    #[serde(skip)]
    pub epochs: BTreeMap<ChunkPos, u32>,
    #[serde(skip)]
    pub held: Vec<crate::cmd::ChunkReconcile>,
    #[serde(skip)]
    pub sweep: u32,
    #[serde(skip)]
    pub threshold_on: Vec<bool>,
    #[serde(skip)]
    pub last_ran: Vec<Tick>,
    #[serde(skip)]
    pub broken: BTreeSet<BlockPos>,
    #[serde(skip)]
    pub revision: u32,
    /// Loaded at least once: only a reload can tick on stale stock.
    #[serde(skip)]
    pub ever_loaded: bool,
}

impl FDistrict {
    /// Its chunks: those of its inventories, its machines, its surveyed blocks and its deposit.
    pub fn chunks(&self) -> BTreeSet<ChunkPos> {
        let mut c: BTreeSet<ChunkPos> = self
            .inventories
            .iter()
            .map(|i| i.pos.chunk(self.dim))
            .collect();
        c.extend(self.recipes.iter().map(|r| r.machine.chunk(self.dim)));
        c.extend(self.exposed.iter().map(|e| e.pos.chunk(self.dim)));
        if self.deposit.is_some() {
            c.insert(self.deposit_chunk);
        }
        c
    }

    fn inv_chunk(&self, index: u32) -> Option<ChunkPos> {
        self.inventories
            .iter()
            .find(|i| i.index == index)
            .map(|i| i.pos.chunk(self.dim))
    }

    /// The chunks a recipe needs loaded to run: its machine's, its inventories', its deposit's.
    fn recipe_chunks(&self, ri: usize) -> BTreeSet<ChunkPos> {
        let r = &self.recipes[ri];
        let mut c = BTreeSet::from([r.machine.chunk(self.dim)]);
        for &(_, inv, _) in r.inputs.iter().chain(&r.outputs) {
            c.extend(self.inv_chunk(inv));
        }
        if let Some((_, inv, _, _)) = r.target {
            c.extend(self.inv_chunk(inv));
        }
        if r.deposit_per_batch != 0 && self.deposit.is_some() {
            c.insert(self.deposit_chunk);
        }
        c
    }

    fn inv(&mut self, index: u32) -> &mut FInventory {
        self.inventories
            .iter_mut()
            .find(|i| i.index == index)
            .expect("inventory")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FStation {
    pub id: StationId,
    pub district: usize,
    pub pos: BlockPos,
    /// Portable Storage Interfaces: goods unloaded into, and loaded from, these inventories.
    pub unload: Vec<(Good, u32)>,
    pub load: Vec<(Good, u32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FCarriage {
    pub index: u32,
    pub slots: u32,
    pub cargo: GoodVec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TrainState {
    Dwelling { stop: usize, since: u64 },
    Moving { to: usize, arrive: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FTrain {
    pub id: TrainId,
    /// Station indices, visited in a loop.
    pub stops: Vec<usize>,
    pub travel: u64,
    pub dwell: u64,
    pub carriages: Vec<FCarriage>,
    pub state: TrainState,
}

/// The world's own books: what its machines made and used while loaded, and what fell out of
/// destroyed blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct WorldBooks {
    pub minted: GoodVec,
    pub burned: GoodVec,
    pub dropped: GoodVec,
    /// Brought in from outside the world's districts and trains ([`FakeWorld::import`]).
    pub imported: GoodVec,
    pub batches: u64,
}

/// Where the player is, from a tick on: the districts within view distance (empty: none).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Visit {
    #[serde(serialize_with = "crate::ser::tick")]
    pub from: Tick,
    pub at: Vec<usize>,
}

/// From a tick on, exactly these chunks of one district are loaded (I7: a player at its edge).
/// The latest of a [`Visit`] and a `ChunkVisit` for the district decides; a tie goes to this.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChunkVisit {
    #[serde(serialize_with = "crate::ser::tick")]
    pub from: Tick,
    pub district: usize,
    pub chunks: Vec<ChunkPos>,
}

/// What a remote fake world hands its bridge ([`FakeWorld::remote`]): observations, and the
/// bridge's own COMMAND_RESULT for every command with an op id it applied or skipped
/// (docs/boundary.md §2.3), in the order the Java side would journal them.
#[derive(Clone, Debug, PartialEq)]
pub enum Outgoing {
    Obs(Timed),
    /// `status`: [`RESULT_APPLIED`], [`RESULT_DUPLICATE`] or [`RESULT_FAILED`].
    CommandResult {
        op: OpId,
        status: u32,
    },
}

/// COMMAND_RESULT statuses (docs/boundary.md §2.3).
pub const RESULT_APPLIED: u32 = 1;
pub const RESULT_DUPLICATE: u32 = 2;
pub const RESULT_FAILED: u32 = 3;

/// What a world save keeps of a fake world, for a simulated crash ([`FakeWorld::saved`]).
#[derive(Clone, Debug)]
pub struct SavedWorld {
    t: Tick,
    districts: Vec<FDistrict>,
    trains: Vec<FTrain>,
    books: WorldBooks,
    applied: BTreeSet<OpId>,
}

impl SavedWorld {
    /// This save with the districts' chunks (contents, reconcile epochs, broken blocks) and the
    /// world's books from a later one: chunks Minecraft saved on unload after the world save, which
    /// a crash then leaves ahead of the journal (docs/boundary.md §3.1).
    pub fn with_chunks_of(&self, ahead: &SavedWorld) -> SavedWorld {
        SavedWorld {
            districts: ahead.districts.clone(),
            books: ahead.books.clone(),
            ..self.clone()
        }
    }
}

/// One line of what happened, for the CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Event {
    pub tick: u64,
    pub day: f64,
    pub what: String,
}

pub struct FakeWorld {
    pub engine: Engine,
    pub t: Tick,
    pub goods: BTreeMap<Good, GoodInfo>,
    pub districts: Vec<FDistrict>,
    pub stations: Vec<FStation>,
    pub trains: Vec<FTrain>,
    pub visits: Vec<Visit>,
    /// Partial loads (I7), on top of `visits`.
    pub chunk_visits: Vec<ChunkVisit>,
    /// The chunks of each district (by index) loaded now.
    pub loaded_chunks: BTreeMap<usize, BTreeSet<ChunkPos>>,
    /// IND_CHUNK_UNLOADED is sent for each chunk that unloads (the Java side's half of I7). Off:
    /// a Java side that never sends it.
    pub chunk_unload_records: bool,
    /// How long before a scripted load the approach signal starts (`approach_signal`).
    /// PLACEHOLDER 60 ticks: three of the Java side's 20-tick reports.
    pub approach_lead: u64,
    /// (sink, source) district indices joined by a PowerGrid line (I13).
    pub power_links: Vec<(usize, usize)>,
    /// Every district stays loaded (the control run).
    pub control: bool,
    /// Ticks between sweeps of a loaded district (TUNABLE on the Java side, PLACEHOLDER 100).
    pub sweep_period: u64,
    /// When a district loads, its machines tick once before the reconcile lands.
    pub machines_tick_first: bool,
    /// An ideal Java side: reconciles land on the load tick itself, before any machine runs. The
    /// real protocol is at least one tick late (docs/boundary.md §1.2); this removes the race to
    /// measure the replay alone.
    pub apply_on_load: bool,
    /// Sends IND_APPROACH before a scripted visit loads a district, and applies the reconciles
    /// held for a chunk at its load, before any machine ticks (the Java side's half of "reconcile
    /// ahead of arrival").
    pub approach_signal: bool,
    /// Every IND_CARRIAGE_DELTA is answered FAILED and changes nothing (a remote world's test of
    /// the engine's correction).
    pub fail_carriage_deltas: bool,
    /// A loaded call reports the inventories its station's interfaces serve (IND_STATION_PORTS,
    /// choice SG32). Off: a Java side that never finds them.
    pub station_ports: bool,
    pub threats: Option<Box<dyn ThreatFeed>>,
    /// A scripted player's one-off repair has happened (the iron line's).
    pub repaired: bool,
    pub books: WorldBooks,
    pub log: Vec<Event>,
    pub wire_bytes: u64,
    /// The engine is elsewhere (a native engine behind the bridge's buffers): observations go to
    /// [`FakeWorld::outgoing`], commands come in through [`FakeWorld::deliver`], threats go to
    /// [`FakeWorld::threats_out`], and `engine` is unused.
    pub remote: bool,
    pub outgoing: Vec<Outgoing>,
    pub threats_out: Vec<Threat>,
    /// Every IND_PROBE_SET the engine sent (GLASSBOX.md §5.3), oldest first.
    pub probes: Vec<crate::glass::records::ProbeSet>,
    /// Every chunk reconcile applied: (op, district, epoch, chunk), oldest first.
    pub reconciled: Vec<(OpId, DistrictId, u32, ChunkPos)>,
    obs_seq: u64,
    cmd_seq: u64,
    queue: Vec<Cmd>,
    applied: BTreeSet<OpId>,
    /// Trains at simulated stations waiting for the engine's exchange.
    initial: GoodVec,
}

impl GoodLookup for FakeWorld {
    fn info(&self, g: Good) -> Option<&GoodInfo> {
        self.goods.get(&g)
    }
}

impl FakeWorld {
    pub fn new(seed: u64, tun: Tunables) -> Self {
        let mut engine = Engine::new(seed, tun, Tick::ZERO);
        engine.log_enabled = true;
        Self {
            engine,
            t: Tick::ZERO,
            goods: BTreeMap::new(),
            districts: Vec::new(),
            stations: Vec::new(),
            trains: Vec::new(),
            visits: Vec::new(),
            chunk_visits: Vec::new(),
            loaded_chunks: BTreeMap::new(),
            chunk_unload_records: true,
            approach_lead: 60,
            power_links: Vec::new(),
            control: false,
            sweep_period: 100,
            machines_tick_first: true,
            apply_on_load: false,
            approach_signal: false,
            threats: None,
            repaired: false,
            books: WorldBooks::default(),
            log: Vec::new(),
            wire_bytes: 0,
            remote: false,
            fail_carriage_deltas: false,
            station_ports: true,
            outgoing: Vec::new(),
            threats_out: Vec::new(),
            probes: Vec::new(),
            reconciled: Vec::new(),
            obs_seq: 0,
            cmd_seq: 0,
            queue: Vec::new(),
            applied: BTreeSet::new(),
            initial: GoodVec::new(),
        }
    }

    pub fn good(&mut self, id: u32, info: GoodInfo) -> Good {
        let g = Good(id);
        self.goods.insert(g, info.clone());
        self.engine.define_good(g, info);
        g
    }

    fn note(&mut self, what: String) {
        self.log.push(Event {
            tick: self.t.0,
            day: self.t.0 as f64 / 24_000.0,
            what,
        });
    }

    /// The same world with its engine elsewhere ([`FakeWorld::remote`]). Reconciles are at least
    /// one tick late there, so the ideal Java side (`apply_on_load`) is off.
    pub fn into_remote(mut self) -> Self {
        self.remote = true;
        self.apply_on_load = false;
        self
    }

    /// Moves every district to dimension `dim` (Java's dictionary id) before [`FakeWorld::declare`]:
    /// a bridge's dictionary ids start at 1, the scenarios use 0.
    pub fn move_to_dim(&mut self, dim: u32) {
        for d in &mut self.districts {
            d.dim = dim;
            d.deposit_chunk.dim = dim;
        }
    }

    /// A command from the remote engine, decoded from its buffer: applied at the next tick.
    pub fn deliver(&mut self, c: Cmd) {
        self.queue.push(c);
    }

    /// Commands delivered and not applied yet (they apply at the next tick), and reconciles held
    /// for chunks that are not loaded.
    pub fn commands_in_flight(&self) -> usize {
        self.queue.len() + self.districts.iter().map(|d| d.held.len()).sum::<usize>()
    }

    /// The engine op ids this world applied (its journal's applied set).
    pub fn applied_ops(&self) -> &BTreeSet<OpId> {
        &self.applied
    }

    /// What a world save holds (a remote world's crash test rolls back to it).
    pub fn saved(&self) -> SavedWorld {
        SavedWorld {
            t: self.t,
            districts: self.districts.clone(),
            trains: self.trains.clone(),
            books: self.books.clone(),
            applied: self.applied.clone(),
        }
    }

    /// Rolls the world back to a save: what Minecraft does after a crash. Commands in flight and
    /// observations not yet handed over are lost with the session; districts are unloaded until
    /// the next tick loads them again.
    pub fn restore(&mut self, s: &SavedWorld) {
        self.t = s.t;
        self.districts = s.districts.clone();
        self.trains = s.trains.clone();
        self.books = s.books.clone();
        self.applied = s.applied.clone();
        self.queue.clear();
        self.outgoing.clear();
        self.loaded_chunks.clear();
        for d in &mut self.districts {
            d.loaded = false;
            d.held.clear();
        }
    }

    fn answer(&mut self, op: OpId, status: u32) {
        if self.remote {
            self.outgoing.push(Outgoing::CommandResult { op, status });
        }
    }

    /// Sends one observation through the wire codec to the engine.
    fn send(&mut self, obs: Obs) {
        let timed = Timed { at: self.t, obs };
        if self.remote {
            let mut buf = Vec::new();
            wire::encode_obs(1, &timed, &mut buf).expect("encodable observation");
            self.wire_bytes += buf.len() as u64;
            self.outgoing.push(Outgoing::Obs(timed));
            return;
        }
        let seq = if timed.obs.is_journaled() {
            self.obs_seq += 1;
            self.obs_seq
        } else {
            0
        };
        let mut buf = Vec::new();
        wire::encode_obs(seq, &timed, &mut buf).expect("encodable observation");
        self.wire_bytes += buf.len() as u64;
        let (_, back) = wire::decode_obs(&buf)
            .expect("decodable observation")
            .expect("industry kind");
        assert_eq!(back, timed, "the wire codec changed an observation");
        self.engine.ingest(&back);
    }

    /// Declares everything to the engine and records the starting totals.
    pub fn declare(&mut self) {
        for d in &mut self.districts {
            d.revision += 1;
            d.threshold_on = vec![true; d.recipes.len()];
            d.last_ran = vec![Tick::ZERO; d.recipes.len()];
        }
        self.announce();
        self.initial = self.totals();
    }

    /// Tells the engine every district (its chunks, inventories and survey), power link and
    /// station as they are now, and each district's loaded chunks and load state: what the Java
    /// side does at a session start, and when the industry module is switched on again (U10: a
    /// switched-off engine ignored everything meanwhile).
    pub fn announce(&mut self) {
        for di in 0..self.districts.len() {
            let d = &self.districts[di];
            let (id, rev, dim, n) = (d.id, d.revision, d.dim, d.inventories.len() as u32);
            let chunks: Vec<(i32, i32)> = d.chunks().iter().map(|c| (c.x, c.z)).collect();
            let invs = d.inventories.clone();
            self.send(Obs::District(DistrictDecl {
                district: id,
                revision: rev,
                dim,
                name: di as u32 + 1,
                not_learnable: None,
                inventory_count: n,
            }));
            self.send(Obs::DistrictChunks {
                district: id,
                revision: rev,
                part: 0,
                chunks: chunks.clone(),
            });
            for i in invs {
                self.send(Obs::Inventory(InventoryDecl {
                    district: id,
                    revision: rev,
                    index: i.index,
                    pos: i.pos,
                    kind: InvKind(i.kind),
                    item_slots: i.slots,
                    fluid_capacity: i.fluid_cap,
                    energy_capacity: i.energy_cap,
                }));
            }
            if let Some(s) = self.districts[di].survey.clone() {
                self.send(Obs::Survey(s));
                // One part per chunk, every chunk of the district, as the Java survey sends them (F5).
                let mut by_chunk: BTreeMap<(i32, i32), Vec<ExposedBlock>> =
                    chunks.iter().map(|&c| (c, Vec::new())).collect();
                for b in &self.districts[di].exposed {
                    by_chunk
                        .entry((b.pos.x >> 4, b.pos.z >> 4))
                        .or_default()
                        .push(*b);
                }
                for (chunk, blocks) in by_chunk {
                    self.send(Obs::SurveyBlocks {
                        district: id,
                        revision: rev,
                        part: 0,
                        chunk,
                        partial: false,
                        blocks,
                    });
                }
            }
        }
        for (sink, src) in self.power_links.clone() {
            let (sink, source) = (self.districts[sink].id, self.districts[src].id);
            self.send(Obs::PowerLink {
                sink,
                source: Some(source),
            });
        }
        for s in self.stations.clone() {
            let d = &self.districts[s.district];
            self.send(Obs::Station(StationDecl {
                station: s.id,
                district: Some(d.id),
                dim: d.dim,
                pos: s.pos,
                name: s.id.0 as u32,
            }));
        }
        for di in 0..self.districts.len() {
            let loaded: Vec<ChunkPos> = self
                .loaded_chunks
                .get(&di)
                .map(|c| c.iter().copied().collect())
                .unwrap_or_default();
            if loaded.is_empty() {
                continue;
            }
            let id = self.districts[di].id;
            let total = self.districts[di].chunks().len() as u32;
            for c in &loaded {
                let e = self.districts[di].epochs.get(c).copied().unwrap_or(0);
                self.send(Obs::ChunkLoaded {
                    district: id,
                    chunk: *c,
                    applied_epoch: e,
                });
            }
            let state = if loaded.len() as u32 == total {
                LoadState::Live
            } else {
                LoadState::Partial
            };
            self.send(Obs::LoadState {
                district: id,
                state,
                ticking_chunks: loaded.len() as u32,
                total_chunks: total,
            });
            self.send(Obs::Watched {
                district: id,
                watched: true,
            });
        }
    }

    /// Everything physical: inventories, carriages, deposits, dropped items.
    pub fn totals(&self) -> GoodVec {
        let mut v = GoodVec::new();
        for d in &self.districts {
            for i in &d.inventories {
                v.merge(&i.contents);
            }
            if let Some((g, n)) = d.deposit {
                v.add(g, n);
            }
        }
        for t in &self.trains {
            for c in &t.carriages {
                v.merge(&c.cargo);
            }
        }
        v.merge(&self.books.dropped);
        v
    }

    pub fn initial_totals(&self) -> &GoodVec {
        &self.initial
    }

    /// The chunks of district `di` the script loads at `at`.
    fn chunks_at(&self, di: usize, at: Tick) -> BTreeSet<ChunkPos> {
        let all = self.districts[di].chunks();
        if self.control {
            return all;
        }
        let visit = self.visits.iter().rev().find(|v| v.from <= at);
        let partial = self
            .chunk_visits
            .iter()
            .rev()
            .find(|v| v.district == di && v.from <= at);
        match (visit, partial) {
            (_, Some(p)) if visit.is_none_or(|v| p.from >= v.from) => p
                .chunks
                .iter()
                .copied()
                .filter(|c| all.contains(c))
                .collect(),
            (Some(v), _) if v.at.contains(&di) => all,
            _ => BTreeSet::new(),
        }
    }

    fn chunk_loaded(&self, di: usize, c: ChunkPos) -> bool {
        self.loaded_chunks.get(&di).is_some_and(|s| s.contains(&c))
    }

    fn any_loaded(&self, di: usize) -> bool {
        self.loaded_chunks.get(&di).is_some_and(|s| !s.is_empty())
    }

    fn recipe_loaded(&self, di: usize, ri: usize) -> bool {
        self.districts[di]
            .recipe_chunks(ri)
            .iter()
            .all(|c| self.chunk_loaded(di, *c))
    }

    /// One game tick.
    pub fn tick(&mut self) {
        self.t += 1;
        // Commands from the last step, one tick late (docs/boundary.md §1.2).
        let cmds = std::mem::take(&mut self.queue);
        for c in cmds {
            self.apply(c);
        }
        // A player heading for a district that will load soon (the approach signal, H23).
        if self.approach_signal && self.t.0.is_multiple_of(20) {
            self.approach_records();
        }
        // Loads and unloads.
        let mut loaded_now = false;
        for di in 0..self.districts.len() {
            let want = self.chunks_at(di, self.t);
            let have = self.loaded_chunks.get(&di).cloned().unwrap_or_default();
            if want != have {
                loaded_now |= want.difference(&have).next().is_some();
                self.set_chunks(di, &have, &want);
            }
        }
        if loaded_now && self.apply_on_load && !self.remote {
            let cmds = self.engine.drain_commands();
            for c in cmds {
                self.apply(c);
            }
        }
        // Held reconciles for chunks that are loaded now.
        for di in 0..self.districts.len() {
            if !self.districts[di].held.is_empty() {
                let held = std::mem::take(&mut self.districts[di].held);
                for c in held {
                    self.apply(Cmd::ChunkReconcile(c));
                }
            }
        }
        // Machines: those whose chunks are all loaded.
        for di in 0..self.districts.len() {
            for ri in 0..self.districts[di].recipes.len() {
                if self.recipe_loaded(di, ri) {
                    self.run_recipe(di, ri);
                }
            }
        }
        self.trains_tick();
        // Sampling: the loaded inventories of districts with a chunk loaded.
        if self.t.0.is_multiple_of(self.sweep_period) {
            for di in 0..self.districts.len() {
                if self.any_loaded(di) {
                    self.sweep(di);
                }
            }
        }
        // Natural spawns, measured while the player is in a district at night (I20).
        if self.t.0.is_multiple_of(24_000) {
            for di in 0..self.districts.len() {
                let d = &self.districts[di];
                if d.loaded && !self.control && d.spawn_rate > 0.0 {
                    let dark = d.survey.as_ref().map_or(0, |s| s.dark_spawnable);
                    let ticks = 10_000u64;
                    let n = (d.spawn_rate * f64::from(dark) * ticks as f64).round() as u32;
                    let id = d.id;
                    self.send(Obs::SpawnSample(SpawnSample {
                        district: id,
                        observed_ticks: ticks,
                        dark_blocks: u64::from(dark),
                        spawns: [n, n / 2, 0, n / 4],
                    }));
                }
            }
        }
        // Threats from other systems.
        if let Some(mut feed) = self.threats.take() {
            let mut due = Vec::new();
            feed.due(self.t, self.t + 1, &mut due);
            self.threats = Some(feed);
            for th in due {
                self.threat(th);
            }
        }
        if self.remote {
            return;
        }
        self.engine.advance_to(self.t);
        // The step's commands.
        for c in self.engine.drain_commands() {
            self.cmd_seq += 1;
            let mut buf = Vec::new();
            wire::encode_cmd(self.cmd_seq, &c, &mut buf).expect("encodable command");
            self.wire_bytes += buf.len() as u64;
            let (_, back) = wire::decode_cmd(&buf)
                .expect("decodable")
                .expect("industry kind");
            assert_eq!(back, c, "the wire codec changed a command");
            self.queue.push(back);
        }
    }

    pub fn run_until(&mut self, t: Tick) {
        while self.t < t {
            self.tick();
        }
    }

    fn threat(&mut self, th: Threat) {
        // A watched district fights for real: a fake fight, observed (I21 calibration).
        if let crate::threat::ThreatTarget::District(d) = th.target
            && let Some(di) = self.districts.iter().position(|x| x.id == d)
            && self.any_loaded(di)
        {
            let killed = th.count / 2;
            self.send(Obs::Fight(FightObserved {
                district: d,
                class: th.class,
                attackers: th.count,
                attacker_strength: th.strength as f32,
                attackers_killed: killed,
                blocks_destroyed: 0,
                machines_destroyed: 0,
                guards_lost: 0,
                items_stolen: 0,
                duration_ticks: 200,
            }));
            self.note(format!(
                "real fight at {}: {} {:?}, {killed} killed",
                self.districts[di].name, th.count, th.class
            ));
            return;
        }
        if self.remote {
            self.threats_out.push(th);
        } else {
            self.engine.threat(&th);
        }
        self.note(format!(
            "{:?} attack ({:?}, {}) on {:?}",
            th.source, th.class, th.count, th.target
        ));
    }

    /// IND_APPROACH for every district a scripted load reaches within `approach_lead` ticks,
    /// while some chunk of it it will load is not loaded yet.
    fn approach_records(&mut self) {
        let horizon = self.approach_lead;
        let mut out = Vec::new();
        for di in 0..self.districts.len() {
            let have = self.loaded_chunks.get(&di).cloned().unwrap_or_default();
            let soon = (1..=horizon).step_by(20).chain([horizon]).find(|&dt| {
                self.chunks_at(di, self.t + dt)
                    .difference(&have)
                    .next()
                    .is_some()
            });
            if let Some(dt) = soon {
                out.push((self.districts[di].id, dt));
            }
        }
        for (id, dt) in out {
            self.send(Obs::Approach {
                district: id,
                distance: 200,
                eta_ticks: Some(dt as u32),
                closing: true,
            });
        }
    }

    /// The district's loaded chunks go from `have` to `want`, as Minecraft's chunk events and the
    /// Java side's records say it (I7).
    fn set_chunks(&mut self, di: usize, have: &BTreeSet<ChunkPos>, want: &BTreeSet<ChunkPos>) {
        let id = self.districts[di].id;
        let total = self.districts[di].chunks().len() as u32;
        let runnable_before: Vec<bool> = (0..self.districts[di].recipes.len())
            .map(|ri| self.recipe_loaded(di, ri))
            .collect();
        let state = |n: usize| {
            if n == 0 {
                LoadState::Unloaded
            } else if n as u32 == total {
                LoadState::Live
            } else {
                LoadState::Partial
            }
        };
        let old = state(have.len());
        for c in have.difference(want) {
            self.loaded_chunks.entry(di).or_default().remove(c);
            if self.chunk_unload_records {
                self.send(Obs::ChunkUnloaded {
                    district: id,
                    chunk: *c,
                });
            }
        }
        let loading: Vec<ChunkPos> = want.difference(have).copied().collect();
        for c in &loading {
            self.loaded_chunks.entry(di).or_default().insert(*c);
            if self.approach_signal {
                // The Java side applies a reconcile it holds for this chunk at the chunk's load,
                // before any machine ticks (reconcile ahead of arrival). Its result is journaled
                // before IND_CHUNK_LOADED, which then carries the epoch now applied.
                let held: Vec<crate::cmd::ChunkReconcile> = {
                    let d = &mut self.districts[di];
                    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut d.held)
                        .into_iter()
                        .partition(|r| r.chunk == *c);
                    d.held = rest;
                    let mut mine = mine;
                    mine.sort_by_key(|r| r.epoch);
                    mine
                };
                for rc in held {
                    self.apply(Cmd::ChunkReconcile(rc));
                }
            }
            let e = self.districts[di].epochs.get(c).copied().unwrap_or(0);
            self.send(Obs::ChunkLoaded {
                district: id,
                chunk: *c,
                applied_epoch: e,
            });
        }
        let reload =
            !loading.is_empty() && std::mem::replace(&mut self.districts[di].ever_loaded, true);
        // A machine that ticks before the reconcile lands takes its inputs first. Only a reload
        // has a reconcile to race.
        if self.machines_tick_first && reload {
            for (ri, before) in runnable_before.iter().enumerate() {
                if !before && self.recipe_loaded(di, ri) {
                    self.batch(di, ri, true);
                }
            }
        }
        let new = state(want.len());
        self.districts[di].loaded = new == LoadState::Live;
        if new != old {
            self.send(Obs::LoadState {
                district: id,
                state: new,
                ticking_chunks: want.len() as u32,
                total_chunks: total,
            });
        }
        let name = self.districts[di].name.clone();
        match (old, new) {
            (LoadState::Unloaded, LoadState::Unloaded) => {}
            (LoadState::Unloaded, _) => {
                // The player's presence is what loads a district here, so it is watched.
                self.send(Obs::Watched {
                    district: id,
                    watched: true,
                });
                self.note(format!(
                    "{name} loads{}",
                    if new == LoadState::Partial {
                        " in part"
                    } else {
                        ""
                    }
                ));
            }
            (_, LoadState::Unloaded) => self.note(format!("{name} unloads")),
            (_, LoadState::Partial) => self.note(format!("{name}: part of it loaded")),
            _ => self.note(format!("{name} loads")),
        }
    }

    fn run_recipe(&mut self, di: usize, ri: usize) {
        let period = self.districts[di].recipes[ri].period;
        if !self.t.0.is_multiple_of(period) {
            return;
        }
        self.batch(di, ri, false);
    }

    /// One batch if it can run. `forced` runs it off its period (the tick-first case).
    fn batch(&mut self, di: usize, ri: usize, forced: bool) -> bool {
        let t = self.t;
        let goods = self.goods.clone();
        let power_ok = match self.districts[di].recipes[ri].power_from {
            None => true,
            Some((pd, pr)) => {
                let src = &self.districts[pd];
                let p = src.recipes[pr].period;
                self.recipe_loaded(pd, pr) && t.0.saturating_sub(src.last_ran[pr].0) <= p
            }
        };
        let d = &mut self.districts[di];
        let r = d.recipes[ri].clone();
        if !power_ok || d.broken.contains(&r.machine) {
            return false;
        }
        if let Some((g, inv, stop, restart)) = r.target {
            let have = d.inv(inv).contents.get(g);
            if d.threshold_on[ri] && have >= stop {
                d.threshold_on[ri] = false;
            } else if !d.threshold_on[ri] && have <= restart {
                d.threshold_on[ri] = true;
            }
            if !d.threshold_on[ri] {
                return false;
            }
        }
        for &(g, inv, n) in &r.inputs {
            if d.inv(inv).contents.get(g) < n {
                return false;
            }
        }
        if r.deposit_per_batch > 0 && d.deposit.is_none_or(|(_, left)| left < r.deposit_per_batch) {
            return false;
        }
        // Outputs must fit (after the inputs leave).
        let mut after: BTreeMap<u32, GoodVec> = BTreeMap::new();
        for &(g, inv, n) in &r.inputs {
            after
                .entry(inv)
                .or_insert_with(|| d.inv(inv).contents.clone())
                .add(g, -n);
        }
        for &(g, inv, n) in &r.outputs {
            let fi = d.inv(inv).clone();
            let cur = after.entry(inv).or_insert_with(|| fi.contents.clone());
            let st = inv_state(&fi, cur.clone());
            if room_for(&st, cur, g, &goods) < n {
                return false;
            }
            cur.add(g, n);
        }
        for &(g, inv, n) in &r.inputs {
            d.inv(inv).contents.add(g, -n);
            self.books.burned.add(g, n);
        }
        if r.deposit_per_batch > 0
            && let Some((g, left)) = d.deposit.as_mut()
        {
            *left -= r.deposit_per_batch;
            self.books.burned.add(*g, r.deposit_per_batch);
        }
        if r.deposit_per_batch < 0
            && let Some((g, left)) = d.deposit.as_mut()
        {
            *left -= r.deposit_per_batch;
            self.books.minted.add(*g, -r.deposit_per_batch);
        }
        for &(g, inv, n) in &r.outputs {
            d.inv(inv).contents.add(g, n);
            self.books.minted.add(g, n);
        }
        d.last_ran[ri] = t;
        self.books.batches += 1;
        let _ = forced;
        true
    }

    fn sweep(&mut self, di: usize) {
        let loaded = self.loaded_chunks.get(&di).cloned().unwrap_or_default();
        let live = self.districts[di].loaded;
        let d = &mut self.districts[di];
        d.sweep += 1;
        let (id, sweep, dim) = (d.id, d.sweep, d.dim);
        // Only inventories in loaded chunks can be read; a partial sweep is no learning point.
        let invs: Vec<FInventory> = d
            .inventories
            .iter()
            .filter(|i| !i.destroyed && loaded.contains(&i.pos.chunk(dim)))
            .cloned()
            .collect();
        let deposit = d.deposit.filter(|_| loaded.contains(&d.deposit_chunk));
        let chunk = d.deposit_chunk;
        let mut records = 0u32;
        for i in &invs {
            self.send(Obs::StockSample {
                district: id,
                inventory: i.index,
                sweep,
                goods: i.contents.clone(),
            });
            records += 1;
        }
        if let Some((g, left)) = deposit {
            self.send(Obs::DepositSample(DepositSample {
                district: id,
                good: g,
                plugin: DepositPluginId::OreExcavation,
                chunk,
                remaining: left,
                infinite: false,
                sweep,
            }));
            records += 1;
        }
        self.send(Obs::SweepEnd(SweepEnd {
            district: id,
            sweep,
            records,
            inventories: invs.len() as u32,
            live,
            creative: false,
            overstressed: false,
        }));
    }

    fn trains_tick(&mut self) {
        let t = self.t.0;
        for ti in 0..self.trains.len() {
            match self.trains[ti].state {
                TrainState::Moving { to, arrive } if t >= arrive => {
                    self.trains[ti].state = TrainState::Dwelling { stop: to, since: t };
                    self.arrive(ti, to);
                }
                TrainState::Dwelling { stop, since } if t >= since + self.trains[ti].dwell => {
                    self.depart(ti, stop);
                    let next = (stop + 1) % self.trains[ti].stops.len();
                    let travel = self.trains[ti].travel;
                    self.trains[ti].state = TrainState::Moving {
                        to: next,
                        arrive: t + travel,
                    };
                }
                _ => {}
            }
        }
    }

    fn carriages(&self, ti: usize) -> Vec<Carriage> {
        self.trains[ti]
            .carriages
            .iter()
            .map(|c| Carriage {
                index: c.index,
                item_slots: c.slots,
                fluid_capacity: 0,
                goods: c.cargo.clone(),
            })
            .collect()
    }

    fn arrive(&mut self, ti: usize, stop: usize) {
        let si = self.trains[ti].stops[stop];
        let st = self.stations[si].clone();
        let di = st.district;
        // The interfaces work while the whole district is loaded; Java emulates them only while
        // it is wholly unloaded; in part loaded, nothing moves.
        let loaded = self.districts[di].loaded;
        let simulated = !self.any_loaded(di);
        let id = self.districts[di].id;
        let train = self.trains[ti].id;
        let cars = self.carriages(ti);
        // The cargo before the interfaces dock, as the Java side's approach snapshot (SG30).
        self.send(Obs::TrainArrived(TrainCall {
            train,
            station: st.id,
            district: id,
            simulated,
            cargo_from: crate::obs::CargoFrom::Approach,
            carriages: cars,
        }));
        if !loaded {
            return;
        }
        // The inventories the interfaces serve, as the Java side reports them from a loaded call
        // (SG32).
        let mut inventories: Vec<u32> = st.unload.iter().chain(&st.load).map(|&(_, i)| i).collect();
        inventories.sort_unstable();
        inventories.dedup();
        if self.station_ports {
            self.send(Obs::StationPorts(StationPorts {
                station: st.id,
                district: id,
                inventories,
            }));
        }
        // Portable Storage Interfaces, for real: unload all, then load all (I15's pattern).
        let goods = self.goods.clone();
        let mut port = GoodVec::new();
        for (g, inv) in &st.unload {
            for c in 0..self.trains[ti].carriages.len() {
                let n = self.trains[ti].carriages[c].cargo.get(*g);
                if n == 0 {
                    continue;
                }
                let fi = self.districts[di].inv(*inv).clone();
                let room = room_for(
                    &inv_state(&fi, fi.contents.clone()),
                    &fi.contents,
                    *g,
                    &goods,
                );
                let m = n.min(room);
                self.trains[ti].carriages[c].cargo.add(*g, -m);
                self.districts[di].inv(*inv).contents.add(*g, m);
                port.add(*g, m);
            }
        }
        for (g, inv) in &st.load {
            for c in 0..self.trains[ti].carriages.len() {
                let have = self.districts[di].inv(*inv).contents.get(*g);
                let car = self.trains[ti].carriages[c].clone();
                let cs = InvState {
                    index: 0,
                    pos: BlockPos::default(),
                    chunk: ChunkPos::default(),
                    kind: InvKind(InvKind::ITEMS),
                    item_slots: car.slots,
                    fluid_capacity: 0,
                    energy_capacity: 0,
                    contents: car.cargo.clone(),
                    received: BTreeSet::new(),
                };
                let room = room_for(&cs, &car.cargo, *g, &goods);
                let m = have.min(room);
                if m > 0 {
                    self.districts[di].inv(*inv).contents.add(*g, -m);
                    self.trains[ti].carriages[c].cargo.add(*g, m);
                    port.add(*g, -m);
                }
            }
        }
        if !port.is_empty() {
            self.send(Obs::Port(PortTransfer {
                district: id,
                kind: PortKind::TrainCargo,
                inventory: None,
                counterpart: train.0,
                op: None,
                goods: port,
            }));
        }
    }

    fn depart(&mut self, ti: usize, stop: usize) {
        let si = self.trains[ti].stops[stop];
        let st = self.stations[si].clone();
        let next = self.trains[ti].stops[(stop + 1) % self.trains[ti].stops.len()];
        let id = self.districts[st.district].id;
        let train = self.trains[ti].id;
        let cars = self.carriages(ti);
        let travel = self.trains[ti].travel;
        self.send(Obs::TrainDeparted(TrainDeparture {
            train,
            station: st.id,
            district: id,
            destination: Some(self.stations[next].id),
            eta_ticks: travel,
            carriages: cars,
        }));
    }

    // ---- applying commands, as the Java side would ------------------------------------------

    fn apply(&mut self, c: Cmd) {
        let goods = self.goods.clone();
        match c {
            Cmd::ChunkReconcile(rc) => {
                let Some(di) = self.districts.iter().position(|d| d.id == rc.district) else {
                    return;
                };
                if !self.chunk_loaded(di, rc.chunk) {
                    self.districts[di].held.push(rc);
                    return;
                }
                let applied_epoch = self.districts[di]
                    .epochs
                    .get(&rc.chunk)
                    .copied()
                    .unwrap_or(0);
                if rc.epoch <= applied_epoch || self.applied.contains(&rc.op) {
                    self.answer(rc.op, RESULT_DUPLICATE);
                    return;
                }
                let mut entries = Vec::new();
                // Removals first, then additions (I8).
                for pass in [false, true] {
                    for dl in rc.deltas.iter().filter(|x| (x.delta > 0) == pass) {
                        if dl.inventory == crate::district::DEPOSIT_INVENTORY {
                            let mut applied = 0;
                            if let Some((g, left)) = self.districts[di].deposit.as_mut()
                                && *g == dl.good
                            {
                                // A vein never goes below empty.
                                applied = dl.delta.max(-*left);
                                *left += applied;
                            }
                            entries.push(ResultEntry {
                                inventory: dl.inventory,
                                good: dl.good,
                                requested: dl.delta,
                                applied,
                            });
                            continue;
                        }
                        let d = &mut self.districts[di];
                        let Some(fi) = d.inventories.iter_mut().find(|i| i.index == dl.inventory)
                        else {
                            continue;
                        };
                        let applied = if fi.destroyed {
                            0
                        } else if dl.delta < 0 {
                            -(fi.contents.get(dl.good).min(-dl.delta))
                        } else {
                            let st = inv_state(fi, fi.contents.clone());
                            room_for(&st, &fi.contents, dl.good, &goods).min(dl.delta)
                        };
                        fi.contents.add(dl.good, applied);
                        entries.push(ResultEntry {
                            inventory: dl.inventory,
                            good: dl.good,
                            requested: dl.delta,
                            applied,
                        });
                    }
                }
                let (mut destroyed, mut skipped) = (Vec::new(), 0);
                for b in &rc.destroy {
                    let d = &mut self.districts[di];
                    let present = d
                        .exposed
                        .iter()
                        .any(|e| e.pos == b.pos && e.state == b.expected_state)
                        && !d.broken.contains(&b.pos);
                    if !present {
                        skipped += 1;
                        continue;
                    }
                    destroyed.push(b.pos);
                    d.broken.insert(b.pos);
                    if let Some(fi) = d
                        .inventories
                        .iter_mut()
                        .find(|i| i.pos == b.pos && !i.destroyed)
                    {
                        fi.destroyed = true;
                        let spill = std::mem::take(&mut fi.contents);
                        self.books.dropped.merge(&spill);
                    }
                }
                self.districts[di].epochs.insert(rc.chunk, rc.epoch);
                self.applied.insert(rc.op);
                self.reconciled
                    .push((rc.op, rc.district, rc.epoch, rc.chunk));
                let id = self.districts[di].id;
                self.send(Obs::ReconcileResult(ReconcileResult {
                    op: rc.op,
                    district: id,
                    epoch: rc.epoch,
                    chunk: rc.chunk,
                    entries,
                    destroyed: destroyed.clone(),
                    blocks_skipped: skipped,
                }));
                self.answer(rc.op, RESULT_APPLIED);
                if !destroyed.is_empty() {
                    let name = self.districts[di].name.clone();
                    self.note(format!(
                        "{name}: {} blocks destroyed on load",
                        destroyed.len()
                    ));
                }
            }
            Cmd::CarriageDelta(cd) => {
                if self.fail_carriage_deltas {
                    self.answer(cd.op, RESULT_FAILED);
                    return;
                }
                if !self.applied.insert(cd.op) {
                    self.answer(cd.op, RESULT_DUPLICATE);
                    return;
                }
                let Some(ti) = self.trains.iter().position(|t| t.id == cd.train) else {
                    self.answer(cd.op, RESULT_FAILED);
                    return;
                };
                let mut entries = Vec::new();
                for &(ci, g, delta) in &cd.entries {
                    let car = self.trains[ti]
                        .carriages
                        .iter_mut()
                        .find(|c| c.index == ci)
                        .expect("carriage");
                    let applied = if delta < 0 {
                        -(car.cargo.get(g).min(-delta))
                    } else {
                        let cs = InvState {
                            index: 0,
                            pos: BlockPos::default(),
                            chunk: ChunkPos::default(),
                            kind: InvKind(InvKind::ITEMS),
                            item_slots: car.slots,
                            fluid_capacity: 0,
                            energy_capacity: 0,
                            contents: car.cargo.clone(),
                            received: BTreeSet::new(),
                        };
                        room_for(&cs, &car.cargo, g, &goods).min(delta)
                    };
                    car.cargo.add(g, applied);
                    entries.push(CarriageResultEntry {
                        carriage: ci,
                        good: g,
                        requested: delta,
                        applied,
                    });
                }
                self.send(Obs::CarriageResult(CarriageResult {
                    op: cd.op,
                    train: cd.train,
                    entries,
                }));
                self.answer(cd.op, RESULT_APPLIED);
            }
            // Level SavedData, applied at once (docs/industry-boundary.md §4.3). The fake world
            // has no Diesel deposits or postbox buffers: they are taken as applied.
            Cmd::DepositDelta(c) => {
                let status = if self.applied.insert(c.op) {
                    RESULT_APPLIED
                } else {
                    RESULT_DUPLICATE
                };
                self.answer(c.op, status);
            }
            Cmd::WritePackage(c) => {
                let status = if self.applied.insert(c.op) {
                    RESULT_APPLIED
                } else {
                    RESULT_DUPLICATE
                };
                self.answer(c.op, status);
            }
            Cmd::ProbeSet(p) => self.probes.push(p),
        }
    }

    /// Goods brought into a loaded district's inventory from outside (a player's hands, a train the
    /// script does not model): a port, so learning never reads it as production.
    pub fn import(&mut self, di: usize, inv: u32, g: Good, n: i64) {
        let id = self.districts[di].id;
        self.districts[di].inv(inv).contents.add(g, n);
        self.books.imported.add(g, n);
        self.send(Obs::Port(PortTransfer {
            district: id,
            kind: PortKind::Other,
            inventory: Some(inv),
            counterpart: 0,
            op: None,
            goods: GoodVec::from_pairs([(g, n)]),
        }));
    }

    /// The player edits a district (a repair after an attack, say): it goes stale and relearns.
    pub fn repair_all(&mut self, di: usize) {
        if self.districts[di].broken.is_empty() {
            return;
        }
        self.districts[di].broken.clear();
        for i in &mut self.districts[di].inventories {
            i.destroyed = false;
        }
        let id = self.districts[di].id;
        let pos = self.districts[di]
            .recipes
            .first()
            .map_or(BlockPos::default(), |r| r.machine);
        self.send(Obs::Edit {
            district: id,
            kind: crate::obs::EditKind::Placed,
            player: (1, 1),
            pos,
        });
        let name = self.districts[di].name.clone();
        self.note(format!("the player repairs {name}"));
    }

    /// The end-to-end conservation check: what the world holds now against what it started with,
    /// what its own machines did, and what the engine booked. Call it with every district loaded and
    /// every command applied.
    pub fn conservation(&self) -> Result<(), String> {
        self.conservation_with(&self.engine)
    }

    /// [`FakeWorld::conservation`] against an engine elsewhere (a remote world's).
    pub fn conservation_with(&self, engine: &Engine) -> Result<(), String> {
        let now = self.totals();
        let b = &engine.books;
        let mut want = self.initial.clone();
        want.merge(&self.books.minted);
        want.subtract(&self.books.burned);
        want.merge(&self.books.imported);
        want.merge(&b.minted);
        want.subtract(&b.burned);
        want.subtract(&b.stolen);
        want.merge(&b.shortfall);
        want.subtract(&b.trimmed);
        let mut owed = GoodVec::new();
        for d in engine.districts() {
            owed.merge(&d.owed);
            for inv in d.inventories.values() {
                owed.merge(&inv.pending);
                owed.merge(&inv.held);
            }
        }
        want.subtract(&owed);
        let diff = now.minus(&want);
        if diff.is_empty() {
            Ok(())
        } else {
            let names: Vec<String> = diff
                .iter()
                .map(|(g, n)| {
                    format!(
                        "{} {n:+}",
                        self.goods
                            .get(&g)
                            .map_or(format!("good {}", g.0), |i| i.name.clone())
                    )
                })
                .collect();
            Err(format!("world totals off by: {}", names.join(", ")))
        }
    }
}

fn inv_state(fi: &FInventory, contents: GoodVec) -> InvState {
    InvState {
        index: fi.index,
        pos: fi.pos,
        chunk: ChunkPos::default(),
        kind: InvKind(fi.kind),
        item_slots: fi.slots,
        fluid_capacity: fi.fluid_cap,
        energy_capacity: fi.energy_cap,
        contents,
        received: BTreeSet::new(),
    }
}

/// An exposed block row for scenarios.
pub fn exposed(pos: BlockPos, state: u32, flags: u32, inventory: Option<u32>) -> ExposedBlock {
    ExposedBlock {
        pos,
        state,
        flags: BlockFlags(flags),
        inventory,
    }
}

/// A defence survey for scenarios.
pub fn survey(
    d: DistrictId,
    dark: u32,
    gaps: u32,
    columns: u32,
    roofed: u32,
    machines: u32,
    turrets: [f32; 4],
) -> DefenceSurvey {
    DefenceSurvey {
        district: d,
        revision: 1,
        mob_griefing: true,
        perimeter_columns: columns,
        perimeter_gaps: gaps,
        dark_spawnable: dark,
        machine_blocks: machines,
        roofed_machines: roofed,
        kill_machines_at_gaps: 0,
        turret_strength: turrets,
        guards: [0; ThreatClass::COUNT],
        ammo_good: None,
        ammo_per_engagement: 0,
    }
}

/// Item goods only; the scenarios use these.
pub fn is_item(goods: &BTreeMap<Good, GoodInfo>, g: Good) -> bool {
    goods.get(&g).is_none_or(|i| i.class == GoodClass::Item)
}
