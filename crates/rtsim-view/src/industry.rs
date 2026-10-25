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
use std::panic::{AssertUnwindSafe, catch_unwind};

use rtsim_industry::cluster::Cluster;
use rtsim_industry::district::District;
use rtsim_industry::engine::{Engine, Inspection, LineEdge, ProcessPeek};
use rtsim_industry::glass::net::{Nets, PartId};
use rtsim_industry::glass::page::{PageRead, read_district_page};
use rtsim_industry::glass::report::DistrictSections;
use rtsim_industry::ids::{DistrictId, Good, GoodInfo, GoodVec};
use rtsim_industry::obs::StationDecl;
use rtsim_industry::persist::{
    self, CLUSTER_TABLE, DISTRICT_TABLE, GLASS_TABLE, GLOBAL_TABLE, LEG_TABLE, Persist, STORE_TABLE,
};
use rtsim_industry::stores::{PackageStore, read_store_page};
use rtsim_industry::threat::Leg;
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::Reader;
use rtsim_kernel::page::PageKey;
use rtsim_kernel::rows::PageSource;
use rtsim_layers::industry::{NetPart, store_mark};

use crate::load::Limits;
use rtsim_layers::{DistrictMark, IndustrySource, NetMark, SectionMark, StoreMark, Table};

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
    /// The glass pages' sections per district with a complete compile, by district id (slice S7).
    pub sections: Vec<(u64, DistrictSections)>,
    /// The package stores, by id (slice S7).
    pub stores: Vec<PackageStore>,
    /// The shared kinetic networks the compiles make, in the registry's order (slice S7).
    pub nets: Vec<NetMark>,
    /// Glass pages that did not read, as "district N: why" (the layers draw without them).
    pub glass_unread: Vec<String>,
    /// Store pages that did not read, as "store N: why" (the layers draw without them).
    pub stores_unread: Vec<String>,
    globals: Vec<u8>,
}

impl IndustryView {
    /// The industry pages, or `None` when the world has none.
    pub fn load(source: &dyn PageSource, limits: &Limits) -> Result<Option<IndustryView>, String> {
        let gkey = PageKey::new(GLOBAL_TABLE, 0);
        let globals = match source.read(gkey) {
            Ok(Some(b)) => b,
            Ok(None) => {
                for t in [
                    DISTRICT_TABLE,
                    CLUSTER_TABLE,
                    GLASS_TABLE,
                    LEG_TABLE,
                    STORE_TABLE,
                ] {
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
        // The booked legs are pages of their own (ID10); the viewer's map draws them.
        let mut pages = vec![(gkey, globals.clone())];
        let ids = source
            .table_ids(LEG_TABLE)
            .map_err(|e| format!("cannot list the leg pages: {e}"))?;
        for id in ids {
            let k = PageKey::new(LEG_TABLE, id);
            let b = source
                .read(k)
                .map_err(|e| format!("cannot read leg page {id}: {e}"))?
                .ok_or_else(|| format!("leg page {id} is listed and missing"))?;
            hash.page(k, &b);
            pages.push((k, b));
        }
        let e = Engine::restore(pages, Tunables::default())
            .map_err(|e| format!("the industry globals and leg pages: {e}"))?;
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
        let tun = Tunables::default();
        let (sections, nets, glass_unread) = read_glass(source, &districts, &tun, limits)?;
        let (stores, stores_unread) = read_stores(source, limits)?;
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
            sections,
            stores,
            nets,
            glass_unread,
            stores_unread,
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

    /// A good as words for a card: `minecraft:iron_ingot` as "Iron ingot" (a component hash
    /// after `#` dropped).
    pub fn good_title(&self, g: Good) -> String {
        match self.goods.get(&g) {
            Some(i) => {
                mic_viewer_create::tags::humanize(i.name.split('#').next().unwrap_or(&i.name))
            }
            None => format!("Good {}", g.0),
        }
    }

    /// The unit a good is counted in: items, millibuckets, or an energy's own unit.
    pub fn good_unit(&self, g: Good) -> &'static str {
        match self.goods.get(&g).map(|i| i.class) {
            Some(rtsim_industry::ids::GoodClass::Fluid) => "mB",
            Some(rtsim_industry::ids::GoodClass::Item) | None => "items",
            Some(_) => "units",
        }
    }

    /// A view over nothing but what the test hands it: the glass and store tables of slice S7.
    #[cfg(test)]
    pub(crate) fn synthetic(
        districts: Vec<DistrictSummary>,
        sections: Vec<(u64, DistrictSections)>,
        stores: Vec<PackageStore>,
        nets: Vec<NetMark>,
    ) -> IndustryView {
        IndustryView {
            now: 0,
            goods: BTreeMap::new(),
            stations: Vec::new(),
            lines: Vec::new(),
            legs: Vec::new(),
            districts,
            cluster_of: BTreeMap::new(),
            pages_hash: 0,
            sections,
            stores,
            nets,
            glass_unread: Vec::new(),
            stores_unread: Vec::new(),
            globals: Vec::new(),
        }
    }

    /// A station's name from Java's dictionary in `names`, or its number.
    pub fn station_title(&self, names: &crate::journal::Names, id: u64) -> String {
        self.station(id)
            .and_then(|s| names.get(crate::keys::DICT_NAME, s.name))
            .map_or_else(|| format!("Station {id}"), str::to_owned)
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

/// The glass pages' sections and the networks the compiles make. A page that does not decode or
/// the save's format dropped is left out and named; only a listing failure fails the load.
#[allow(clippy::type_complexity)]
fn read_glass(
    source: &dyn PageSource,
    districts: &[DistrictSummary],
    tun: &Tunables,
    limits: &Limits,
) -> Result<(Vec<(u64, DistrictSections)>, Vec<NetMark>, Vec<String>), String> {
    let ids = source
        .table_ids(GLASS_TABLE)
        .map_err(|e| format!("cannot list the glass pages: {e}"))?;
    let mut sections = Vec::new();
    let mut unread = Unread::default();
    let mut parts: Vec<(PartId, u32, rtsim_industry::glass::records::GraphDomain)> = Vec::new();
    let mut read = 0usize;
    for id in ids {
        let Some(dim) = districts
            .binary_search_by_key(&id, |d| d.id)
            .ok()
            .map(|i| districts[i].dim)
        else {
            continue;
        };
        if read >= limits.glass_pages {
            unread.cut();
            continue;
        }
        let b = match source.read(PageKey::new(GLASS_TABLE, id)) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                unread.note(format!("district {id}: {e}"));
                continue;
            }
        };
        read += 1;
        // A page that panics the decoder is left out like one that does not decode.
        let page = catch_unwind(AssertUnwindSafe(|| {
            read_district_page(&b, tun.glass.sections_max, tun.glass.counted_stores_max)
        }));
        let dg = match page {
            Ok(Ok(PageRead::Kept(dg))) => dg,
            Ok(Ok(PageRead::Dropped(why))) => {
                unread.note(format!("district {id}: {why}"));
                continue;
            }
            Ok(Err(e)) => {
                unread.note(format!("district {id}: {e}"));
                continue;
            }
            Err(p) => {
                unread.note(format!(
                    "district {id}: decoding failed ({})",
                    crate::load::panic_text(p)
                ));
                continue;
            }
        };
        let Some(c) = dg.compiled.as_ref() else {
            continue;
        };
        for (i, dom) in c.graph.domains.iter().enumerate() {
            if dom.shared() {
                parts.push((
                    PartId {
                        district: DistrictId(id),
                        domain: i as u32,
                    },
                    dim,
                    dom.clone(),
                ));
            }
        }
        if let Some(ds) = DistrictSections::of(&dg) {
            sections.push((id, ds));
        }
    }
    let refs: Vec<_> = parts.iter().map(|(p, d, g)| (*p, *d, g)).collect();
    let nets = Nets::build(&refs, tun.replay.cluster_districts_max);
    let centre = |d: u64| {
        districts
            .binary_search_by_key(&d, |x| x.id)
            .ok()
            .map_or((0.0, 0.0), |i| districts[i].centre())
    };
    let marks = (0..nets.len())
        .filter_map(|i| nets.get(i).map(|n| (i, n)))
        .map(|(i, n)| {
            let mut dim = 0;
            let parts: Vec<NetPart> = n
                .parts()
                .map(|p| {
                    let (d, source) = parts
                        .iter()
                        .find(|(q, _, _)| *q == p)
                        .map_or((0, None), |(_, dm, g)| {
                            (*dm, g.sources.first().map(|s| s.pos))
                        });
                    dim = d;
                    let (x, z) = source.map_or_else(
                        || centre(p.district.0),
                        |pos| (f64::from(block_x(pos)), f64::from(block_z(pos))),
                    );
                    NetPart {
                        district: p.district.0,
                        x,
                        z,
                    }
                })
                .collect();
            NetMark {
                id: i as u64,
                dim,
                status: n.status.words(),
                coupled: n.status == rtsim_industry::glass::net::Status::Coupled,
                parts,
            }
        })
        .collect();
    Ok((sections, marks, unread.finish("glass")))
}

/// `BlockPos.asLong`'s x: the top 26 bits, signed.
fn block_x(pos: i64) -> i32 {
    (pos >> 38) as i32
}

/// `BlockPos.asLong`'s z: the middle 26 bits, signed.
fn block_z(pos: i64) -> i32 {
    ((pos << 26) >> 38) as i32
}

/// The package stores, by id, and the pages left out. A page that does not decode, or that panics
/// the decoder, is named and skipped; the stores past `limits.store_pages` are named as one line.
fn read_stores(
    source: &dyn PageSource,
    limits: &Limits,
) -> Result<(Vec<PackageStore>, Vec<String>), String> {
    let ids = source
        .table_ids(STORE_TABLE)
        .map_err(|e| format!("cannot list the store pages: {e}"))?;
    let mut out = Vec::new();
    let mut unread = Unread::default();
    for id in ids {
        if out.len() >= limits.store_pages {
            unread.cut();
            continue;
        }
        let b = match source.read(PageKey::new(STORE_TABLE, id)) {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                unread.note(format!("store {id}: {e}"));
                continue;
            }
        };
        match catch_unwind(AssertUnwindSafe(|| read_store_page(id, &b))) {
            Ok(Ok(s)) => out.push(s),
            Ok(Err(e)) => unread.note(format!("store {id}: {e}")),
            Err(p) => unread.note(format!(
                "store {id}: decoding failed ({})",
                crate::load::panic_text(p)
            )),
        }
    }
    Ok((out, unread.finish("store")))
}

/// Pages a reader left out: the first few named, the rest counted.
#[derive(Default)]
struct Unread {
    named: Vec<String>,
    more: usize,
    cut: usize,
}

impl Unread {
    /// Names listed before the rest are only counted.
    const NAMED: usize = 8;

    fn note(&mut self, why: String) {
        if self.named.len() < Self::NAMED {
            self.named.push(why);
        } else {
            self.more += 1;
        }
    }

    /// A page past the reader's cap.
    fn cut(&mut self) {
        self.cut += 1;
    }

    fn finish(mut self, what: &str) -> Vec<String> {
        if self.more > 0 {
            self.named
                .push(format!("{} more {what} pages did not read", self.more));
        }
        if self.cut > 0 {
            self.named.push(format!(
                "{} {what} pages past the viewer's limit were not read",
                self.cut
            ));
        }
        self.named
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

    fn sections(&self, ids: Range<u64>, f: &mut dyn FnMut(&SectionMark)) {
        let lo = self.sections.partition_point(|(d, _)| *d < ids.start);
        for (d, ds) in self.sections[lo..].iter().take_while(|(d, _)| *d < ids.end) {
            let Some(dim) = self.district(*d).map(|x| x.dim) else {
                continue;
            };
            for (i, s) in ds.sections.iter().enumerate() {
                let Some((a, b)) = s.bbox else {
                    continue;
                };
                f(&SectionMark {
                    district: *d,
                    dim,
                    index: i as u32,
                    state: s.state,
                    working: s.working,
                    nodes: s.nodes,
                    stores: s.stores.len() as u32,
                    shared_stores: s.stores.iter().filter(|x| x.1).count() as u32,
                    unit: s.unit,
                    block_box: [
                        f64::from(a.x),
                        f64::from(a.z),
                        f64::from(b.x) + 1.0,
                        f64::from(b.z) + 1.0,
                    ],
                });
            }
        }
    }

    fn glass_why(&self) -> Option<String> {
        let first = self.glass_unread.first()?;
        let more = self.glass_unread.len() - 1;
        Some(format!(
            "glass pages did not read, so their sections and networks are left out ({first}{})",
            if more > 0 {
                format!("; and {more} more")
            } else {
                String::new()
            }
        ))
    }

    fn stores_why(&self) -> Option<String> {
        let first = self.stores_unread.first()?;
        let more = self.stores_unread.len() - 1;
        Some(format!(
            "store pages did not read, so their stores are left out ({first}{})",
            if more > 0 {
                format!("; and {more} more")
            } else {
                String::new()
            }
        ))
    }

    fn stores(&self, ids: Range<u64>, f: &mut dyn FnMut(&StoreMark)) {
        let lo = self.stores.partition_point(|s| s.id.0 < ids.start);
        for s in self.stores[lo..].iter().take_while(|s| s.id.0 < ids.end) {
            f(&store_mark(s));
        }
    }

    fn networks(&self, ids: Range<u64>, f: &mut dyn FnMut(&NetMark)) {
        let lo = (ids.start as usize).min(self.nets.len());
        let hi = (ids.end.min(self.nets.len() as u64)) as usize;
        for n in &self.nets[lo..hi.max(lo)] {
            f(n);
        }
    }

    fn next_key(&self, t: Table, from: u64) -> Option<u64> {
        match t {
            Table::Sections => {
                let i = self.sections.partition_point(|(d, _)| *d < from);
                self.sections.get(i).map(|(d, _)| *d)
            }
            Table::Stores => {
                let i = self.stores.partition_point(|s| s.id.0 < from);
                self.stores.get(i).map(|s| s.id.0)
            }
            Table::Networks => (from < self.nets.len() as u64).then_some(from),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rtsim_industry::ids::StoreId;
    use rtsim_industry::stores::store_page;

    /// Pages held in memory.
    struct Pages(BTreeMap<(u16, u64), Vec<u8>>);

    impl PageSource for Pages {
        fn read(&self, key: PageKey) -> rtsim_kernel::page::Result<Option<Vec<u8>>> {
            Ok(self.0.get(&(key.table(), key.id())).cloned())
        }
        fn table_ids(&self, table: u16) -> rtsim_kernel::page::Result<Vec<u64>> {
            Ok(self
                .0
                .keys()
                .filter(|k| k.0 == table)
                .map(|k| k.1)
                .collect())
        }
    }

    fn store(id: u64) -> PackageStore {
        PackageStore {
            id: StoreId(id),
            ..PackageStore::default()
        }
    }

    /// One store page that does not decode is named and skipped; the others still draw, and the
    /// pages past the limit are counted, not read.
    #[test]
    fn a_bad_store_page_is_named_not_fatal_and_the_limit_cuts() {
        let mut pages = BTreeMap::new();
        for id in 1..=3 {
            pages.insert((STORE_TABLE, id), store_page(&store(id)));
        }
        pages.insert((STORE_TABLE, 2), b"garbage".to_vec());
        let src = Pages(pages);
        let (stores, unread) = read_stores(&src, &Limits::default()).expect("the listing reads");
        assert_eq!(stores.iter().map(|s| s.id.0).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(unread.len(), 1, "{unread:?}");
        assert!(unread[0].starts_with("store 2:"), "{unread:?}");
        let few = Limits {
            store_pages: 1,
            ..Limits::default()
        };
        let (stores, unread) = read_stores(&src, &few).expect("the listing reads");
        assert_eq!(stores.len(), 1);
        assert!(
            unread.iter().any(|u| u.contains("past the viewer's limit")),
            "{unread:?}"
        );
    }

    /// Glass pages that do not decode are named and left out; the view still loads.
    #[test]
    fn a_bad_glass_page_is_named_and_the_glass_pages_cap() {
        let mut pages = BTreeMap::new();
        for id in 1..=3 {
            pages.insert((GLASS_TABLE, id), b"not a glass page".to_vec());
        }
        let src = Pages(pages);
        let districts: Vec<DistrictSummary> = (1..=3)
            .map(|id| DistrictSummary {
                id,
                dim: 0,
                name: 0,
                chunks: 1,
                chunk_box: (0, 0, 0, 0),
                state: "running",
                stations: Vec::new(),
                attacks: 0,
                inventories: 0,
                watched: false,
                chunk_list: Vec::new(),
                outline: Vec::new(),
            })
            .collect();
        let tun = Tunables::default();
        let (sections, nets, unread) =
            read_glass(&src, &districts, &tun, &Limits::default()).expect("the listing reads");
        assert!(sections.is_empty() && nets.is_empty());
        assert_eq!(unread.len(), 3, "{unread:?}");
        let few = Limits {
            glass_pages: 1,
            ..Limits::default()
        };
        let (_, _, unread) = read_glass(&src, &districts, &tun, &few).expect("the listing reads");
        assert!(
            unread
                .iter()
                .any(|u| u.starts_with("2 glass pages past the viewer's limit")),
            "{unread:?}"
        );
    }
}
