// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`load_world`]: a world's `rtsim/` pages opened read only and every module decoded as the
//! native link's start decodes it (VIEWER.md §5.3), with the configuration rebuilt from the code's
//! defaults. Nothing is written: the store is opened with `PageStore::open_read_only`, shows the
//! confirmed generation (what the next start would open), and a module whose pages do not read is
//! reported and left out, never dropped or rewritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rtsim_conflict::{ConflictTunables, Cultures};
use rtsim_ecology::herds::habitat::HabitatClimate;
use rtsim_ecology::herds::{HerdTunables, Herds};
use rtsim_ecology::{DragonTunables, Dragons};
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader};
use rtsim_kernel::page::{Generation, PageKey, PageStore};
use rtsim_kernel::rows::PageSource;
use rtsim_ships::{ShipTunables, Ships};
use rtsim_space::{SampledWorld, WorldHandle};
use rtsim_world::inputs::{self, Logged};
use rtsim_world::{World, WorldConfig};

use crate::industry::IndustryView;
use crate::journal::Names;
use crate::keys::{self, phase1 as p1};

/// What the viewer may hold. Every value is a TUNABLE.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Resident row bytes per paged module (the link's `DEFAULT_ROW_BUDGET`).
    pub row_budget: usize,
    /// Bytes of built cards kept for repeat requests (VIEWER.md §16: 256 MiB).
    pub card_cache: u64,
    /// Bytes of encoded base map tiles kept in memory (halved between HABITAT and Surveyor when a
    /// world has both).
    pub tile_cache: u64,
    /// Bytes of Surveyor region pictures kept in memory, shared by the frames that have them.
    pub surveyor_cache: u64,
    /// Heap bytes above which a card is not decoded (it answers with a note instead). `None`:
    /// no probe (the library cannot see the heap; rtsim-cli passes its allocator's count).
    pub heap_cap: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            row_budget: 4 << 20,
            card_cache: 256 << 20,
            tile_cache: 64 << 20,
            surveyor_cache: mic_viewer_surveyor::DEFAULT_CACHE_BYTES as u64,
            heap_cap: None,
        }
    }
}

/// How a module's pages read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No pages: the module never ran in this world, or ran off.
    Absent,
    Loaded,
    /// Its pages are there and do not read: left out of the view, untouched on disk.
    Unreadable(String),
}

/// One module's load.
#[derive(Clone, Debug)]
pub struct Module {
    pub name: &'static str,
    pub status: Status,
    /// What it holds, in a line (counts).
    pub detail: String,
    /// The state hash read back, and the one a TEST save wrote beside it.
    pub hash: Option<(u64, Option<u64>)>,
}

/// The phase 1 world as the link restores it: its snapshot, then the input tail replayed.
pub struct Phase1 {
    pub world: World,
    /// World tick minus game time (the link's test offset; 0 in a real world).
    pub offset: u64,
    /// Village key to node index.
    pub nodes: BTreeMap<u64, usize>,
    /// The state hash a TEST save wrote (0: none).
    pub saved_hash: u64,
    /// Inputs replayed after the snapshot.
    pub replayed: usize,
}

impl Phase1 {
    /// The game tick of world tick `t`.
    pub fn game(&self, t: u64) -> i64 {
        t.saturating_sub(self.offset) as i64
    }
}

/// A saved world, decoded.
pub struct Saved {
    pub world_dir: PathBuf,
    pub pages_dir: PathBuf,
    pub source: Arc<PageStore>,
    /// The generation shown (the confirmed one).
    pub generation: Generation,
    /// A save the world never confirmed was in the folder: not shown, as the next start drops it.
    pub unconfirmed_hidden: bool,
    pub pages: usize,
    pub names: Names,
    pub phase1: Option<Phase1>,
    pub world_model: Option<WorldHandle>,
    pub dragons: Option<Dragons>,
    pub herds: Option<Herds>,
    pub ships: Option<Ships>,
    pub conflict: Option<rtsim_conflict::Conflict>,
    pub industry: Option<IndustryView>,
    pub modules: Vec<Module>,
    pub limits: Limits,
}

impl Saved {
    /// The save's game tick: the phase 1 world's, else the industry engine's.
    pub fn now(&self) -> i64 {
        if let Some(p) = &self.phase1 {
            return p.game(p.world.now().0);
        }
        self.industry.as_ref().map_or(0, |i| i.now)
    }

    /// The game tick of a world tick (the phase 1 clock the other modules share).
    pub fn game(&self, t: Tick) -> i64 {
        self.phase1.as_ref().map_or(t.0 as i64, |p| p.game(t.0))
    }

    pub fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }

    /// Every module's state hash as it reads now (the determinism check, VWD-11).
    pub fn state_hashes(&self) -> Vec<(&'static str, u64)> {
        let mut out = Vec::new();
        if let Some(p) = &self.phase1 {
            out.push(("phase1", p.world.state_hash()));
        }
        if let Some(w) = &self.world_model
            && let Some(s) = w.read().sampled_ref()
        {
            out.push(("world_model", s.state_hash()));
        }
        if let Some(d) = &self.dragons {
            out.push(("dragons", d.state_hash()));
        }
        if let Some(h) = &self.herds {
            out.push(("herds", h.state_hash()));
        }
        if let Some(s) = &self.ships {
            out.push(("ships", s.state_hash()));
        }
        if let Some(c) = &self.conflict {
            out.push(("conflict", c.state_hash()));
        }
        if let Some(i) = &self.industry {
            out.push(("industry", i.pages_hash));
        }
        out
    }
}

/// The pages folder of a world folder (`<world>/rtsim`), or the folder itself if it is one.
pub fn pages_dir(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let inner = dir.join("rtsim");
    if inner.join("manifest.rtm").is_file() {
        return Some((dir.to_path_buf(), inner));
    }
    if dir.join("manifest.rtm").is_file() {
        let world = dir
            .parent()
            .map_or_else(|| dir.to_path_buf(), Path::to_path_buf);
        return Some((world, dir.to_path_buf()));
    }
    None
}

/// Opens `dir` (a world folder, or its `rtsim/` folder) read only and decodes every module.
/// Errors only when there is no page store to open; a module that does not read is reported in
/// [`Saved::modules`].
pub fn load_world(dir: &Path, limits: Limits) -> Result<Saved, String> {
    let (world_dir, pages) = pages_dir(dir).ok_or_else(|| {
        format!(
            "{} holds no rtsim pages (no rtsim/manifest.rtm)",
            dir.display()
        )
    })?;
    let mut store = PageStore::open_read_only(&pages)
        .map_err(|e| format!("cannot open {} read only: {e}", pages.display()))?;
    let unconfirmed_hidden = store
        .view_confirmed()
        .map_err(|e| format!("cannot show the confirmed save: {e}"))?;
    let generation = store.current();
    let n_pages = store.len();
    let store = Arc::new(store);
    let source: Arc<dyn PageSource> = store.clone();
    let budget = limits.row_budget;
    let names = Names::read(&world_dir);
    let mut modules = Vec::new();

    let phase1 = match read_phase1(&source, budget) {
        Ok(None) => {
            modules.push(module("phase1", Status::Absent, String::new(), None));
            None
        }
        Ok(Some(p)) => {
            let hash = (p.saved_hash != 0).then(|| (p.world.state_hash(), Some(p.saved_hash)));
            let detail = format!(
                "{} nodes, {} edges, {} caravans, {} bands, {} packets; {} inputs replayed",
                p.world.nodes.len(),
                p.world.edges.len(),
                p.world.caravans.len(),
                p.world.bands.len(),
                p.world.packets.len(),
                p.replayed
            );
            modules.push(module("phase1", Status::Loaded, detail, hash));
            Some(p)
        }
        Err(why) => {
            modules.push(module(
                "phase1",
                Status::Unreadable(why),
                String::new(),
                None,
            ));
            None
        }
    };
    let world_restored = phase1.is_some();
    let lives = "the phase 1 world they live in did not come back from its pages";

    let world_model = match read_world_model(&*source) {
        Ok(None) => {
            modules.push(module("world_model", Status::Absent, String::new(), None));
            None
        }
        Ok(Some((w, saved))) => {
            let hash = w.state_hash();
            let detail = format!("{} cells in {} regions", w.cell_count(), w.region_count());
            modules.push(module(
                "world_model",
                Status::Loaded,
                detail,
                Some((hash, Some(saved))),
            ));
            Some(WorldHandle::new(Box::new(w)))
        }
        Err(why) => {
            modules.push(module(
                "world_model",
                Status::Unreadable(why),
                String::new(),
                None,
            ));
            None
        }
    };

    let paged = |name: &'static str, key: PageKey| -> Result<bool, String> {
        match source.read(key) {
            Err(e) => Err(format!("cannot read the {name} meta page: {e}")),
            Ok(None) if holds_pages(&*source, key.table()) => {
                Err(format!("{name} pages without their meta page"))
            }
            Ok(None) => Ok(false),
            Ok(Some(_)) if !world_restored => Err(lives.into()),
            Ok(Some(_)) => Ok(true),
        }
    };

    let dragons = match paged("dragons", keys::DRAGONS) {
        Ok(false) => absent(&mut modules, "dragons"),
        Err(why) => unreadable(&mut modules, "dragons", why),
        Ok(true) => match Dragons::open_paged(
            source.clone(),
            budget,
            keys::DRAGONS,
            DragonTunables::default(),
        ) {
            Ok(d) => {
                let saved = saved_hash(&*source, keys::DRAGONS);
                let detail = format!("{} dragons, {} lairs", d.dragons.len(), d.lairs.len());
                let hash = saved.map(|s| (d.state_hash(), Some(s)));
                modules.push(module("dragons", Status::Loaded, detail, hash));
                Some(d)
            }
            Err(e) => unreadable(
                &mut modules,
                "dragons",
                format!("the dragons' meta page: {e}"),
            ),
        },
    };

    let climate_world = world_model.clone().unwrap_or_else(WorldHandle::sampled);
    let herds = match paged("herds", keys::HERDS) {
        Ok(false) => absent(&mut modules, "herds"),
        Err(why) => unreadable(&mut modules, "herds", why),
        Ok(true) => match Herds::open_paged(
            source.clone(),
            budget,
            keys::HERDS,
            HerdTunables::default(),
            Box::new(HabitatClimate::new(climate_world)),
        ) {
            Ok(h) => {
                let saved = saved_hash(&*source, keys::HERDS);
                let detail = format!("{} herds", h.herds.len());
                let hash = saved.map(|s| (h.state_hash(), Some(s)));
                modules.push(module("herds", Status::Loaded, detail, hash));
                Some(h)
            }
            Err(e) => unreadable(&mut modules, "herds", format!("the herds' meta page: {e}")),
        },
    };

    let ships = match paged("ships", keys::SHIPS) {
        Ok(false) => absent(&mut modules, "ships"),
        Err(why) => unreadable(&mut modules, "ships", why),
        Ok(true) => {
            match Ships::open_paged(source.clone(), budget, keys::SHIPS, ShipTunables::default()) {
                Ok(s) => {
                    let saved = saved_hash(&*source, keys::SHIPS);
                    let detail = format!(
                        "{} docks, {} lanes, {} ships, {} havens",
                        s.docks.len(),
                        s.lanes.len(),
                        s.ships.len(),
                        s.havens.len()
                    );
                    let hash = saved.map(|h| (s.state_hash(), Some(h)));
                    modules.push(module("ships", Status::Loaded, detail, hash));
                    Some(s)
                }
                Err(e) => unreadable(&mut modules, "ships", format!("the ships' meta page: {e}")),
            }
        }
    };

    let conflict = match paged("conflict", keys::CONFLICT) {
        Ok(false) => absent(&mut modules, "conflict"),
        Err(why) => unreadable(&mut modules, "conflict", why),
        Ok(true) => match rtsim_conflict::Conflict::open_paged(
            source.clone(),
            budget,
            keys::CONFLICT,
            ConflictTunables::default(),
            Cultures::builtin(),
        ) {
            Ok(Some(c)) => {
                let saved = saved_hash(&*source, keys::CONFLICT);
                let detail = format!(
                    "{} factions, {} feuds, {} parties",
                    c.factions.len(),
                    c.feud_count(),
                    c.parties.len()
                );
                let hash = saved.map(|h| (c.state_hash(), Some(h)));
                modules.push(module("conflict", Status::Loaded, detail, hash));
                Some(c)
            }
            Ok(None) => absent(&mut modules, "conflict"),
            Err(e) => unreadable(
                &mut modules,
                "conflict",
                format!("the conflict's meta page: {e}"),
            ),
        },
    };

    let industry = match IndustryView::load(&*source) {
        Ok(None) => absent(&mut modules, "industry"),
        Ok(Some(v)) => {
            let detail = format!(
                "{} districts, {} stations, {} lines, {} train legs",
                v.districts.len(),
                v.stations.len(),
                v.lines.len(),
                v.legs.len()
            );
            modules.push(module("industry", Status::Loaded, detail, None));
            Some(v)
        }
        Err(why) => unreadable(&mut modules, "industry", why),
    };

    Ok(Saved {
        world_dir,
        pages_dir: pages,
        source: store,
        generation,
        unconfirmed_hidden,
        pages: n_pages,
        names,
        phase1,
        world_model,
        dragons,
        herds,
        ships,
        conflict,
        industry,
        modules,
        limits,
    })
}

fn module(
    name: &'static str,
    status: Status,
    detail: String,
    hash: Option<(u64, Option<u64>)>,
) -> Module {
    Module {
        name,
        status,
        detail,
        hash,
    }
}

fn absent<T>(modules: &mut Vec<Module>, name: &'static str) -> Option<T> {
    modules.push(module(name, Status::Absent, String::new(), None));
    None
}

fn unreadable<T>(modules: &mut Vec<Module>, name: &'static str, why: String) -> Option<T> {
    modules.push(module(name, Status::Unreadable(why), String::new(), None));
    None
}

/// Whether the store holds any page of the module whose meta page is in `table`.
fn holds_pages(source: &dyn PageSource, table: u16) -> bool {
    keys::tables(table).any(|t| {
        source
            .table_ids(t)
            .map(|ids| !ids.is_empty())
            .unwrap_or(true)
    })
}

/// The state hash a TEST save wrote beside a meta page.
fn saved_hash(source: &dyn PageSource, meta: PageKey) -> Option<u64> {
    let b = source.read(keys::hash_key(meta)).ok()??;
    let mut r = Reader::new(&b);
    (r.u32().ok()? == keys::HASH_MAGIC).then(|| r.u64().ok())?
}

/// The phase 1 link's meta page: what the viewer needs of it (the rest is the link's session
/// state: scripted villages, the kept outbox, agents, queued commands).
struct Meta {
    now: u64,
    hash: u64,
    offset: u64,
    log_len: usize,
    snap_pieces: usize,
    snap_len: usize,
    snap_at: u64,
}

fn decode_meta(b: &[u8]) -> Result<Meta, CodecError> {
    let mut r = Reader::new(b);
    if r.u32()? != p1::META_MAGIC {
        return Err(CodecError::Invalid("not a phase 1 meta page".into()));
    }
    let v = r.u16()?;
    if v != p1::META_VERSION {
        return Err(CodecError::Invalid(format!(
            "phase 1 meta page version {v}, this build reads {}",
            p1::META_VERSION
        )));
    }
    let now = r.u64()?;
    let hash = r.u64()?;
    let offset = r.u64()?;
    let _next_epoch = r.u32()?;
    let log_len = r.var_u64()? as usize;
    let snap_pieces = r.var_u64()? as usize;
    let snap_len = r.var_u64()? as usize;
    let snap_at = r.u64()?;
    let _base_inputs = r.u64()?;
    if snap_len.div_ceil(p1::PIECE) != snap_pieces {
        return Err(CodecError::Invalid(format!(
            "a snapshot of {snap_len} bytes in {snap_pieces} pages"
        )));
    }
    Ok(Meta {
        now,
        hash,
        offset,
        log_len,
        snap_pieces,
        snap_len,
        snap_at,
    })
}

fn decode_snapshot(b: &[u8]) -> Result<BTreeMap<u64, usize>, CodecError> {
    let mut r = Reader::new(b);
    if r.u32()? != p1::SNAP_MAGIC {
        return Err(CodecError::Invalid("not a phase 1 snapshot".into()));
    }
    let mut nodes = BTreeMap::new();
    for _ in 0..r.len_prefix(9)? {
        let k = r.u64()?;
        nodes.insert(k, r.var_u64()? as usize);
    }
    for _ in 0..r.len_prefix(8)? {
        r.u64()?;
    }
    r.finish()?;
    Ok(nodes)
}

fn world_parts() -> (
    WorldConfig,
    rtsim_trade::Goods,
    rtsim_trade::labour::RecipeBook,
) {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    (WorldConfig::new(p1::WORLD_SEED), goods, book)
}

/// The phase 1 world, as `rtsim-jni`'s `Link::read_pages` restores it.
fn read_phase1(source: &Arc<dyn PageSource>, budget: usize) -> Result<Option<Phase1>, String> {
    let Some(meta) = source
        .read(p1::KEY_META)
        .map_err(|e| format!("cannot read the phase 1 meta page: {e}"))?
    else {
        if holds_pages(&**source, p1::TABLE) {
            return Err("phase 1 pages without their meta page".into());
        }
        return Ok(None);
    };
    let m = decode_meta(&meta).map_err(|e| format!("the phase 1 meta page: {e}"))?;
    let (cfg, goods, book) = world_parts();
    let (mut world, mut nodes) = if m.snap_pieces > 0 {
        let mut blob = Vec::with_capacity(m.snap_len);
        for k in 0..m.snap_pieces {
            let b = source
                .read(p1::piece_key(k))
                .map_err(|e| format!("cannot read phase 1 snapshot page {k}: {e}"))?
                .ok_or_else(|| format!("phase 1 snapshot page {k} is missing"))?;
            blob.extend_from_slice(&b);
        }
        if blob.len() != m.snap_len {
            return Err(format!(
                "the phase 1 snapshot holds {} bytes, its meta page says {}",
                blob.len(),
                m.snap_len
            ));
        }
        let nodes = decode_snapshot(&blob).map_err(|e| format!("the phase 1 snapshot: {e}"))?;
        let world = World::open_paged(source.clone(), budget, p1::WORLD_META, cfg, goods, book)
            .map_err(|e| format!("the phase 1 snapshot's world: {e}"))?;
        if world.now().0 != m.snap_at {
            return Err(format!(
                "the phase 1 snapshot is at world tick {}, its meta page says {}",
                world.now().0,
                m.snap_at
            ));
        }
        if let Some((k, i)) = nodes
            .iter()
            .find(|(k, i)| world.nodes.get(**i).is_none_or(|n| n.key != **k))
        {
            return Err(format!(
                "the phase 1 snapshot's village {k:#x} is node {i}, which the world does not have"
            ));
        }
        (world, nodes)
    } else if keys::tables(p1::TABLE)
        .filter(|t| *t >= p1::SNAPSHOT_TABLE)
        .any(|t| source.table_ids(t).map(|i| !i.is_empty()).unwrap_or(true))
    {
        return Err("phase 1 world pages without a snapshot".into());
    } else {
        let mut w = World::build(cfg, goods, book, Vec::new());
        w.attach_store(source.clone(), budget);
        (w, BTreeMap::new())
    };
    let mut log = Vec::with_capacity(m.log_len);
    for k in 0..m.log_len.div_ceil(p1::CHUNK) {
        let b = source
            .read(p1::chunk_key(k))
            .map_err(|e| format!("cannot read phase 1 log page {k}: {e}"))?
            .ok_or_else(|| format!("phase 1 log page {k} is missing"))?;
        let mut r = Reader::new(&b);
        while !r.is_empty() {
            log.push(Logged::decode(&mut r).map_err(|e| format!("phase 1 log page {k}: {e}"))?);
        }
    }
    if log.len() != m.log_len {
        return Err(format!(
            "the phase 1 log holds {} inputs, its meta page says {}",
            log.len(),
            m.log_len
        ));
    }
    for l in &log {
        inputs::apply(&mut world, &mut nodes, &l.input, l.at);
    }
    world.run_until(Tick(m.now));
    let _ = world.take_commands();
    world.enforce_budget();
    Ok(Some(Phase1 {
        world,
        offset: m.offset,
        nodes,
        saved_hash: m.hash,
        replayed: log.len(),
    }))
}

/// The world model's cells: its meta page and every region page.
fn read_world_model(source: &dyn PageSource) -> Result<Option<(SampledWorld, u64)>, String> {
    let meta = match source.read(keys::WORLD_MODEL) {
        Ok(Some(b)) => b,
        Ok(None) => {
            let stray = source
                .table_ids(keys::WORLD_MODEL_REGIONS)
                .map(|i| !i.is_empty())
                .unwrap_or(true);
            if stray {
                return Err("region pages without their meta page".into());
            }
            return Ok(None);
        }
        Err(e) => return Err(format!("cannot read the world model's meta page: {e}")),
    };
    let ids = source
        .table_ids(keys::WORLD_MODEL_REGIONS)
        .map_err(|e| format!("cannot list the region pages: {e}"))?;
    let mut pages = Vec::with_capacity(ids.len());
    for id in ids {
        match source.read(PageKey::new(keys::WORLD_MODEL_REGIONS, id)) {
            Ok(Some(b)) => pages.push(b),
            Ok(None) => {}
            Err(e) => return Err(format!("cannot read a region page: {e}")),
        }
    }
    SampledWorld::load(&meta, pages.iter().map(Vec::as_slice))
        .map(Some)
        .map_err(|e| format!("the world model's pages: {e}"))
}
