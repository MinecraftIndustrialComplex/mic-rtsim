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
//! - **The frontier** (slice S4c): a package Java holds on a boundary conveyor, its hand-off toward
//!   a frogport whose chunk does not tick refused, is a `CHAIN_HELD` with a key; the store keeps
//!   the held packages toward each frogport, oldest first ([`PackageStore::on_hold`]). A keyed
//!   `CHAIN_IN` strikes its key, a census clears them (nothing is held while all of it ticks), and
//!   a declaration of another epoch drops the keys of the one before (Java dropped them).
//! - **Pages**: one per store (`STORE_TABLE`, id = the store id), starting with [`STORE_MAGIC`].
//!
//! The pools above are the world's packages: no replay draws them, and nothing is written from
//! them (UR-1: the packages are on the chain). What replays send each other (slice S4b) is
//! virtual: a cluster's `PoolKey::Chain` pool while one holds it, else the store's
//! [`PackageStore::owed`], written into a loaded importer's frogport as real packages
//! (`Engine::store_write_wake`).

use std::collections::BTreeMap;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::codec::{CodecError, Reader, Result, Writer};
use rtsim_kernel::journal::OpId;

use crate::ids::{BlockPos, DistrictId, GoodVec, StoreId};
use crate::obs::{ChainDirection, ChainPackage, HeldKey, StoreCensus, StoreDecl};
use crate::persist::{Persist, persist_struct};

/// A store page starts with this, so a page of another format fails by name.
pub const STORE_MAGIC: u32 = u32::from_le_bytes(*b"IST3");

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
    /// Packages the frontier held (`CHAIN_HELD`), and keyed packages that landed (`CHAIN_IN`).
    pub held: u64,
    pub held_landed: u64,
    /// `CHAIN_HELD` records turned away: a key already held, of another epoch than the store's,
    /// or one the last census came after.
    pub held_stale: u64,
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
    /// The engine's writes toward each frogport (by position) that Java has not answered, oldest
    /// first: no other write is sent there while one is young (choice SC113), and a written
    /// `CHAIN_IN` of one of them is that write's answer, taken at the frogport the write named.
    pub writing: BTreeMap<BlockPos, Vec<Sent>>,
    /// The last written `CHAIN_IN` taken toward each frogport (by position): a record of no write
    /// the engine still waits for, no later than the mark, was taken already (choice SC113).
    pub marks: BTreeMap<BlockPos, WriteMark>,
    /// The frontier's epoch of the last declaration (Java's, nonzero; it changes at every
    /// announce and starts anywhere): keys of any other are Java's no more.
    pub epoch: u32,
    /// The packages the frontier holds toward each frogport (by position), oldest first: they are
    /// in the world, booked in [`PackageStore::pools`] like any other (choice SC137).
    pub on_hold: BTreeMap<BlockPos, Vec<Held>>,
}

/// One package the frontier holds on a boundary conveyor (`CHAIN_HELD`, choice SC137).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub key: HeldKey,
    pub packages: u64,
    pub goods: GoodVec,
    /// The boundary conveyor it circles.
    pub at: BlockPos,
    /// The record's own game time.
    pub since: Tick,
}

/// One write the engine sent toward a frogport that Java has not answered yet (choice SC113).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sent {
    pub op: OpId,
    /// When the engine handed it on (`Engine::drain_commands`); None while it waits in the
    /// engine's own outbox, so one long step never takes its answer as lost.
    pub at: Option<Tick>,
}

/// The unanswered writes kept toward one frogport: past it the oldest is taken as lost.
pub const WRITES_KEPT: usize = 8;

/// The last written `CHAIN_IN` the engine took toward one frogport (choice SC113): its record's
/// own game time and the op ids taken at that time. A frogport's chunk carries such a record until
/// the engine acknowledges it; one that unloads first is saved with it and journals it again at its
/// next load, after the op id may be forgotten (WORLD_SAVED). Writes into one frogport are answered
/// in order, so a record no later than the mark, of an op the mark holds when as late, is that one.
/// A record of a write the engine still waits for is never one: after a crash the world's clock
/// goes back to its last save, and Java's next writes are recorded before a mark that a record
/// from the lost stretch set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteMark {
    pub at: Tick,
    pub ops: Vec<OpId>,
}

impl WriteMark {
    /// A written record at `recorded` of `op` that was taken already.
    pub fn covers(&self, recorded: Tick, op: OpId) -> bool {
        recorded < self.at || (recorded == self.at && self.ops.contains(&op))
    }

    fn add(&mut self, recorded: Tick, op: OpId) {
        if recorded > self.at {
            self.at = recorded;
            self.ops.clear();
        }
        if recorded == self.at && !self.ops.contains(&op) {
            self.ops.push(op);
        }
    }

    /// The later of two marks (both ops when as late).
    fn merge(&mut self, o: &WriteMark) {
        for &op in &o.ops {
            self.add(o.at, op);
        }
    }
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
            writing: self.writing.remove(&pos),
            mark: self.marks.remove(&pos),
            held: self.on_hold.remove(&pos).unwrap_or_default(),
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
        if let Some(w) = t.writing {
            self.add_writing(pos, &w);
        }
        if let Some(m) = t.mark {
            self.marks.entry(pos).or_default().merge(&m);
        }
        self.add_held(pos, t.held);
    }

    /// Adds held packages toward `pos` (another store's), keeping the oldest first and each key once.
    fn add_held(&mut self, pos: BlockPos, held: Vec<Held>) {
        if held.is_empty() {
            return;
        }
        let e = self.on_hold.entry(pos).or_default();
        for h in held {
            if !e.iter().any(|x| x.key == h.key) {
                e.push(h);
            }
        }
        e.sort_by_key(|h| (h.since, h.key));
    }

    /// Whether the store holds a package with `key`.
    pub fn holds(&self, key: HeldKey) -> bool {
        self.on_hold.values().flatten().any(|h| h.key == key)
    }

    /// Strikes the package with `key` from the held ones (it landed); returns it.
    pub fn strike(&mut self, key: HeldKey) -> Option<Held> {
        let pos = *self
            .on_hold
            .iter()
            .find(|(_, l)| l.iter().any(|h| h.key == key))?
            .0;
        let l = self.on_hold.get_mut(&pos).expect("found");
        let i = l.iter().position(|h| h.key == key).expect("found");
        let h = l.remove(i);
        if l.is_empty() {
            self.on_hold.remove(&pos);
        }
        Some(h)
    }

    /// Packages held toward the frogport at `pos`.
    pub fn held_toward(&self, pos: BlockPos) -> &[Held] {
        self.on_hold.get(&pos).map_or(&[], |l| l.as_slice())
    }

    fn add_writing(&mut self, pos: BlockPos, w: &[Sent]) {
        let e = self.writing.entry(pos).or_default();
        for s in w {
            if !e.iter().any(|x| x.op == s.op) {
                e.push(*s);
            }
        }
        let over = e.len().saturating_sub(WRITES_KEPT);
        e.drain(..over);
    }

    /// A written `CHAIN_IN` at `recorded` of `op` toward `pos`, of no write the engine still
    /// waits for, that the engine took already.
    pub fn written_again(&self, pos: BlockPos, recorded: Tick, op: OpId) -> bool {
        self.marks.get(&pos).is_some_and(|m| m.covers(recorded, op))
    }

    /// The engine wrote `op` toward `pos`; it waits in the engine's outbox. Returns how many older
    /// unanswered writes it pushed out (taken as lost, past [`WRITES_KEPT`]).
    pub fn sent_write(&mut self, pos: BlockPos, op: OpId) -> u64 {
        let e = self.writing.entry(pos).or_default();
        e.push(Sent { op, at: None });
        let over = e.len().saturating_sub(WRITES_KEPT);
        e.drain(..over);
        over as u64
    }

    /// The engine handed write `op` on at `at`. True when this store sent it.
    pub fn write_drained(&mut self, op: OpId, at: Tick) -> bool {
        match self
            .writing
            .values_mut()
            .flat_map(|w| w.iter_mut())
            .find(|s| s.op == op)
        {
            Some(s) => {
                s.at.get_or_insert(at);
                true
            }
            None => false,
        }
    }

    /// Whether a write toward `pos` still holds the next at `at`: one in the engine's outbox, or
    /// one handed on less than `lost` ticks ago and not answered.
    pub fn write_pending(&self, pos: BlockPos, at: Tick, lost: u64) -> bool {
        self.writing
            .get(&pos)
            .is_some_and(|w| w.iter().any(|s| s.at.is_none_or(|t| at < t + lost)))
    }

    /// Whether a write toward `pos` was handed on `forget` ticks or more before `at`.
    pub fn writes_to_forget(&self, pos: BlockPos, at: Tick, forget: u64) -> bool {
        self.writing
            .get(&pos)
            .is_some_and(|w| w.iter().any(|s| s.at.is_some_and(|t| at >= t + forget)))
    }

    /// Takes as lost the writes toward `pos` handed on `forget` ticks or more before `at`; returns
    /// how many.
    pub fn forget_writes(&mut self, pos: BlockPos, at: Tick, forget: u64) -> u64 {
        let Some(w) = self.writing.get_mut(&pos) else {
            return 0;
        };
        let before = w.len();
        w.retain(|s| s.at.is_none_or(|t| at < t + forget));
        let gone = (before - w.len()) as u64;
        if w.is_empty() {
            self.writing.remove(&pos);
        }
        gone
    }

    /// The frogport (by position) of the unanswered write `op`, when this store sent it.
    pub fn writing_at(&self, op: OpId) -> Option<BlockPos> {
        self.writing
            .iter()
            .find(|(_, w)| w.iter().any(|s| s.op == op))
            .map(|(p, _)| *p)
    }

    /// Java answered write `op` (toward any frogport). True when this store sent it.
    pub fn write_answered(&mut self, op: OpId) -> bool {
        let Some(pos) = self.writing_at(op) else {
            return false;
        };
        let w = self.writing.get_mut(&pos).expect("found");
        w.retain(|s| s.op != op);
        if w.is_empty() {
            self.writing.remove(&pos);
        }
        true
    }

    /// A written `CHAIN_IN` of `op` at `recorded` toward `pos` is taken: marked, and answered.
    pub fn take_written(&mut self, pos: BlockPos, recorded: Tick, op: OpId) {
        self.marks.entry(pos).or_default().add(recorded, op);
        self.write_answered(op);
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
        self.new_epoch(d.epoch);
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

    /// Java's frontier is at `epoch`: keys of any other epoch are dropped (Java no longer holds
    /// them). Epochs are compared for equality only: Java starts them anywhere (choice SC136).
    fn new_epoch(&mut self, epoch: u32) {
        if epoch == self.epoch {
            return;
        }
        self.epoch = epoch;
        self.on_hold.retain(|_, l| {
            l.retain(|h| h.key.epoch == epoch);
            !l.is_empty()
        });
    }

    /// A `CHAIN_HELD`: the package joins those held toward its destination, unless its key is held
    /// already, of another epoch than the last declaration's, or recorded no later than the last
    /// census (which cleared them).
    fn hold(&mut self, p: &ChainPackage, recorded: Tick) -> bool {
        let Some(f) = p.destination.and_then(|d| self.frogports.get(d as usize)) else {
            self.counts.stale_records += 1;
            return false;
        };
        let pos = f.pos;
        let Some(key) = p.key else {
            self.counts.held_stale += 1;
            return false;
        };
        if key.epoch != self.epoch
            || self.census_at.is_some_and(|c| recorded <= c)
            || self.holds(key)
        {
            self.counts.held_stale += 1;
            return false;
        }
        self.counts.held += u64::from(p.packages);
        self.add_held(
            pos,
            vec![Held {
                key,
                packages: u64::from(p.packages),
                goods: p.goods.clone(),
                at: p.pos,
                since: recorded,
            }],
        );
        true
    }

    /// Books an IND_CHAIN_PACKAGE into the pools. False when it names a frogport index the
    /// store's last declaration does not have (counted). `recorded`: the record's own game time.
    /// A census runs after the block entities of its tick, so it counts that tick's crossings,
    /// whose records reach the engine after it: a record no later than the last census moves the
    /// counts only.
    pub fn chain(&mut self, p: &ChainPackage, recorded: Tick) -> bool {
        let n = self.frogports.len();
        let packages = u64::from(p.packages);
        // The frontier's hold: the package's `CHAIN_OUT` booked it already.
        if p.direction == ChainDirection::Held {
            return self.hold(p, recorded);
        }
        // The engine's own write (slice S4b): it came from what replays sent, never from the
        // chain, and no census counts a frogport's inventory. The store it is taken at counts it
        // (`counts.written`), which a redeclaration may have made another.
        if p.op.is_some() && p.direction == ChainDirection::In {
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
                ChainDirection::Out | ChainDirection::Held => (p.frogport as usize) < n,
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
                ChainDirection::Held => {}
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
            ChainDirection::Held => {}
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
        // Nothing is held while every chunk of the store ticks: Java cleared its keys too.
        self.on_hold.clear();
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
    writing: Option<Vec<Sent>>,
    mark: Option<WriteMark>,
    held: Vec<Held>,
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
        for (pos, w) in &gone.writing {
            into.add_writing(*pos, w);
        }
        for (pos, m) in &gone.marks {
            into.marks.entry(*pos).or_default().merge(m);
        }
        for (pos, h) in gone.on_hold {
            into.add_held(pos, h);
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
    written,
    held,
    held_landed,
    held_stale
});
persist_struct!(Sent { op, at });
persist_struct!(WriteMark { at, ops });
persist_struct!(HeldKey { epoch, serial });
persist_struct!(Held {
    key,
    packages,
    goods,
    at,
    since
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
    writing,
    marks,
    epoch,
    on_hold
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
