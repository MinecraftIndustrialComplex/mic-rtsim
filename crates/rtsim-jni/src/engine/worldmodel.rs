// SPDX-License-Identifier: AGPL-3.0-or-later
//! The world model behind the boundary (docs/boundary.md §10; choices X2-# in
//! docs/overnight-choices-worldmodel.md): the core's habitat sampler reports 64-block cells
//! (HABITAT, latest wins), the engine keeps them in rtsim-space's [`SampledWorld`], and every
//! module that needs the ground reads it through one [`WorldHandle`]:
//!
//! - **the herds**: their climate (`HabitatClimate`: forage, temperature, terrain; water and
//!   unknown ground closed) and their founding: one herd per suitable region on its first
//!   sample ([`simulate`], Y4's recommendation, X2-11);
//! - **the ships**: their water mask (`HabitatWater`), replacing the all-water mask (U11).
//!
//! The step calls this module at its observe (HABITAT is a latest-wins record), simulate
//! (founding, before the herds run), write (HABITAT_KNOWN) and save points.
//!
//! - **Sent once** (X2-3): at a session start the engine tells Java what it holds,
//!   HABITAT_KNOWN per region (each cell's fingerprint) and a last record flagged END; Java
//!   samples a cell again only when its terrain's fingerprint differs from the engine's, so a
//!   cell crosses the boundary once per change, and a cell the engine lost (pages older than
//!   the world save) is sent again when its chunks next load.
//! - **Pages** (X2-15): table 0x5700 holds the meta page (name tables, state hash), table
//!   0x5701 one page per region; a save writes the meta page and the regions changed since the
//!   last save. Pages this build cannot read are dropped loudly and the world model starts
//!   empty: Java resends every cell as its chunks load.
//! - **No module switch**: the world model is a core service; founding herds follows the herds'
//!   switch (`Module.ECOLOGY`).

use std::collections::{BTreeSet, VecDeque};

use rtsim_ecology::herds::habitat::{
    FoundingTunables, HabitatClimate, RegionPlan, plan_region, region_herd_name,
};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{PageKey, PageStore, SaveBatch};
use rtsim_space::habitat::{DICT_BIOME, HABITAT, HabitatRecord, KNOWN_END, KnownRecord};
use rtsim_space::sampled::mark;
use rtsim_space::{RegionKey, SampledWorld, WorldHandle};

use super::{State, herds, log_error, log_info, log_warn, phase1};
use crate::wire::{self, Rec, RecordWriter};

/// The world model's pages: the meta page in table 0x5700 ('W'), regions in 0x5701.
pub const TABLE_META: u16 = 0x5700;
pub const TABLE_REGIONS: u16 = 0x5701;
pub const KEY_META: PageKey = PageKey::new(TABLE_META, 0);

/// The founding draws' seed. PLACEHOLDER: the bridge does not send the Minecraft world's seed (as
/// the herds' HERD_SEED).
pub const FOUNDING_SEED: u64 = 0x6d69_635f_666f_756e;

/// Regions judged for founding per step at most (the rest wait for the next step). PLACEHOLDER.
pub const FOUNDING_PER_STEP: usize = 64;

/// The world model's state the step keeps (`State::worldmodel`).
pub(super) struct Link {
    pub(super) world: WorldHandle,
    /// HABITAT_KNOWN records not written yet (per session). Their `dimension` is Java's id
    /// until written, then the engine's dictionary id.
    known: VecDeque<KnownRecord>,
    /// Regions with new cells, waiting to be judged for a herd.
    pending: BTreeSet<RegionKey>,
    pub(super) founding: FoundingTunables,
    /// The last load dropped pages it could not read.
    pub(super) dropped_pages: bool,
    /// The last load read back a state hash other than the one saved: (saved, read).
    pub(super) restore_diverged: Option<(u64, u64)>,
    pub(super) records: u64,
    /// HABITAT records of cells past a turned seam (copies), dropped (TP17).
    pub(super) seam_copies: u64,
    /// HABITAT records that did not decode or named what Java's dictionary lacks.
    pub(super) refused: u64,
    pub(super) commands_written: u64,
    pub(super) herds_founded: u64,
    pub(super) regions_barren: u64,
    /// Save failures seen at the last snapshot: a new one means a save may not have landed, so
    /// every region is written again.
    failures_seen: u64,
}

/// **The policy for world-model pages this build cannot read** (as the other modules'): dropped
/// with a loud log and deleted at once (a stray region page would fail every later load), the
/// world model starts empty, and Java resends every cell as its chunks load.
fn when_pages_unreadable(why: &str, store: &mut PageStore) -> Link {
    log_error(&format!(
        "WORLD MODEL PAGES DROPPED: {why}. The world model starts empty; the bridge resends each cell as its chunks load"
    ));
    let mut batch = SaveBatch::new();
    for k in store
        .keys()
        .filter(|k| k.table() == TABLE_META || k.table() == TABLE_REGIONS)
        .collect::<Vec<_>>()
    {
        batch.delete(k);
    }
    let at = store.obs_seq();
    if let Err(e) = store.commit(batch, at) {
        log_error(&format!(
            "cannot delete the unreadable world-model pages: {e}"
        ));
    }
    let mut l = Link::empty();
    l.dropped_pages = true;
    l
}

impl Link {
    pub(super) fn empty() -> Link {
        Link {
            world: WorldHandle::sampled(),
            known: VecDeque::new(),
            pending: BTreeSet::new(),
            founding: FoundingTunables::default(),
            dropped_pages: false,
            restore_diverged: None,
            records: 0,
            refused: 0,
            seam_copies: 0,
            commands_written: 0,
            herds_founded: 0,
            regions_barren: 0,
            failures_seen: 0,
        }
    }

    /// Loads the world model's pages.
    pub(super) fn load(store: &mut PageStore) -> Link {
        let meta = match store.get(KEY_META) {
            Ok(Some(b)) => b,
            Ok(None) => {
                let stray = store.keys().any(|k| k.table() == TABLE_REGIONS);
                if stray {
                    return when_pages_unreadable("region pages without their meta page", store);
                }
                return Link::empty();
            }
            Err(e) => {
                return when_pages_unreadable(&format!("cannot read the meta page: {e}"), store);
            }
        };
        let keys: Vec<PageKey> = store
            .keys()
            .filter(|k| k.table() == TABLE_REGIONS)
            .collect();
        let mut pages = Vec::with_capacity(keys.len());
        for k in keys {
            match store.get(k) {
                Ok(Some(b)) => pages.push(b),
                Ok(None) => {}
                Err(e) => {
                    return when_pages_unreadable(
                        &format!("cannot read a region page: {e}"),
                        store,
                    );
                }
            }
        }
        match SampledWorld::load(&meta, pages.iter().map(|b| b.as_slice())) {
            Ok((w, saved)) => {
                let hash = w.state_hash();
                let mut l = Link::empty();
                if hash != saved {
                    log_warn(&format!(
                        "the world model read back to state {hash:016x}, the save had {saved:016x}"
                    ));
                    l.restore_diverged = Some((saved, hash));
                }
                log_info(&format!(
                    "world model restored: {} cells in {} regions, state {hash:016x}",
                    w.cell_count(),
                    w.region_count()
                ));
                l.world = WorldHandle::new(Box::new(w));
                l
            }
            Err(e) => when_pages_unreadable(&format!("the world model's pages: {e}"), store),
        }
    }
}

// ---- hooks --------------------------------------------------------------------------------------

/// SESSION_START: Java forgot what the engine holds; tell it again (HABITAT_KNOWN, then END).
pub(super) fn session_start(st: &mut State) {
    let l = &mut st.worldmodel;
    l.known.clear();
    {
        let w = l.world.read();
        if let Some(s) = w.sampled_ref() {
            for (k, cells) in s.known_lists() {
                l.known.push_back(KnownRecord {
                    flags: 0,
                    dimension: u32::from(k.frame),
                    region: (k.rx, k.rz),
                    cells,
                });
            }
        }
    }
    l.known.push_back(KnownRecord {
        flags: KNOWN_END,
        dimension: 0,
        region: (0, 0),
        cells: Vec::new(),
    });
}

/// A latest-wins record: HABITAT.
pub(super) fn observe_state(st: &mut State, rec: &Rec<'_>) {
    if rec.kind != HABITAT {
        return;
    }
    let mut r = match HabitatRecord::decode(rec.bytes) {
        Ok(r) => r,
        Err(e) => {
            st.worldmodel.refused += 1;
            log_warn(&format!("a HABITAT record does not decode ({e}); skipped"));
            return;
        }
    };
    let dict = &st.world.java_dict;
    let name = |kind: u32, id: u32| -> Option<String> {
        if id == 0 {
            Some(String::new())
        } else {
            dict.get(&(kind, id)).cloned()
        }
    };
    let biome = name(DICT_BIOME, r.biome);
    let land = name(DICT_BIOME, r.land_biome);
    let bodies: Vec<String> = r
        .creatures
        .iter()
        .map(|c| name(wire::DICT_BODY, c.0).unwrap_or_default())
        .collect();
    let (Some(biome), Some(land)) = (biome, land) else {
        st.worldmodel.refused += 1;
        log_warn("a HABITAT record names a biome Java never named; skipped");
        return;
    };
    let refs: Vec<&str> = bodies.iter().map(|s| s.as_str()).collect();
    // Cells are keyed canonically (TP4): a cell past a slid edge is its canonical cell moved; one
    // past a turned edge (the folded copy Deep Time generates beyond a north or south edge) is a
    // copy of a cell the sampler reports on its own, and is dropped (TP17).
    let topo = st.topology.shared.clone();
    let t = topo.of(r.dimension as u16);
    if !t.is_flat() {
        let c = i64::from(rtsim_space::habitat::CELL_BLOCKS);
        let (cx, cz, k) = t.grid(c).fold(i64::from(r.cell.0), i64::from(r.cell.1));
        if k != 0 {
            st.worldmodel.seam_copies += 1;
            return;
        }
        r.cell = (cx as i32, cz as i32);
    }
    st.worldmodel.records += 1;
    let mut w = st.worldmodel.world.write();
    match w.sampled() {
        Some(s) => {
            s.ingest(&r, &biome, &land, &refs);
        }
        None => st.worldmodel.refused += 1,
    }
}

/// Before the herds run: regions with new cells are judged for a herd (one per suitable region,
/// once: the region is marked when decided). Only while the herds' switch is on; regions wait
/// meanwhile.
pub(super) fn simulate(st: &mut State) {
    {
        let mut w = st.worldmodel.world.write();
        if let Some(s) = w.sampled() {
            let changed = s.take_changed();
            st.worldmodel.pending.extend(changed);
        }
    }
    if st.worldmodel.pending.is_empty() || !herds::enabled(st) {
        return;
    }
    let batch: Vec<RegionKey> = st
        .worldmodel
        .pending
        .iter()
        .copied()
        .take(FOUNDING_PER_STEP)
        .collect();
    let climate = HabitatClimate::new(st.worldmodel.world.clone());
    for k in batch {
        st.worldmodel.pending.remove(&k);
        let plan = {
            let w = st.worldmodel.world.read();
            if w.sampled_ref()
                .is_some_and(|s| s.marks(k) & mark::HERDS != 0)
            {
                continue;
            }
            if herds::has_herd_named(st, &region_herd_name(k)) {
                RegionPlan::Barren // founded before a crash took the mark: decided
            } else {
                plan_region(&**w, &climate, k, FOUNDING_SEED, &st.worldmodel.founding)
            }
        };
        let decided = match plan {
            RegionPlan::NotYet => false,
            RegionPlan::Barren => {
                st.worldmodel.regions_barren += 1;
                true
            }
            RegionPlan::Found(n) => {
                let at = Tick(phase1::world_tick(st, st.game_time));
                herds::found(st, n, at);
                st.worldmodel.herds_founded += 1;
                true
            }
        };
        if decided && let Some(s) = st.worldmodel.world.write().sampled() {
            s.set_mark(k, mark::HERDS);
        }
    }
}

/// Writes the queued HABITAT_KNOWN records while they fit. Returns the highest sequence written
/// and whether some wait for the next buffer.
pub(super) fn write_queue(st: &mut State, w: &mut RecordWriter<'_>) -> (u64, bool) {
    let mut highest = 0;
    while let Some(k) = st.worldmodel.known.front().cloned() {
        let dim_name = if k.flags & KNOWN_END != 0 {
            None
        } else {
            match st.world.java_dict.get(&(wire::DICT_DIMENSION, k.dimension)) {
                Some(n) => Some(n.clone()),
                None => {
                    // Java names every dimension it ever used at each session start; one it
                    // does not name has no chunks to sample this session.
                    st.worldmodel.known.pop_front();
                    continue;
                }
            }
        };
        let names: Vec<(u32, &str)> = dim_name
            .iter()
            .map(|n| (wire::DICT_DIMENSION, n.as_str()))
            .collect();
        let need = st.dict_bytes(&names) + k.size();
        if need > w.remaining() {
            return (highest, true);
        }
        let dim = dim_name.map_or(0, |n| st.dict_id(w, wire::DICT_DIMENSION, &n));
        let seq = st.session_seq();
        let mut rec = Vec::with_capacity(k.size());
        KnownRecord {
            dimension: dim,
            ..k
        }
        .encode(seq, &mut rec);
        let written = w.copy(&rec);
        debug_assert!(written, "checked to fit");
        highest = highest.max(seq);
        st.worldmodel.commands_written += 1;
        st.worldmodel.known.pop_front();
    }
    (highest, false)
}

// ---- saves --------------------------------------------------------------------------------------

/// The world model's pages for a save at this step boundary: the meta page and the regions
/// changed since the last save (every region after a save failure or dropped pages).
/// `save_failures` is the engine's count so far.
pub(super) fn snapshot(st: &mut State, save_failures: u64) -> Vec<(PageKey, Vec<u8>)> {
    let l = &mut st.worldmodel;
    let all = l.dropped_pages || save_failures > l.failures_seen;
    l.failures_seen = save_failures;
    l.dropped_pages = false;
    let mut w = l.world.write();
    let Some(s) = w.sampled() else {
        return Vec::new();
    };
    if s.cell_count() == 0 {
        return Vec::new();
    }
    let (meta, regions) = s.save_pages(all);
    let mut pages = Vec::with_capacity(regions.len() + 1);
    pages.push((KEY_META, meta));
    for (id, b) in regions {
        pages.push((PageKey::new(TABLE_REGIONS, id), b));
    }
    pages
}

// ---- inspection ---------------------------------------------------------------------------------

/// [`super::Engine::habitat_info`]'s fields, in order.
pub const HABITAT_INFO: usize = 16;

impl super::Engine {
    /// The world model at block (x, z) of Java's dimension `dim` ([`HABITAT_INFO`] values): 1
    /// when the cell is known, its known, water, ice and land masks, its mean surface y, its
    /// fingerprint, the cells and regions known, the agent of the herd founded for the region (0
    /// none), the region's marks, the HABITAT records taken, the herds founded, the
    /// HABITAT_KNOWN records written, and the region's herd's position (x, z) now.
    pub fn habitat_info(&self, dim: u32, x: i32, z: i32) -> [i64; HABITAT_INFO] {
        let st = super::lock(&self.state);
        let w = st.worldmodel.world.read();
        let frame = dim as u16;
        let k = RegionKey::of_block(frame, x, z);
        let h = &st.herds.engine;
        let found = h.by_name(&region_herd_name(k));
        let herd = found.map_or(0, |id| rtsim_ecology::herds::herd::agent_of(id) as i64);
        let herd_pos = found.map_or((0, 0), |id| h.position(id, h.now()));
        let (cells, regions, marks) = w.sampled_ref().map_or((0, 0, 0), |s| {
            (
                s.cell_count() as i64,
                s.region_count() as i64,
                i64::from(s.marks(k)),
            )
        });
        let mut out = [0i64; HABITAT_INFO];
        if let Some(h) = w.habitat_at(frame, x, z) {
            out[0] = 1;
            out[1] = i64::from(h.known);
            out[2] = i64::from(h.water);
            out[3] = i64::from(h.ice);
            out[4] = i64::from(h.land());
            out[5] = i64::from(h.surface_mean);
            out[6] = i64::from(h.fingerprint);
        }
        out[7] = cells;
        out[8] = regions;
        out[9] = herd;
        out[10] = marks;
        out[11] = st.worldmodel.records as i64;
        out[12] = st.worldmodel.herds_founded as i64;
        out[13] = st.worldmodel.commands_written as i64;
        out[14] = herd_pos.0;
        out[15] = herd_pos.1;
        out
    }

    /// Runs `f` on the world model, under the state lock (tests and debugging, between steps).
    pub fn with_world_model<R>(&self, f: impl FnOnce(&dyn rtsim_space::WorldModel) -> R) -> R {
        let st = super::lock(&self.state);
        let w = st.worldmodel.world.read();
        f(&**w)
    }

    /// Whether the last start read the world model back to the state hash it was saved with
    /// (`None`), or (saved, read); and whether it dropped its pages.
    pub fn world_model_restore_check(&self) -> (Option<(u64, u64)>, bool) {
        let st = super::lock(&self.state);
        (st.worldmodel.restore_diverged, st.worldmodel.dropped_pages)
    }
}
