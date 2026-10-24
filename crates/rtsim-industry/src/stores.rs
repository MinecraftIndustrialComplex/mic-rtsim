// SPDX-License-Identifier: AGPL-3.0-or-later
//! Package stores (SECTIONS.md §2.5, slice S4): a chain network whose frogports sit in more than
//! one district holds the packages in transit, owned by no district, with a port at each frogport.
//!
//! - **Identity is Java's** (IND_PACKAGE_STORE): the store's id, its frogports by position and the
//!   hops between their conveyors along the chain. A declaration re-indexes the store's pools by
//!   frogport position, so a pool follows its frogport across declarations. A frogport that comes
//!   from another store (`from`) brings what that store held for it. A declaration with no
//!   frogports removes the store, and what it held joins `into` when that is given.
//! - **Books** (IND_CHAIN_PACKAGE): a `CHAIN_OUT` books its packages into the pool of the frogport
//!   Java resolved its route to, or into the no-route pool; a `CHAIN_IN` takes them out of its own
//!   frogport's pool, never below zero. The district side (a port of the frogport's district's
//!   window) is the engine's (`Engine::chain_package`).
//! - **The census** (IND_STORE_CENSUS): while every chunk of the store ticks, Java counts the
//!   packages on its conveyors by destination; the pools are set to it, and the difference from the
//!   books is the store's residue (packages a player hung on or took off, or a route Java resolved
//!   otherwise than Create did).
//! - **Pages**: one per store (`STORE_TABLE`, id = the store id), starting with [`STORE_MAGIC`].
//!
//! The pools above are the world's packages: no replay draws them, and nothing is written from
//! them (UR-1: the packages are on the chain). What replays send each other (slice S4b) is
//! virtual: a cluster's `PoolKey::Chain` pool while one holds it, else the store's
//! [`PackageStore::owed`], written into a loaded importer's frogport as real packages
//! (`Engine::write_chain`).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};

use crate::ids::{BlockPos, DistrictId, GoodVec, StoreId};
use crate::obs::{ChainDirection, ChainPackage, StoreCensus, StoreDecl};
use crate::persist::{Persist, persist_struct};

/// A store page starts with this, so a page of another format fails by name.
pub const STORE_MAGIC: u32 = u32::from_le_bytes(*b"IST2");

/// Packages and the goods inside them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    pub packages: u64,
    pub goods: GoodVec,
}

impl Pool {
    pub fn is_empty(&self) -> bool {
        self.packages == 0 && self.goods.is_empty()
    }

    pub fn add(&mut self, packages: u64, goods: &GoodVec) {
        self.packages += packages;
        self.goods.merge(goods);
    }

    pub fn merge(&mut self, o: &Pool) {
        self.add(o.packages, &o.goods);
    }

    /// Takes `packages` and `goods` out, each floored at zero (§2.5: a `CHAIN_IN` beyond its pool
    /// takes the pool to zero, never below). Returns what was missing.
    pub fn take(&mut self, packages: u64, goods: &GoodVec) -> Pool {
        let mut short = Pool::default();
        let have = self.packages.min(packages);
        self.packages -= have;
        short.packages = packages - have;
        for (g, n) in goods.iter() {
            if n <= 0 {
                continue;
            }
            let got = self.goods.get(g).clamp(0, n);
            self.goods.add(g, -got);
            short.goods.add(g, n - got);
        }
        short
    }
}

/// One frogport of a store, as its last declaration listed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frogport {
    pub district: Option<DistrictId>,
    pub inventory: Option<u32>,
    pub star: bool,
    pub pos: BlockPos,
    pub conveyor: BlockPos,
}

/// Counts a store keeps for the operator (`/rtsim district sections`, `rtsim-cli stores`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreCounts {
    pub packages_out: u64,
    pub packages_in: u64,
    /// `CHAIN_OUT`s with no route (booked into the no-route pool).
    pub unrouted: u64,
    /// Packages a `CHAIN_IN` took beyond its pool (the census had not seen them yet, or a player
    /// hung them on).
    pub in_beyond: u64,
    pub censuses: u64,
    pub declarations: u64,
    /// Records that named a frogport index the store's last declaration does not have.
    pub stale_records: u64,
    /// Packages of records no later than the last census: the census, taken at the end of its
    /// tick, already counted them (only their counts above move).
    pub in_census: u64,
    /// Packages the engine wrote into frogports from what replays sent (`CHAIN_IN` with an op).
    pub written: u64,
}

/// What a district's replay needs of a package store for one of its chain links (slice S4b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainRoute {
    pub store: StoreId,
    /// The district's own frogport there.
    pub own: BlockPos,
    /// An export stops once its destination's pool holds this many packages: the route's room
    /// (`chain_room_per_conveyor` on each conveyor from the exporter's to the destination's) less
    /// the world's packages the store holds for that destination, which wait on the chain too.
    pub room_packages: i64,
    /// One package's contents toward the destination (empty: none seen yet).
    pub batch: GoodVec,
    /// An import's restart latency: the hops to the destination's farthest frogport.
    pub travel: u64,
}

/// A package store (SECTIONS.md §2.5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageStore {
    pub id: StoreId,
    pub dim: u32,
    pub frogports: Vec<Frogport>,
    /// The hops between frogports' conveyors, upper triangle row by row (`obs::hops_between`).
    pub hops: Vec<u16>,
    /// What the store holds for each frogport, in [`PackageStore::frogports`] order.
    pub pools: Vec<Pool>,
    /// Packages whose address no frogport matches: they circle, drawn by no one.
    pub no_route: Pool,
    /// What the store held for frogports it no longer lists, by position: a frogport that moved to
    /// another store takes it along (`from`), one that comes back finds it, and the next census
    /// replaces it.
    pub left: BTreeMap<BlockPos, Pool>,
    /// The last census's difference from the books (census minus books, per good), and in packages.
    pub residue: GoodVec,
    pub residue_packages: i64,
    pub census_at: Option<Tick>,
    pub counts: StoreCounts,
    /// What replays sent toward each frogport (by position) that no replay cluster holds now: a
    /// cluster's `PoolKey::Chain` pool starts from it and returns to it (slice S4b). Virtual: it is
    /// written into the frogport while that is loaded, and a reset may lose it (UR-1).
    pub owed: BTreeMap<BlockPos, GoodVec>,
    /// One package's contents toward each frogport (by position), from the last `CHAIN_OUT` routed
    /// there: the replay's room and the engine's writes count packages of it.
    pub batch: BTreeMap<BlockPos, GoodVec>,
    /// When the engine last wrote toward each frogport (by position): no other write is sent
    /// there until a `CHAIN_IN` with an op answers it or `chain_write_ticks` pass (Java answers a
    /// write that fitted nothing with no record).
    pub writing: BTreeMap<BlockPos, Tick>,
}

impl PackageStore {
    pub fn new(id: StoreId, dim: u32) -> Self {
        Self {
            id,
            dim,
            ..Self::default()
        }
    }

    /// Hops between frogports `i` and `j` along the chain.
    pub fn hops_between(&self, i: usize, j: usize) -> Option<u16> {
        crate::obs::hops_between(&self.hops, self.frogports.len(), i, j)
    }

    /// The frogport at `pos`, by index.
    pub fn index_of(&self, pos: BlockPos) -> Option<usize> {
        self.frogports.iter().position(|f| f.pos == pos)
    }

    /// The frogport of district `d`'s inventory `inventory`, by index.
    pub fn port_of(&self, d: DistrictId, inventory: u32) -> Option<usize> {
        self.frogports
            .iter()
            .position(|f| f.district == Some(d) && f.inventory == Some(inventory))
    }

    /// District `d`'s chain link from its frogport (inventory `inventory`) toward the frogport at
    /// `to` (its own for an import), when both are this store's and a district has the one at
    /// `to`: nothing replays a frogport of no district, so nothing would ever draw what an export
    /// sent there (it waits for the player, as the world's packages do).
    pub fn route(
        &self,
        d: DistrictId,
        inventory: u32,
        to: BlockPos,
        tun: &crate::tunables::ReplayTunables,
    ) -> Option<ChainRoute> {
        let i = self.port_of(d, inventory)?;
        let j = self.index_of(to)?;
        self.frogports[j].district?;
        let hops = i64::from(self.hops_between(i, j)?);
        let held = self.pools.get(j).map_or(0, |p| p.packages as i64);
        let farthest = (0..self.frogports.len())
            .filter_map(|k| self.hops_between(j, k))
            .max()
            .unwrap_or(0);
        Some(ChainRoute {
            store: self.id,
            own: self.frogports[i].pos,
            room_packages: tun.chain_room_per_conveyor * (hops + 1) - held,
            batch: self.batch.get(&to).cloned().unwrap_or_default(),
            travel: u64::from(farthest) * tun.chain_ticks_per_hop,
        })
    }

    /// Everything the store holds: every pool, the no-route pool and what it holds for frogports
    /// it no longer lists.
    pub fn held(&self) -> Pool {
        let mut p = self.no_route.clone();
        for x in self.pools.iter().chain(self.left.values()) {
            p.merge(x);
        }
        p
    }

    /// Takes what the store holds for the frogport at `pos` (a frogport moving to another store):
    /// its pool, and what replays owe it with its batch.
    fn take_for(&mut self, pos: BlockPos) -> Taken {
        let pool = match self.index_of(pos) {
            Some(i) => std::mem::take(&mut self.pools[i]),
            None => self.left.remove(&pos).unwrap_or_default(),
        };
        Taken {
            pool,
            owed: self.owed.remove(&pos).unwrap_or_default(),
            batch: self.batch.remove(&pos),
        }
    }

    /// Adds what another store held for the frogport at `pos`.
    fn bring(&mut self, pos: BlockPos, t: Taken) {
        if !t.pool.is_empty() {
            match self.index_of(pos) {
                Some(i) => self.pools[i].merge(&t.pool),
                None => self.left.entry(pos).or_default().merge(&t.pool),
            }
        }
        if !t.owed.is_empty() {
            self.owed.entry(pos).or_default().merge(&t.owed);
        }
        if let Some(b) = t.batch {
            self.batch.entry(pos).or_insert(b);
        }
    }

    /// What replays owe the frogport at `pos`, by good.
    pub fn owed_to(&self, pos: BlockPos) -> GoodVec {
        self.owed.get(&pos).cloned().unwrap_or_default()
    }

    /// Takes what replays owe the frogport at `pos` of good `g` (a cluster's pool starts from it).
    pub fn take_owed(&mut self, pos: BlockPos, g: crate::ids::Good) -> i64 {
        let Some(o) = self.owed.get_mut(&pos) else {
            return 0;
        };
        let n = o.get(g);
        o.add(g, -n);
        if o.is_empty() {
            self.owed.remove(&pos);
        }
        n
    }

    /// Adds to what replays owe the frogport at `pos` (a cluster's pool returning, floored at 0).
    pub fn add_owed(&mut self, pos: BlockPos, g: crate::ids::Good, n: i64) {
        let o = self.owed.entry(pos).or_default();
        o.add(g, n.max(-o.get(g)));
        if o.is_empty() {
            self.owed.remove(&pos);
        }
    }

    /// The store as `d` declares it, its pools re-indexed by frogport position. Frogports `d` no
    /// longer lists leave their pools in [`PackageStore::left`].
    fn redeclare(&mut self, d: &StoreDecl) {
        let mut by_pos: BTreeMap<BlockPos, Pool> = std::mem::take(&mut self.left);
        for (f, pool) in self.frogports.iter().zip(std::mem::take(&mut self.pools)) {
            if !pool.is_empty() {
                by_pos.entry(f.pos).or_default().merge(&pool);
            }
        }
        self.dim = d.dim;
        self.hops = d.hops.clone();
        self.frogports = d
            .frogports
            .iter()
            .map(|f| Frogport {
                district: f.district,
                inventory: f.inventory,
                star: f.star,
                pos: f.pos,
                conveyor: f.conveyor,
            })
            .collect();
        self.pools = self
            .frogports
            .iter()
            .map(|f| by_pos.remove(&f.pos).unwrap_or_default())
            .collect();
        self.left = by_pos;
        self.counts.declarations += 1;
    }

    /// Books an IND_CHAIN_PACKAGE into the pools. False when it names a frogport index the
    /// store's last declaration does not have (counted). `recorded`: the record's own game time.
    /// A census runs after the block entities of its tick, so it counts that tick's crossings,
    /// whose records reach the engine after it: a record no later than the last census moves the
    /// counts only.
    pub fn chain(&mut self, p: &ChainPackage, recorded: Tick) -> bool {
        let n = self.frogports.len();
        let packages = u64::from(p.packages);
        // The engine's own write (slice S4b): it came from what replays sent, never from the
        // chain, and no census counts a frogport's inventory.
        if p.op.is_some() && p.direction == ChainDirection::In {
            self.counts.written += packages;
            return (p.frogport as usize) < n;
        }
        // One package's contents toward its destination, for the replay's room and the writes.
        if p.direction == ChainDirection::Out
            && p.packages > 0
            && let Some(f) = p.destination.and_then(|d| self.frogports.get(d as usize))
        {
            let per: GoodVec = p
                .goods
                .iter()
                .map(|(g, a)| (g, a / i64::from(p.packages)))
                .filter(|e| e.1 > 0)
                .collect();
            if !per.is_empty() {
                self.batch.insert(f.pos, per);
            }
        }
        let counted = self.census_at.is_some_and(|c| recorded <= c);
        if counted {
            let known = match p.direction {
                ChainDirection::Out => (p.frogport as usize) < n,
                ChainDirection::In => (p.frogport as usize) < self.pools.len(),
            };
            if !known {
                self.counts.stale_records += 1;
                return false;
            }
            match p.direction {
                ChainDirection::Out => {
                    self.counts.packages_out += packages;
                    if p.destination.is_none() {
                        self.counts.unrouted += packages;
                    }
                }
                ChainDirection::In => self.counts.packages_in += packages,
            }
            self.counts.in_census += packages;
            return true;
        }
        match p.direction {
            ChainDirection::Out => {
                if p.frogport as usize >= n {
                    self.counts.stale_records += 1;
                    return false;
                }
                self.counts.packages_out += packages;
                match p.destination {
                    Some(dst) if (dst as usize) < n => {
                        self.pools[dst as usize].add(packages, &p.goods);
                    }
                    Some(_) => {
                        self.counts.stale_records += 1;
                        self.no_route.add(packages, &p.goods);
                    }
                    None => {
                        self.counts.unrouted += packages;
                        self.no_route.add(packages, &p.goods);
                    }
                }
            }
            ChainDirection::In => {
                let Some(pool) = self.pools.get_mut(p.frogport as usize) else {
                    self.counts.stale_records += 1;
                    return false;
                };
                self.counts.packages_in += packages;
                let short = pool.take(packages, &p.goods);
                self.counts.in_beyond += short.packages;
            }
        }
        true
    }

    /// Sets the pools to a census (IND_STORE_CENSUS), keeping the difference as the residue.
    /// `at`: the census's own game time (see [`PackageStore::chain`]).
    pub fn census(&mut self, at: Tick, c: &StoreCensus) {
        let before = self.held();
        let n = self.frogports.len();
        let mut pools = vec![Pool::default(); n];
        let mut no_route = Pool::default();
        let mut stale = 0;
        // Index n is the no-route pool; a destination the declaration does not have counts there.
        let mut slot = |dst: Option<u32>| match dst {
            Some(i) if (i as usize) < n => i as usize,
            Some(_) => {
                stale += 1;
                n
            }
            None => n,
        };
        for &(dst, packages) in &c.packages {
            let i = slot(dst);
            let pool = if i < n { &mut pools[i] } else { &mut no_route };
            pool.packages += u64::from(packages);
        }
        // A count is never negative; one that is would take from the pools.
        for &(dst, g, amount) in c.goods.iter().filter(|e| e.2 > 0) {
            let i = slot(dst);
            let pool = if i < n { &mut pools[i] } else { &mut no_route };
            pool.goods.add(g, amount);
        }
        self.pools = pools;
        self.no_route = no_route;
        self.left.clear();
        let after = self.held();
        self.residue = after.goods.minus(&before.goods);
        self.residue_packages = after.packages as i64 - before.packages as i64;
        self.census_at = Some(at);
        self.counts.censuses += 1;
        self.counts.stale_records += stale;
    }
}

/// What a store held for one frogport, moving with it to another store.
#[derive(Clone, Debug, Default)]
struct Taken {
    pool: Pool,
    owed: GoodVec,
    batch: Option<GoodVec>,
}

/// What [`declare`] did, for the engine's log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declared {
    New,
    /// Redeclared; true when its frogports (by position) or hops changed.
    Changed(bool),
    /// Removed; what it held went to `into` (true) or was dropped with it (false).
    Removed(bool),
    /// A removal of a store the engine does not have.
    Unknown,
}

/// Applies an IND_PACKAGE_STORE to the stores. Java sends a store that takes frogports from
/// others before those others' own declarations, and removals last; any other order keeps the
/// totals (pools wait in `left` or in `into`), only the split by frogport may differ until the
/// next census.
pub fn declare(
    stores: &mut crate::tracked::Tracked<StoreId, PackageStore>,
    d: &StoreDecl,
) -> Declared {
    if d.frogports.is_empty() {
        let Some(gone) = stores.remove(&d.store) else {
            return Declared::Unknown;
        };
        let Some(into) = d
            .into
            .filter(|&i| i != d.store)
            .and_then(|i| stores.get_mut(&i))
        else {
            return Declared::Removed(false);
        };
        for (f, pool) in gone.frogports.iter().zip(&gone.pools) {
            if pool.is_empty() {
                continue;
            }
            match into.index_of(f.pos) {
                Some(i) => into.pools[i].merge(pool),
                None => into.left.entry(f.pos).or_default().merge(pool),
            }
        }
        for (pos, pool) in &gone.left {
            match into.index_of(*pos) {
                Some(i) => into.pools[i].merge(pool),
                None => into.left.entry(*pos).or_default().merge(pool),
            }
        }
        into.no_route.merge(&gone.no_route);
        for (pos, owed) in &gone.owed {
            into.owed.entry(*pos).or_default().merge(owed);
        }
        for (pos, b) in &gone.batch {
            into.batch.entry(*pos).or_insert_with(|| b.clone());
        }
        return Declared::Removed(true);
    }
    // What other stores held for the frogports coming from them.
    let mut brought: Vec<(BlockPos, Taken)> = Vec::new();
    for f in &d.frogports {
        let Some(from) = f.from.filter(|&s| s != d.store) else {
            continue;
        };
        if let Some(other) = stores.get_mut(&from) {
            brought.push((f.pos, other.take_for(f.pos)));
        }
    }
    let new = !stores.contains_key(&d.store);
    let s = stores.get_or_insert_with(d.store, || PackageStore::new(d.store, d.dim));
    let before: Vec<BlockPos> = s.frogports.iter().map(|f| f.pos).collect();
    let hops_before = s.hops.clone();
    s.redeclare(d);
    let moved = hops_before != s.hops || !s.frogports.iter().map(|f| f.pos).eq(before);
    for (pos, t) in brought {
        s.bring(pos, t);
    }
    if new {
        Declared::New
    } else {
        Declared::Changed(moved)
    }
}

persist_struct!(Pool { packages, goods });
persist_struct!(Frogport {
    district,
    inventory,
    star,
    pos,
    conveyor
});
persist_struct!(StoreCounts {
    packages_out,
    packages_in,
    unrouted,
    in_beyond,
    censuses,
    declarations,
    stale_records,
    in_census,
    written
});
persist_struct!(PackageStore {
    id,
    dim,
    frogports,
    hops,
    pools,
    no_route,
    left,
    residue,
    residue_packages,
    census_at,
    counts,
    owed,
    batch,
    writing
});

/// A store's page.
pub fn store_page(s: &PackageStore) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(STORE_MAGIC);
    s.put(&mut w);
    w.into_bytes()
}

/// A store from its page, which must be the page of store `id`.
pub fn read_store_page(id: u64, b: &[u8]) -> Result<PackageStore> {
    let mut r = Reader::new(b);
    let magic = r.u32()?;
    if magic != STORE_MAGIC {
        return Err(CodecError::Invalid(format!(
            "package store page magic {magic:#010x}, not {STORE_MAGIC:#010x}: an older format"
        )));
    }
    let s = PackageStore::get(&mut r)?;
    r.finish()?;
    if s.id.0 != id {
        return Err(CodecError::Invalid(format!(
            "package store {} in the page of store {id}",
            s.id.0
        )));
    }
    let tri = s.frogports.len() * s.frogports.len().saturating_sub(1) / 2;
    if s.pools.len() != s.frogports.len() || s.hops.len() != tri {
        return Err(CodecError::Invalid(format!(
            "package store {id}: {} frogports, {} pools, {} hops",
            s.frogports.len(),
            s.pools.len(),
            s.hops.len()
        )));
    }
    Ok(s)
}
