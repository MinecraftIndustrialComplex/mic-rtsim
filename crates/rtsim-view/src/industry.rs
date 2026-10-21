// SPDX-License-Identifier: AGPL-3.0-or-later
//! The industry engine's pages, read without holding every district decoded (VIEWER.md §5.3).
//!
//! At load: the globals page restores an engine with no districts (stations, lines, train legs,
//! goods and the clock), then every district page is decoded one at a time into a small
//! [`DistrictSummary`] and dropped, and every cluster page is read for its members. A card
//! restores an engine from the globals page, that district's page and its cluster's page, peeks,
//! and drops it.

use std::collections::BTreeMap;
use std::ops::Range;

use rtsim_industry::cluster::Cluster;
use rtsim_industry::district::District;
use rtsim_industry::engine::{Engine, Inspection, LineEdge, ProcessPeek};
use rtsim_industry::ids::{DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::StationDecl;
use rtsim_industry::persist::{self, CLUSTER_TABLE, DISTRICT_TABLE, GLOBAL_TABLE, Persist};
use rtsim_industry::threat::Leg;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::Reader;
use rtsim_kernel::page::PageKey;
use rtsim_kernel::rows::PageSource;
use rtsim_layers::{DistrictMark, IndustrySource, Table};

/// What a district's map mark needs, kept for every district.
#[derive(Clone, Debug)]
pub struct DistrictSummary {
    pub id: u64,
    pub dim: u32,
    /// Java's name id (DICT_NAME), 0 if unnamed.
    pub name: u32,
    pub chunks: u32,
    /// Chunk bounds: min x, min z, max x, max z.
    pub chunk_box: (i32, i32, i32, i32),
    /// What the engine's inspect would call it ("running", "learning", ...).
    pub state: &'static str,
    pub stations: Vec<u64>,
    pub attacks: u32,
    pub inventories: u32,
    pub watched: bool,
    /// Its chunks, sorted.
    pub chunk_list: Vec<(i32, i32)>,
    /// Its chunks' outline in chunk corners (`rtsim_layers::industry::chunk_outline`).
    pub outline: Vec<Vec<[i32; 2]>>,
}

impl DistrictSummary {
    /// The block at the middle of its chunk box.
    pub fn centre(&self) -> (f64, f64) {
        let (x0, z0, x1, z1) = self.chunk_box;
        (
            (x0 as f64 + x1 as f64 + 1.0) * 8.0,
            (z0 as f64 + z1 as f64 + 1.0) * 8.0,
        )
    }

    /// Its chunk box in blocks: x0, z0, x1, z1.
    pub fn block_box(&self) -> [f64; 4] {
        rtsim_layers::industry::block_box(self.chunk_box)
    }
}

/// The industry engine's save, as the viewer holds it.
pub struct IndustryView {
    /// Game tick of the save (industry runs on game time).
    pub now: i64,
    pub goods: BTreeMap<Good, GoodInfo>,
    pub stations: Vec<StationDecl>,
    pub lines: Vec<((u64, u64), LineEdge)>,
    pub legs: Vec<(u64, Leg)>,
    /// By id.
    pub districts: Vec<DistrictSummary>,
    /// District id to its replay cluster's page id.
    pub cluster_of: BTreeMap<u64, u64>,
    /// A hash of every industry page read (the engine has no state hash of its own).
    pub pages_hash: u64,
    globals: Vec<u8>,
}

impl IndustryView {
    /// The industry pages, or `None` when the world has none.
    pub fn load(source: &dyn PageSource) -> Result<Option<IndustryView>, String> {
        let gkey = PageKey::new(GLOBAL_TABLE, 0);
        let globals = match source.read(gkey) {
            Ok(Some(b)) => b,
            Ok(None) => {
                for t in [DISTRICT_TABLE, CLUSTER_TABLE] {
                    if source.table_ids(t).map(|i| !i.is_empty()).unwrap_or(true) {
                        return Err("industry pages without their globals page".into());
                    }
                }
                return Ok(None);
            }
            Err(e) => return Err(format!("cannot read the industry globals page: {e}")),
        };
        let mut hash = Fnv::new();
        hash.page(gkey, &globals);
        let e = Engine::restore([(gkey, globals.clone())], Tunables::default())
            .map_err(|e| format!("the industry globals page: {e}"))?;
        let ids = source
            .table_ids(DISTRICT_TABLE)
            .map_err(|e| format!("cannot list the district pages: {e}"))?;
        let mut districts = Vec::with_capacity(ids.len());
        for id in ids {
            let k = PageKey::new(DISTRICT_TABLE, id);
            let b = source
                .read(k)
                .map_err(|e| format!("cannot read district page {id}: {e}"))?
                .ok_or_else(|| format!("district page {id} is listed and missing"))?;
            hash.page(k, &b);
            let d: District =
                persist::from_bytes(&b).map_err(|e| format!("district page {id}: {e}"))?;
            districts.push(summarize(&d));
        }
        let ids = source
            .table_ids(CLUSTER_TABLE)
            .map_err(|e| format!("cannot list the cluster pages: {e}"))?;
        let mut cluster_of = BTreeMap::new();
        for id in ids {
            let k = PageKey::new(CLUSTER_TABLE, id);
            let b = source
                .read(k)
                .map_err(|e| format!("cannot read cluster page {id}: {e}"))?
                .ok_or_else(|| format!("cluster page {id} is listed and missing"))?;
            hash.page(k, &b);
            let mut r = Reader::new(&b);
            let model = Cluster::get(&mut r).map_err(|e| format!("cluster page {id}: {e}"))?;
            for m in model.members() {
                cluster_of.insert(m.0, id);
            }
        }
        for d in &mut districts {
            if d.state == "learned" {
                d.state = if cluster_of.contains_key(&d.id) {
                    "running"
                } else {
                    "out of season"
                };
            }
        }
        Ok(Some(IndustryView {
            now: e.now().0 as i64,
            goods: e.goods().clone(),
            stations: e.stations().cloned().collect(),
            lines: e
                .lines()
                .iter()
                .map(|((a, b), l)| ((a.0, b.0), l.clone()))
                .collect(),
            legs: e.legs().iter().map(|(id, l)| (id, l.clone())).collect(),
            districts,
            cluster_of,
            pages_hash: hash.0,
            globals,
        }))
    }

    pub fn district(&self, id: u64) -> Option<&DistrictSummary> {
        self.districts
            .binary_search_by_key(&id, |d| d.id)
            .ok()
            .map(|i| &self.districts[i])
    }

    pub fn good_name(&self, g: Good) -> String {
        self.goods
            .get(&g)
            .map_or_else(|| format!("good {}", g.0), |i| i.name.clone())
    }

    /// One district as the engine would inspect it, from an engine restored for it alone (its
    /// cluster's other members' pages are not read: their pools and processes live in the cluster
    /// page).
    pub fn inspect(&self, source: &dyn PageSource, id: u64) -> Result<DistrictPeek, String> {
        let gkey = PageKey::new(GLOBAL_TABLE, 0);
        let dkey = PageKey::new(DISTRICT_TABLE, id);
        let db = source
            .read(dkey)
            .map_err(|e| format!("cannot read district page {id}: {e}"))?
            .ok_or_else(|| format!("no district {id}"))?;
        let mut pages = vec![(gkey, self.globals.clone()), (dkey, db)];
        if let Some(c) = self.cluster_of.get(&id) {
            let ckey = PageKey::new(CLUSTER_TABLE, *c);
            if let Some(b) = source
                .read(ckey)
                .map_err(|e| format!("cannot read cluster page {c}: {e}"))?
            {
                pages.push((ckey, b));
            }
        }
        let e = Engine::restore(pages, Tunables::default())
            .map_err(|e| format!("district {id}'s pages: {e}"))?;
        let d = DistrictId(id);
        let inspection = e
            .inspect_peek(d, Tick(self.now.max(0) as u64))
            .ok_or_else(|| format!("district {id} did not restore"))?;
        Ok(DistrictPeek {
            inspection,
            stock: e.stock_peek(d),
            members: e
                .cluster_members(d)
                .map(|m| m.into_iter().map(|m| m.0).collect())
                .unwrap_or_default(),
            processes: e.processes_peek(d).unwrap_or_default(),
        })
    }
}

/// What a district's card shows beyond its summary ([`IndustryView::inspect`]).
#[derive(Clone, Debug, PartialEq)]
pub struct DistrictPeek {
    pub inspection: Inspection,
    /// The replay's stock, if it replays.
    pub stock: Option<GoodVec>,
    /// Every district of its replay cluster, itself included (empty if it does not replay).
    pub members: Vec<u64>,
    /// Its replay processes, box processes first (empty if it does not replay).
    pub processes: Vec<ProcessPeek>,
}

fn summarize(d: &District) -> DistrictSummary {
    DistrictSummary {
        id: d.id.0,
        dim: d.dim,
        name: d.name,
        chunks: d.chunks.len() as u32,
        chunk_box: rtsim_layers::industry::chunk_box(d),
        // "learned" until the clusters are read (`IndustryView::load`).
        state: rtsim_layers::industry::district_state(d, None),
        stations: d.stations.iter().map(|s| s.0).collect(),
        attacks: d.attacks.len() as u32,
        inventories: d.inventories.len() as u32,
        watched: d.watched,
        chunk_list: d.chunks.iter().copied().collect(),
        outline: rtsim_layers::industry::chunk_outline(&d.chunks),
    }
}

/// The save's industry as the layers read it: every table is held sorted by key.
impl IndustrySource for IndustryView {
    fn districts(&self, ids: Range<u64>, f: &mut dyn FnMut(DistrictMark)) {
        let lo = self.districts.partition_point(|d| d.id < ids.start);
        for d in self.districts[lo..].iter().take_while(|d| d.id < ids.end) {
            f(DistrictMark {
                id: d.id,
                dim: d.dim,
                name: d.name,
                chunks: d.chunks,
                chunk_box: d.chunk_box,
                state: d.state,
                outline: d.outline.clone(),
            });
        }
    }

    fn stations(&self, ids: Range<u64>, f: &mut dyn FnMut(&StationDecl)) {
        let lo = self.stations.partition_point(|s| s.station.0 < ids.start);
        for s in self.stations[lo..]
            .iter()
            .take_while(|s| s.station.0 < ids.end)
        {
            f(s);
        }
    }

    fn station(&self, id: u64) -> Option<&StationDecl> {
        let i = self.stations.partition_point(|s| s.station.0 < id);
        self.stations.get(i).filter(|s| s.station.0 == id)
    }

    fn lines(&self, ids: Range<u64>, f: &mut dyn FnMut((u64, u64), &LineEdge)) {
        let lo = self.lines.partition_point(|((a, _), _)| *a < ids.start);
        for (k, l) in self.lines[lo..]
            .iter()
            .take_while(|((a, _), _)| *a < ids.end)
        {
            f(*k, l);
        }
    }

    fn legs(&self, ids: Range<u64>, f: &mut dyn FnMut(u64, &Leg)) {
        let lo = self.legs.partition_point(|(id, _)| *id < ids.start);
        for (id, l) in self.legs[lo..].iter().take_while(|(id, _)| *id < ids.end) {
            f(*id, l);
        }
    }

    fn next_key(&self, t: Table, from: u64) -> Option<u64> {
        match t {
            Table::Districts => {
                let i = self.districts.partition_point(|d| d.id < from);
                self.districts.get(i).map(|d| d.id)
            }
            Table::Stations => {
                let i = self.stations.partition_point(|s| s.station.0 < from);
                self.stations.get(i).map(|s| s.station.0)
            }
            Table::Lines => {
                let i = self.lines.partition_point(|((a, _), _)| *a < from);
                self.lines.get(i).map(|((a, _), _)| *a)
            }
            Table::Legs => {
                let i = self.legs.partition_point(|(id, _)| *id < from);
                self.legs.get(i).map(|(id, _)| *id)
            }
        }
    }
}

/// FNV-1a over page keys and bytes.
pub(crate) struct Fnv(pub u64);

impl Fnv {
    pub fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    pub fn page(&mut self, k: PageKey, b: &[u8]) {
        self.bytes(&k.table().to_le_bytes());
        self.bytes(&k.id().to_le_bytes());
        self.bytes(&(b.len() as u64).to_le_bytes());
        self.bytes(b);
    }
}
