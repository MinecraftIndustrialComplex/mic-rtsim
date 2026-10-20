// SPDX-License-Identifier: AGPL-3.0-or-later
//! The P4 storage bench (DECISIONS.md P4, PLAN.md §17): the kernel's `ColumnTable` (through the
//! `RowStore` facade) against `bevy_ecs` storage, on the same component types, the same rows and the
//! same operations in the same order.
//!
//! bevy_ecs runs with no schedule, no systems and no executor: every query is a `QueryState` built
//! once and driven directly, which is how a facade over bevy storage would use it. Each bevy step
//! first calls `World::increment_change_tick`, as a system run would, so writes store a new change
//! tick; bevy always keeps change ticks, so they are part of its cost.
//!
//! Patterns, each at every size in `P4_SIZES` (see `p4_storage_bench::Config`):
//! - a: stream one field (hunger += 1 over all rows).
//! - b: stream two fields (pos += vel over all rows).
//! - c: random access by stored handle (10% of rows, fixed pseudo-random order): read one component,
//!   write another.
//! - d: churn (remove 1% and insert 1% per step) then a; d0 is the churn alone.
//! - d2: tag churn: 1% of rows change their tag per step (an archetype move in bevy). Not in the
//!   brief; added because tags are where bevy moves rows between tables.
//! - e: sparse filter: touch only the 1% of rows that carry a tag.
//! - f: wide rows: a 352-byte notable (SCALE.md §9) as four components; touch one 8-byte field.
//! - g: memory per row.
//! - h: insert cost: build a store of n rows.
//!
//! Before timing anything, every pattern runs a few steps on a small world in every variant and
//! checks that all variants end in the same state.

use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, With};
use bevy_ecs::world::World;
use p4_storage_bench::{
    Config, Counting, Machine, MemRecord, Record, Report, SplitMix64, Variant, heap_live,
    run_group, timed,
};
use rtsim_kernel::store::{ColumnTable, RowId, RowStore};

#[global_allocator]
static ALLOC: Counting = Counting;

const SEED_AGENTS: u64 = 0x5045_5034_0000_0001;
const SEED_TAGS: u64 = 0x5045_5034_0000_0002;
const SEED_LOOKUP: u64 = 0x5045_5034_0000_0003;
const SEED_CHURN: u64 = 0x5045_5034_0000_0004;
const SEED_FRESH: u64 = 0x5045_5034_0000_0005;
const SEED_NOTABLES: u64 = 0x5045_5034_0000_0006;
const SEED_RETAG: u64 = 0x5045_5034_0000_0007;

/// Rows in the correctness check, and steps it runs.
const VERIFY_N: usize = 20_000;
const VERIFY_STEPS: usize = 7;

// ---------------------------------------------------------------------------------------------
// Components: one set of types for both backends.

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Pos {
    x: i32,
    z: i32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Vel {
    dx: i16,
    dz: i16,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Hunger(u16);

/// An owner handle in its 6-byte wire form.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Owner([u8; 6]);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Flags(u8);

/// Bit 0 of `Flags`: the tag, in the flag-column variants.
const TAG: u8 = 1;

/// The tag as a bevy marker in table storage: a tagged row lives in another table.
#[derive(Component, Clone, Copy, Debug, Default)]
struct Tagged;

/// The tag as a bevy marker in sparse-set storage: a tagged row changes archetype, not table.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
struct TaggedSparse;

/// A side-table row naming a tagged agent: the columns' way to hold a sparse subset.
#[derive(Clone, Copy, Debug)]
struct TagOf(RowId);

type Agent = (Pos, Vel, Hunger, Owner, Flags);
type Agents = ColumnTable<Agent>;
type Side = ColumnTable<(TagOf,)>;
const AGENT_PAYLOAD: usize = 8 + 4 + 2 + 6 + 1;

/// A memory slot or a sentiment entry: a 6-byte handle, a value byte and a spare (SCALE.md §9).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slot8 {
    who: [u8; 6],
    value: u8,
    spare: u8,
}

/// The 8-byte field the wide pattern touches.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Wealth(i64);

/// The rest of the 96-byte core.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct NotableCore([u64; 11]);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Memories([Slot8; 16]);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Sentiments([Slot8; 16]);

type Notable = (NotableCore, Wealth, Memories, Sentiments);
type Notables = ColumnTable<Notable>;

/// The same notable as one 352-byte struct: the array-of-structs reference for pattern f.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct NotableAos {
    core: NotableCore,
    wealth: Wealth,
    memories: Memories,
    sentiments: Sentiments,
}

const NOTABLE_PAYLOAD: usize = 352;
const _: () = assert!(std::mem::size_of::<NotableAos>() == NOTABLE_PAYLOAD);
const _: () = assert!(
    std::mem::size_of::<Pos>()
        + std::mem::size_of::<Vel>()
        + std::mem::size_of::<Hunger>()
        + std::mem::size_of::<Owner>()
        + std::mem::size_of::<Flags>()
        == AGENT_PAYLOAD
);

// ---------------------------------------------------------------------------------------------
// Data: generated once per size from fixed seeds, then copied into every store.

fn agent(r: &mut SplitMix64) -> Agent {
    let a = r.next_u64();
    let b = r.next_u64();
    let c = r.next_u64().to_le_bytes();
    (
        Pos {
            x: (a as u32 % 2_000_001) as i32 - 1_000_000,
            z: ((a >> 32) as u32 % 2_000_001) as i32 - 1_000_000,
        },
        Vel {
            dx: (b % 9) as i16 - 4,
            dz: ((b >> 8) % 9) as i16 - 4,
        },
        Hunger(((b >> 16) % 1000) as u16),
        Owner([c[0], c[1], c[2], c[3], c[4], c[5]]),
        Flags((b >> 56) as u8 & !TAG),
    )
}

fn gen_agents(n: usize, seed: u64) -> Vec<Agent> {
    let mut r = SplitMix64::new(seed);
    (0..n).map(|_| agent(&mut r)).collect()
}

/// Which rows carry the tag: about 1%, scattered.
fn gen_tags(n: usize) -> Vec<bool> {
    let mut r = SplitMix64::new(SEED_TAGS);
    (0..n).map(|_| r.below(100) == 0).collect()
}

/// Row positions (insertion order) of the handles pattern c looks up: 10% of n, with repeats.
fn gen_lookups(n: usize) -> Vec<usize> {
    let mut r = SplitMix64::new(SEED_LOOKUP);
    (0..n / 10).map(|_| r.below(n)).collect()
}

fn slots(r: &mut SplitMix64) -> [Slot8; 16] {
    std::array::from_fn(|_| {
        let b = r.next_u64().to_le_bytes();
        Slot8 {
            who: [b[0], b[1], b[2], b[3], b[4], b[5]],
            value: b[6],
            spare: 0,
        }
    })
}

fn gen_notables(n: usize) -> Vec<NotableAos> {
    let mut r = SplitMix64::new(SEED_NOTABLES);
    (0..n)
        .map(|_| NotableAos {
            core: NotableCore(std::array::from_fn(|_| r.next_u64())),
            wealth: Wealth((r.next_u64() % 1_000_000) as i64),
            memories: Memories(slots(&mut r)),
            sentiments: Sentiments(slots(&mut r)),
        })
        .collect()
}

fn notable_row(a: &NotableAos) -> Notable {
    (a.core, a.wealth, a.memories, a.sentiments)
}

// ---------------------------------------------------------------------------------------------
// Builders. Both insert one row at a time, in the same order; `ids[i]` is row i's handle.

fn build_cols(rows: &[Agent]) -> (Agents, Vec<RowId>) {
    let mut t = Agents::new();
    let ids = rows.iter().map(|r| t.insert(*r)).collect();
    (t, ids)
}

fn build_bevy(rows: &[Agent]) -> (World, Vec<Entity>) {
    let mut w = World::new();
    let ids = rows.iter().map(|r| w.spawn(*r).id()).collect();
    (w, ids)
}

/// Columns with the tag as bit 0 of `Flags`.
fn build_cols_flagged(rows: &[Agent], tags: &[bool]) -> (Agents, Vec<RowId>) {
    let mut t = Agents::new();
    let ids = rows
        .iter()
        .zip(tags)
        .map(|(r, &tag)| {
            let mut r = *r;
            if tag {
                r.4.0 |= TAG;
            }
            t.insert(r)
        })
        .collect();
    (t, ids)
}

/// Columns with the tag as a side table of handles; the tagged list holds (agent, side row).
fn build_cols_side(rows: &[Agent], tags: &[bool]) -> (Agents, Side, Vec<RowId>) {
    let mut t = Agents::new();
    let mut side = Side::new();
    let ids = rows
        .iter()
        .zip(tags)
        .map(|(r, &tag)| {
            let id = t.insert(*r);
            if tag {
                side.insert((TagOf(id),));
            }
            id
        })
        .collect();
    (t, side, ids)
}

/// bevy with the tag as marker `M`, spawned with the row so tagged rows start in their own archetype.
fn build_bevy_tagged<M: Component + Default>(
    rows: &[Agent],
    tags: &[bool],
) -> (World, Vec<Entity>) {
    let mut w = World::new();
    let ids = rows
        .iter()
        .zip(tags)
        .map(|(r, &tag)| {
            if tag {
                w.spawn((*r, M::default())).id()
            } else {
                w.spawn(*r).id()
            }
        })
        .collect();
    (w, ids)
}

// ---------------------------------------------------------------------------------------------
// Steps. The same function runs in the correctness check and under the timer.

type QHunger = QueryState<&'static mut Hunger>;
type QMove = QueryState<(&'static mut Pos, &'static Vel)>;
type QLookup = QueryState<(&'static Pos, &'static mut Hunger)>;
type QWealth = QueryState<&'static mut Wealth>;

// Slice kernels. Every variant that hands over plain slices runs the same out-of-line function,
// so they execute the same machine code at the same address and differ only in how they obtain
// the slices. A first build inlined the loop into each caller, and identical instructions then ran
// up to 25% apart depending on unrelated changes elsewhere in the binary (code layout; see
// bench/results/p4-storage.md). `.cargo/config.toml` also aligns every loop header to 64 bytes.

#[inline(never)]
fn bump_hunger(h: &mut [Hunger]) {
    for h in h {
        h.0 = h.0.saturating_add(1);
    }
}

#[inline(never)]
fn move_all(p: &mut [Pos], v: &[Vel]) {
    for (p, v) in p.iter_mut().zip(v) {
        add_vel(p, v);
    }
}

#[inline(never)]
fn bump_tagged(h: &mut [Hunger], f: &[Flags]) {
    for (h, f) in h.iter_mut().zip(f) {
        h.0 = h.0.saturating_add(u16::from(f.0 & TAG));
    }
}

#[inline(never)]
fn bump_wealth(w: &mut [Wealth]) {
    for w in w {
        w.0 = w.0.wrapping_add(1);
    }
}

// a: hunger += 1.

fn a_cols(t: &mut Agents) {
    bump_hunger(t.column_mut::<Hunger>());
}

fn a_bevy_iter(w: &mut World, q: &mut QHunger) {
    w.increment_change_tick();
    q.iter_mut(w).for_each(|mut h| h.0 = h.0.saturating_add(1));
}

fn a_bevy_contig(w: &mut World, q: &mut QHunger) {
    w.increment_change_tick();
    for mut h in q.contiguous_iter_mut(w).expect("dense query") {
        // DerefMut marks every row of the slice changed, then the kernel runs.
        bump_hunger(&mut h);
    }
}

fn a_bevy_bypass(w: &mut World, q: &mut QHunger) {
    w.increment_change_tick();
    for mut h in q.contiguous_iter_mut(w).expect("dense query") {
        bump_hunger(h.bypass_change_detection());
    }
}

// b: pos += vel.

fn add_vel(p: &mut Pos, v: &Vel) {
    p.x = p.x.wrapping_add(i32::from(v.dx));
    p.z = p.z.wrapping_add(i32::from(v.dz));
}

/// `RowStore` lends one column mutably at a time (`column_mut` borrows the whole table), so the
/// read column is copied out first.
fn b_cols_copy(t: &mut Agents, scratch: &mut Vec<Vel>) {
    scratch.clear();
    scratch.extend_from_slice(t.column::<Vel>());
    move_all(t.column_mut::<Pos>(), scratch);
}

/// What a two-column borrow on `ColumnTable` would give: both columns at once, no copy. Not in the
/// `RowStore` API today.
fn b_cols_split(t: &mut Agents) {
    let vel = t.column::<Vel>();
    let (ptr, len) = (vel.as_ptr(), vel.len());
    let pos = t.column_mut::<Pos>();
    // SAFETY: every column is its own `Vec` behind its own `Box`. `column_mut::<Pos>` reborrows the
    // table and the `Pos` vector only, never the `Vel` vector's buffer, and nothing resizes a column
    // while `vel` lives. So the `Vel` buffer stays valid and unaliased by `pos`.
    let vel = unsafe { std::slice::from_raw_parts(ptr, len) };
    move_all(pos, vel);
}

fn b_bevy_iter(w: &mut World, q: &mut QMove) {
    w.increment_change_tick();
    q.iter_mut(w).for_each(|(mut p, v)| add_vel(&mut p, v));
}

fn b_bevy_contig(w: &mut World, q: &mut QMove) {
    w.increment_change_tick();
    for (mut p, v) in q.contiguous_iter_mut(w).expect("dense query") {
        move_all(&mut p, v);
    }
}

fn b_bevy_bypass(w: &mut World, q: &mut QMove) {
    w.increment_change_tick();
    for (mut p, v) in q.contiguous_iter_mut(w).expect("dense query") {
        move_all(p.bypass_change_detection(), v);
    }
}

// c: random access by handle: read Pos, write Hunger.

fn c_cols(t: &mut Agents, ids: &[RowId]) {
    for &id in ids {
        let x = t.get::<Pos>(id).expect("live").x;
        let h = t.get_mut::<Hunger>(id).expect("live");
        h.0 = h.0.wrapping_add((x & 7) as u16);
    }
}

fn c_bevy_query(w: &mut World, q: &mut QLookup, ids: &[Entity]) {
    w.increment_change_tick();
    let mut query = q.query_mut(w);
    for &e in ids {
        let (p, mut h) = query.get_mut(e).expect("live");
        h.0 = h.0.wrapping_add((p.x & 7) as u16);
    }
}

fn c_bevy_world(w: &mut World, ids: &[Entity]) {
    w.increment_change_tick();
    for &e in ids {
        let x = w.get::<Pos>(e).expect("live").x;
        let mut h = w.get_mut::<Hunger>(e).expect("live");
        h.0 = h.0.wrapping_add((x & 7) as u16);
    }
}

// d: churn. The harness keeps the live handles in a list; both backends draw the same positions
// from it and insert the same fresh rows.

trait ChurnStore {
    type H: Copy;
    fn insert_row(&mut self, row: Agent) -> Self::H;
    fn remove_row(&mut self, h: Self::H);
}

impl ChurnStore for Agents {
    type H = RowId;
    fn insert_row(&mut self, row: Agent) -> RowId {
        self.insert(row)
    }
    fn remove_row(&mut self, h: RowId) {
        let removed = self.remove(h);
        debug_assert!(removed);
    }
}

impl ChurnStore for World {
    type H = Entity;
    fn insert_row(&mut self, row: Agent) -> Entity {
        self.spawn(row).id()
    }
    fn remove_row(&mut self, h: Entity) {
        let removed = self.despawn(h);
        debug_assert!(removed);
    }
}

struct Churn<H> {
    live: Vec<H>,
    rng: SplitMix64,
    cursor: usize,
    per_step: usize,
}

impl<H: Copy> Churn<H> {
    fn new(ids: Vec<H>) -> Self {
        let per_step = (ids.len() / 100).max(1);
        Churn {
            live: ids,
            rng: SplitMix64::new(SEED_CHURN),
            cursor: 0,
            per_step,
        }
    }

    fn step<S: ChurnStore<H = H>>(&mut self, s: &mut S, fresh: &[Agent]) {
        for _ in 0..self.per_step {
            let i = self.rng.below(self.live.len());
            s.remove_row(self.live.swap_remove(i));
        }
        for _ in 0..self.per_step {
            let row = fresh[self.cursor];
            self.cursor = (self.cursor + 1) % fresh.len();
            self.live.push(s.insert_row(row));
        }
    }
}

// d2: tag churn. The harness keeps tagged and untagged lists; per step `per_step` tagged rows lose
// the tag and `per_step` untagged rows gain it, so 1% of rows change and the tagged count holds.

trait TagStore {
    type H: Copy;
    /// What the tagged list keeps for a tagged row.
    type T: Copy;
    fn tag(&mut self, h: Self::H) -> Self::T;
    fn untag(&mut self, t: Self::T) -> Self::H;
}

struct FlagTags(Agents);

impl TagStore for FlagTags {
    type H = RowId;
    type T = RowId;
    fn tag(&mut self, h: RowId) -> RowId {
        self.0.get_mut::<Flags>(h).expect("live").0 |= TAG;
        h
    }
    fn untag(&mut self, h: RowId) -> RowId {
        self.0.get_mut::<Flags>(h).expect("live").0 &= !TAG;
        h
    }
}

struct SideTags(Agents, Side);

impl TagStore for SideTags {
    type H = RowId;
    type T = (RowId, RowId);
    fn tag(&mut self, h: RowId) -> (RowId, RowId) {
        (h, self.1.insert((TagOf(h),)))
    }
    fn untag(&mut self, (h, side): (RowId, RowId)) -> RowId {
        let removed = self.1.remove(side);
        debug_assert!(removed);
        h
    }
}

struct BevyTags<M>(World, std::marker::PhantomData<M>);

impl<M: Component + Default> TagStore for BevyTags<M> {
    type H = Entity;
    type T = Entity;
    fn tag(&mut self, e: Entity) -> Entity {
        self.0.entity_mut(e).insert(M::default());
        e
    }
    fn untag(&mut self, e: Entity) -> Entity {
        self.0.entity_mut(e).remove::<M>();
        e
    }
}

struct Retag<H, T> {
    tagged: Vec<T>,
    untagged: Vec<H>,
    rng: SplitMix64,
    per_step: usize,
}

impl<H: Copy, T: Copy> Retag<H, T> {
    fn new(tagged: Vec<T>, untagged: Vec<H>, n: usize) -> Self {
        Retag {
            tagged,
            untagged,
            rng: SplitMix64::new(SEED_RETAG),
            per_step: (n / 200).max(1),
        }
    }

    fn step<S: TagStore<H = H, T = T>>(&mut self, s: &mut S) {
        for _ in 0..self.per_step {
            let i = self.rng.below(self.tagged.len());
            let h = s.untag(self.tagged.swap_remove(i));
            self.untagged.push(h);
        }
        for _ in 0..self.per_step {
            let i = self.rng.below(self.untagged.len());
            let t = s.tag(self.untagged.swap_remove(i));
            self.tagged.push(t);
        }
    }
}

// e: sparse filter: hunger += 1 on tagged rows only.

/// Scan the flag column, then touch the hunger column at the tagged rows. The scan collects row
/// positions first because `RowStore` lends one column at a time.
fn e_cols_flag(t: &mut Agents, scratch: &mut Vec<u32>) {
    scratch.clear();
    scratch.extend(
        t.column::<Flags>()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.0 & TAG != 0)
            .map(|(i, _)| i as u32),
    );
    let h = t.column_mut::<Hunger>();
    for &i in scratch.iter() {
        let h = &mut h[i as usize];
        h.0 = h.0.saturating_add(1);
    }
}

/// One branchless pass over both columns: add the tag bit to hunger. Needs the two-column borrow
/// of `b_cols_split`, which `RowStore` does not offer.
fn e_cols_flag_split(t: &mut Agents) {
    let flags = t.column::<Flags>();
    let (ptr, len) = (flags.as_ptr(), flags.len());
    let hunger = t.column_mut::<Hunger>();
    // SAFETY: as in `b_cols_split`: distinct columns, nothing resizes while `flags` lives.
    let flags = unsafe { std::slice::from_raw_parts(ptr, len) };
    bump_tagged(hunger, flags);
}

/// Walk the side table and look each tagged agent up by handle.
fn e_cols_side(t: &mut Agents, side: &Side) {
    for TagOf(id) in side.column::<TagOf>() {
        let h = t.get_mut::<Hunger>(*id).expect("live");
        h.0 = h.0.saturating_add(1);
    }
}

fn e_bevy<M: Component>(w: &mut World, q: &mut QueryState<&'static mut Hunger, With<M>>) {
    w.increment_change_tick();
    q.iter_mut(w).for_each(|mut h| h.0 = h.0.saturating_add(1));
}

// f: wealth += 1 on 352-byte rows.

fn f_cols(t: &mut Notables) {
    bump_wealth(t.column_mut::<Wealth>());
}

fn f_bevy_iter(w: &mut World, q: &mut QWealth) {
    w.increment_change_tick();
    q.iter_mut(w).for_each(|mut x| x.0 = x.0.wrapping_add(1));
}

fn f_bevy_contig(w: &mut World, q: &mut QWealth) {
    w.increment_change_tick();
    for mut s in q.contiguous_iter_mut(w).expect("dense query") {
        bump_wealth(&mut s);
    }
}

fn f_bevy_bypass(w: &mut World, q: &mut QWealth) {
    w.increment_change_tick();
    for mut s in q.contiguous_iter_mut(w).expect("dense query") {
        bump_wealth(s.bypass_change_detection());
    }
}

fn f_aos(v: &mut [NotableAos]) {
    for n in v {
        n.wealth.0 = n.wealth.0.wrapping_add(1);
    }
}

// ---------------------------------------------------------------------------------------------
// Checksums for the correctness check: order-independent sums over every row.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AgentSum {
    rows: u64,
    x: i64,
    z: i64,
    vel: i64,
    hunger: u64,
    owner: u64,
    /// Flags without the tag bit.
    flags: u64,
}

impl AgentSum {
    fn add(&mut self, p: &Pos, v: &Vel, h: &Hunger, o: &Owner, f: &Flags) {
        self.rows += 1;
        self.x += i64::from(p.x);
        self.z += i64::from(p.z);
        self.vel += i64::from(v.dx) * 3 + i64::from(v.dz);
        self.hunger += u64::from(h.0);
        self.owner += o.0.iter().map(|&b| u64::from(b)).sum::<u64>();
        self.flags += u64::from(f.0 & !TAG);
    }
}

fn sum_cols(t: &Agents) -> AgentSum {
    let mut s = AgentSum::default();
    let (p, v, h, o, f) = (
        t.column::<Pos>(),
        t.column::<Vel>(),
        t.column::<Hunger>(),
        t.column::<Owner>(),
        t.column::<Flags>(),
    );
    for i in 0..t.len() {
        s.add(&p[i], &v[i], &h[i], &o[i], &f[i]);
    }
    s
}

fn sum_bevy(w: &mut World) -> AgentSum {
    let mut s = AgentSum::default();
    w.query::<(&Pos, &Vel, &Hunger, &Owner, &Flags)>()
        .iter(w)
        .for_each(|(p, v, h, o, f)| s.add(p, v, h, o, f));
    s
}

/// (tagged rows, sum of their x, sum of their hunger).
type TagSum = (u64, i64, u64);

fn tags_flag(t: &Agents) -> TagSum {
    let mut s = (0, 0, 0);
    for ((f, p), h) in t
        .column::<Flags>()
        .iter()
        .zip(t.column::<Pos>())
        .zip(t.column::<Hunger>())
    {
        if f.0 & TAG != 0 {
            s.0 += 1;
            s.1 += i64::from(p.x);
            s.2 += u64::from(h.0);
        }
    }
    s
}

fn tags_side(t: &Agents, side: &Side) -> TagSum {
    let mut s = (0, 0, 0);
    for TagOf(id) in side.column::<TagOf>() {
        s.0 += 1;
        s.1 += i64::from(t.get::<Pos>(*id).expect("live").x);
        s.2 += u64::from(t.get::<Hunger>(*id).expect("live").0);
    }
    s
}

fn tags_bevy<M: Component>(w: &mut World) -> TagSum {
    let mut s = (0, 0, 0);
    w.query_filtered::<(&Pos, &Hunger), With<M>>()
        .iter(w)
        .for_each(|(p, h)| {
            s.0 += 1;
            s.1 += i64::from(p.x);
            s.2 += u64::from(h.0);
        });
    s
}

fn same<T: PartialEq + std::fmt::Debug>(pattern: &str, sums: &[(&str, T)]) {
    for (name, s) in &sums[1..] {
        assert_eq!(
            s, &sums[0].1,
            "pattern {pattern}: {name} ended in another state than {}",
            sums[0].0
        );
    }
    println!(
        "  verified {pattern}: {} variants agree after {VERIFY_STEPS} steps",
        sums.len()
    );
}

/// Runs every pattern's steps on a small world in every variant and checks they agree.
fn verify() {
    let rows = gen_agents(VERIFY_N, SEED_AGENTS);
    let tags = gen_tags(VERIFY_N);
    let fresh = gen_agents(VERIFY_N / 10, SEED_FRESH);
    let lookups = gen_lookups(VERIFY_N);
    let k = VERIFY_STEPS;

    // a
    let (mut t, _) = build_cols(&rows);
    let (mut w1, _) = build_bevy(&rows);
    let (mut w2, _) = build_bevy(&rows);
    let (mut w3, _) = build_bevy(&rows);
    let (mut q1, mut q2, mut q3) = (w1.query(), w2.query(), w3.query());
    for _ in 0..k {
        a_cols(&mut t);
        a_bevy_iter(&mut w1, &mut q1);
        a_bevy_contig(&mut w2, &mut q2);
        a_bevy_bypass(&mut w3, &mut q3);
    }
    same(
        "a",
        &[
            ("cols", sum_cols(&t)),
            ("bevy iter", sum_bevy(&mut w1)),
            ("bevy contig", sum_bevy(&mut w2)),
            ("bevy bypass", sum_bevy(&mut w3)),
        ],
    );

    // b
    let (mut t1, _) = build_cols(&rows);
    let (mut t2, _) = build_cols(&rows);
    let (mut w1, _) = build_bevy(&rows);
    let (mut w2, _) = build_bevy(&rows);
    let (mut w3, _) = build_bevy(&rows);
    let (mut q1, mut q2, mut q3) = (w1.query(), w2.query(), w3.query());
    let mut scratch = Vec::new();
    for _ in 0..k {
        b_cols_copy(&mut t1, &mut scratch);
        b_cols_split(&mut t2);
        b_bevy_iter(&mut w1, &mut q1);
        b_bevy_contig(&mut w2, &mut q2);
        b_bevy_bypass(&mut w3, &mut q3);
    }
    same(
        "b",
        &[
            ("cols copy", sum_cols(&t1)),
            ("cols split", sum_cols(&t2)),
            ("bevy iter", sum_bevy(&mut w1)),
            ("bevy contig", sum_bevy(&mut w2)),
            ("bevy bypass", sum_bevy(&mut w3)),
        ],
    );

    // c
    let (mut t, ids) = build_cols(&rows);
    let cols_ids: Vec<RowId> = lookups.iter().map(|&i| ids[i]).collect();
    let (mut w1, ids1) = build_bevy(&rows);
    let (mut w2, ids2) = build_bevy(&rows);
    let e1: Vec<Entity> = lookups.iter().map(|&i| ids1[i]).collect();
    let e2: Vec<Entity> = lookups.iter().map(|&i| ids2[i]).collect();
    let mut q1 = w1.query();
    for _ in 0..k {
        c_cols(&mut t, &cols_ids);
        c_bevy_query(&mut w1, &mut q1, &e1);
        c_bevy_world(&mut w2, &e2);
    }
    same(
        "c",
        &[
            ("cols", sum_cols(&t)),
            ("bevy query", sum_bevy(&mut w1)),
            ("bevy world", sum_bevy(&mut w2)),
        ],
    );

    // d and d0
    let (mut t, ids) = build_cols(&rows);
    let (mut w, eids) = build_bevy(&rows);
    let mut q = w.query();
    let (mut ct, mut cw) = (Churn::new(ids), Churn::new(eids));
    for _ in 0..k {
        ct.step(&mut t, &fresh);
        a_cols(&mut t);
        cw.step(&mut w, &fresh);
        a_bevy_iter(&mut w, &mut q);
    }
    same("d", &[("cols", sum_cols(&t)), ("bevy", sum_bevy(&mut w))]);

    // d2
    let (t, ids) = build_cols_flagged(&rows, &tags);
    let mut fl = FlagTags(t);
    let (t, side, ids_s) = build_cols_side(&rows, &tags);
    let side_rows: Vec<RowId> = side.ids().to_vec();
    let mut sd = SideTags(t, side);
    let (w, eids) = build_bevy_tagged::<Tagged>(&rows, &tags);
    let mut bt = BevyTags::<Tagged>(w, std::marker::PhantomData);
    let (w, eids_s) = build_bevy_tagged::<TaggedSparse>(&rows, &tags);
    let mut bs = BevyTags::<TaggedSparse>(w, std::marker::PhantomData);
    let split = |ids: &[RowId]| -> (Vec<RowId>, Vec<RowId>) {
        let t = (0..ids.len())
            .filter(|&i| tags[i])
            .map(|i| ids[i])
            .collect();
        let u = (0..ids.len())
            .filter(|&i| !tags[i])
            .map(|i| ids[i])
            .collect();
        (t, u)
    };
    let split_e = |ids: &[Entity]| -> (Vec<Entity>, Vec<Entity>) {
        let t = (0..ids.len())
            .filter(|&i| tags[i])
            .map(|i| ids[i])
            .collect();
        let u = (0..ids.len())
            .filter(|&i| !tags[i])
            .map(|i| ids[i])
            .collect();
        (t, u)
    };
    let (tg, ug) = split(&ids);
    let mut r_fl = Retag::new(tg, ug, VERIFY_N);
    let (tg, ug) = split(&ids_s);
    let tg_s: Vec<(RowId, RowId)> = tg.into_iter().zip(side_rows).collect();
    let mut r_sd = Retag::new(tg_s, ug, VERIFY_N);
    let (tg, ug) = split_e(&eids);
    let mut r_bt = Retag::new(tg, ug, VERIFY_N);
    let (tg, ug) = split_e(&eids_s);
    let mut r_bs = Retag::new(tg, ug, VERIFY_N);
    for _ in 0..k {
        r_fl.step(&mut fl);
        r_sd.step(&mut sd);
        r_bt.step(&mut bt);
        r_bs.step(&mut bs);
    }
    same(
        "d2",
        &[
            ("cols flag", tags_flag(&fl.0)),
            ("cols side", tags_side(&sd.0, &sd.1)),
            ("bevy table marker", tags_bevy::<Tagged>(&mut bt.0)),
            ("bevy sparse marker", tags_bevy::<TaggedSparse>(&mut bs.0)),
        ],
    );

    // e
    let (mut t1, _) = build_cols_flagged(&rows, &tags);
    let (mut t2, side, _) = build_cols_side(&rows, &tags);
    let (mut t3, _) = build_cols_flagged(&rows, &tags);
    let (mut w1, _) = build_bevy_tagged::<Tagged>(&rows, &tags);
    let (mut w2, _) = build_bevy_tagged::<TaggedSparse>(&rows, &tags);
    let mut q1 = w1.query_filtered::<&mut Hunger, With<Tagged>>();
    let mut q2 = w2.query_filtered::<&mut Hunger, With<TaggedSparse>>();
    let mut scratch = Vec::new();
    for _ in 0..k {
        e_cols_flag(&mut t1, &mut scratch);
        e_cols_side(&mut t2, &side);
        e_cols_flag_split(&mut t3);
        e_bevy(&mut w1, &mut q1);
        e_bevy(&mut w2, &mut q2);
    }
    same(
        "e",
        &[
            ("cols flag", (sum_cols(&t1), tags_flag(&t1))),
            ("cols side", (sum_cols(&t2), tags_side(&t2, &side))),
            ("cols flag split", (sum_cols(&t3), tags_flag(&t3))),
            (
                "bevy table marker",
                (sum_bevy(&mut w1), tags_bevy::<Tagged>(&mut w1)),
            ),
            (
                "bevy sparse marker",
                (sum_bevy(&mut w2), tags_bevy::<TaggedSparse>(&mut w2)),
            ),
        ],
    );

    // f
    let notables = gen_notables(VERIFY_N);
    let mut nt = Notables::new();
    let mut nw: Vec<World> = (0..3).map(|_| World::new()).collect();
    for a in &notables {
        nt.insert(notable_row(a));
        for w in &mut nw {
            w.spawn(notable_row(a));
        }
    }
    let mut aos = notables.clone();
    let mut qs: Vec<QWealth> = nw.iter_mut().map(|w| w.query()).collect();
    for _ in 0..k {
        f_cols(&mut nt);
        f_bevy_iter(&mut nw[0], &mut qs[0]);
        f_bevy_contig(&mut nw[1], &mut qs[1]);
        f_bevy_bypass(&mut nw[2], &mut qs[2]);
        f_aos(&mut aos);
    }
    let wealth_bevy = |w: &mut World| -> i64 {
        let mut s = 0;
        w.query::<&Wealth>().iter(w).for_each(|x| s += x.0);
        s
    };
    let sums = [
        (
            "cols",
            nt.column::<Wealth>().iter().map(|x| x.0).sum::<i64>(),
        ),
        ("bevy iter", wealth_bevy(&mut nw[0])),
        ("bevy contig", wealth_bevy(&mut nw[1])),
        ("bevy bypass", wealth_bevy(&mut nw[2])),
        ("aos", aos.iter().map(|x| x.wealth.0).sum::<i64>()),
    ];
    same("f", &sums);
}

// ---------------------------------------------------------------------------------------------
// Patterns under the timer.

const NOTE_ITER: &str = "QueryState::iter_mut().for_each; one change tick written per row";
const NOTE_CONTIG: &str =
    "QueryState::contiguous_iter_mut (bevy 0.20 table slices); change ticks filled per slice";
const NOTE_BYPASS: &str =
    "contiguous_iter_mut + bypass_change_detection: no tick writes; the floor, not normal use";

fn pattern_a(cfg: &Config, n: usize, rows: &[Agent]) -> Vec<Record> {
    let (mut t, _) = build_cols(rows);
    let mut w: Vec<World> = (0..3).map(|_| build_bevy(rows).0).collect();
    let mut q: Vec<QHunger> = w.iter_mut().map(|w| w.query()).collect();
    let (mut w3, mut q3) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w2, mut q2) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w1, mut q1) = (w.pop().expect("3"), q.pop().expect("3"));
    let nf = n as f64;
    let variants = vec![
        Variant::new(
            "cols",
            "cols",
            "ns/row",
            nf,
            "column_mut::<Hunger>()",
            timed(move || a_cols(black_box(&mut t))),
        ),
        Variant::new(
            "bevy iter",
            "bevy",
            "ns/row",
            nf,
            NOTE_ITER,
            timed(move || a_bevy_iter(black_box(&mut w1), &mut q1)),
        ),
        Variant::new(
            "bevy contig",
            "bevy",
            "ns/row",
            nf,
            NOTE_CONTIG,
            timed(move || a_bevy_contig(black_box(&mut w2), &mut q2)),
        ),
        Variant::new(
            "bevy contig bypass",
            "bevy",
            "ns/row",
            nf,
            NOTE_BYPASS,
            timed(move || a_bevy_bypass(black_box(&mut w3), &mut q3)),
        ),
    ];
    run_group(
        cfg,
        "a",
        "stream one field: hunger += 1, all rows",
        n,
        variants,
    )
}

fn pattern_b(cfg: &Config, n: usize, rows: &[Agent]) -> Vec<Record> {
    let (mut t1, _) = build_cols(rows);
    let (mut t2, _) = build_cols(rows);
    let mut w: Vec<World> = (0..3).map(|_| build_bevy(rows).0).collect();
    let mut q: Vec<QMove> = w.iter_mut().map(|w| w.query()).collect();
    let (mut w3, mut q3) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w2, mut q2) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w1, mut q1) = (w.pop().expect("3"), q.pop().expect("3"));
    let mut scratch = Vec::with_capacity(n);
    let nf = n as f64;
    let variants = vec![
        Variant::new(
            "cols copy",
            "cols",
            "ns/row",
            nf,
            "RowStore lends one column at a time: Vel copied to a scratch Vec, then column_mut::<Pos>()",
            timed(move || b_cols_copy(black_box(&mut t1), &mut scratch)),
        ),
        Variant::new(
            "cols split (unsafe)",
            "cols",
            "ns/row",
            nf,
            "both columns at once through a raw pointer: what a two-column borrow API would give",
            timed(move || b_cols_split(black_box(&mut t2))),
        ),
        Variant::new(
            "bevy iter",
            "bevy",
            "ns/row",
            nf,
            NOTE_ITER,
            timed(move || b_bevy_iter(black_box(&mut w1), &mut q1)),
        ),
        Variant::new(
            "bevy contig",
            "bevy",
            "ns/row",
            nf,
            NOTE_CONTIG,
            timed(move || b_bevy_contig(black_box(&mut w2), &mut q2)),
        ),
        Variant::new(
            "bevy contig bypass",
            "bevy",
            "ns/row",
            nf,
            NOTE_BYPASS,
            timed(move || b_bevy_bypass(black_box(&mut w3), &mut q3)),
        ),
    ];
    run_group(
        cfg,
        "b",
        "stream two fields: pos += vel, all rows",
        n,
        variants,
    )
}

fn pattern_c(cfg: &Config, n: usize, rows: &[Agent], lookups: &[usize]) -> Vec<Record> {
    let (mut t, ids) = build_cols(rows);
    let cols_ids: Vec<RowId> = lookups.iter().map(|&i| ids[i]).collect();
    let (mut w1, ids1) = build_bevy(rows);
    let (mut w2, ids2) = build_bevy(rows);
    let e1: Vec<Entity> = lookups.iter().map(|&i| ids1[i]).collect();
    let e2: Vec<Entity> = lookups.iter().map(|&i| ids2[i]).collect();
    let mut q1: QLookup = w1.query();
    let ops = lookups.len() as f64;
    let variants = vec![
        Variant::new(
            "cols get + get_mut",
            "cols",
            "ns/lookup",
            ops,
            "get::<Pos>(id) then get_mut::<Hunger>(id): two sparse-index lookups per row",
            timed(move || c_cols(black_box(&mut t), &cols_ids)),
        ),
        Variant::new(
            "bevy query.get_mut",
            "bevy",
            "ns/lookup",
            ops,
            "QueryState<(&Pos, &mut Hunger)>::query_mut once per step, then Query::get_mut(entity): one lookup per row",
            timed(move || c_bevy_query(black_box(&mut w1), &mut q1, &e1)),
        ),
        Variant::new(
            "bevy world.get + get_mut",
            "bevy",
            "ns/lookup",
            ops,
            "World::get::<Pos>(e) then World::get_mut::<Hunger>(e): two lookups per row",
            timed(move || c_bevy_world(black_box(&mut w2), &e2)),
        ),
    ];
    run_group(
        cfg,
        "c",
        "random access by stored handle: 10% of rows, read Pos, write Hunger",
        n,
        variants,
    )
}

fn pattern_d(cfg: &Config, n: usize, rows: &[Agent], fresh: &[Agent], stream: bool) -> Vec<Record> {
    let (mut t, ids) = build_cols(rows);
    let (mut w, eids) = build_bevy(rows);
    let mut q: QHunger = w.query();
    let (mut ct, mut cw) = (Churn::new(ids), Churn::new(eids));
    let nf = n as f64;
    let (pattern, title) = if stream {
        (
            "d",
            "churn: remove 1% and insert 1% of rows, then hunger += 1 on all rows",
        )
    } else {
        (
            "d0",
            "churn alone: remove 1% and insert 1% of rows (per row of the table)",
        )
    };
    let variants = vec![
        Variant::new(
            "cols",
            "cols",
            "ns/row",
            nf,
            "RowStore::remove (swap-remove every column) and insert; stream as in a",
            timed(move || {
                ct.step(black_box(&mut t), fresh);
                if stream {
                    a_cols(&mut t);
                }
            }),
        ),
        Variant::new(
            "bevy",
            "bevy",
            "ns/row",
            nf,
            "World::despawn and World::spawn; stream as bevy iter in a",
            timed(move || {
                cw.step(black_box(&mut w), fresh);
                if stream {
                    a_bevy_iter(&mut w, &mut q);
                }
            }),
        ),
    ];
    run_group(cfg, pattern, title, n, variants)
}

fn pattern_d2(cfg: &Config, n: usize, rows: &[Agent], tags: &[bool]) -> Vec<Record> {
    fn split<H: Copy>(ids: &[H], tags: &[bool]) -> (Vec<H>, Vec<H>) {
        let t = (0..ids.len())
            .filter(|&i| tags[i])
            .map(|i| ids[i])
            .collect();
        let u = (0..ids.len())
            .filter(|&i| !tags[i])
            .map(|i| ids[i])
            .collect();
        (t, u)
    }
    let (t, ids) = build_cols_flagged(rows, tags);
    let mut fl = FlagTags(t);
    let (tg, ug) = split(&ids, tags);
    let mut r_fl = Retag::new(tg, ug, n);

    let (t, side, ids) = build_cols_side(rows, tags);
    let side_rows: Vec<RowId> = side.ids().to_vec();
    let mut sd = SideTags(t, side);
    let (tg, ug) = split(&ids, tags);
    let mut r_sd = Retag::new(tg.into_iter().zip(side_rows).collect(), ug, n);

    let (w, ids) = build_bevy_tagged::<Tagged>(rows, tags);
    let mut bt = BevyTags::<Tagged>(w, std::marker::PhantomData);
    let (tg, ug) = split(&ids, tags);
    let mut r_bt = Retag::new(tg, ug, n);

    let (w, ids) = build_bevy_tagged::<TaggedSparse>(rows, tags);
    let mut bs = BevyTags::<TaggedSparse>(w, std::marker::PhantomData);
    let (tg, ug) = split(&ids, tags);
    let mut r_bs = Retag::new(tg, ug, n);

    let ops = (2 * r_fl.per_step) as f64;
    let variants = vec![
        Variant::new(
            "cols flag bit",
            "cols",
            "ns/change",
            ops,
            "get_mut::<Flags>(id), set or clear bit 0",
            timed(move || r_fl.step(black_box(&mut fl))),
        ),
        Variant::new(
            "cols side table",
            "cols",
            "ns/change",
            ops,
            "insert or remove a (TagOf,) row in a second ColumnTable",
            timed(move || r_sd.step(black_box(&mut sd))),
        ),
        Variant::new(
            "bevy table marker",
            "bevy",
            "ns/change",
            ops,
            "entity_mut(e).insert(Tagged) / remove::<Tagged>(): moves the row to another table",
            timed(move || r_bt.step(black_box(&mut bt))),
        ),
        Variant::new(
            "bevy sparse marker",
            "bevy",
            "ns/change",
            ops,
            "insert / remove of a SparseSet marker: archetype changes, table row stays",
            timed(move || r_bs.step(black_box(&mut bs))),
        ),
    ];
    run_group(
        cfg,
        "d2",
        "tag churn: per step 0.5% of rows lose the tag and 0.5% gain it (tagged stay at ~1%)",
        n,
        variants,
    )
}

fn pattern_e(cfg: &Config, n: usize, rows: &[Agent], tags: &[bool]) -> Vec<Record> {
    let (mut t1, _) = build_cols_flagged(rows, tags);
    let (mut t2, side, _) = build_cols_side(rows, tags);
    let (mut t3, _) = build_cols_flagged(rows, tags);
    let (mut w1, _) = build_bevy_tagged::<Tagged>(rows, tags);
    let (mut w2, _) = build_bevy_tagged::<TaggedSparse>(rows, tags);
    let mut q1 = w1.query_filtered::<&mut Hunger, With<Tagged>>();
    let mut q2 = w2.query_filtered::<&mut Hunger, With<TaggedSparse>>();
    let tagged = tags.iter().filter(|&&t| t).count() as f64;
    let mut scratch = Vec::new();
    let variants = vec![
        Variant::new(
            "cols flag scan",
            "cols",
            "ns/tagged",
            tagged,
            "scan the Flags column for bit 0 (all n rows), collect positions, then column_mut::<Hunger>()",
            timed(move || e_cols_flag(black_box(&mut t1), &mut scratch)),
        ),
        Variant::new(
            "cols side table",
            "cols",
            "ns/tagged",
            tagged,
            "walk a ColumnTable<(TagOf,)> of tagged handles, get_mut::<Hunger>(id) in the agent table",
            timed(move || e_cols_side(black_box(&mut t2), &side)),
        ),
        Variant::new(
            "cols flag split (unsafe)",
            "cols",
            "ns/tagged",
            tagged,
            "one branchless pass over Flags and Hunger together (all n rows): needs a two-column borrow",
            timed(move || e_cols_flag_split(black_box(&mut t3))),
        ),
        Variant::new(
            "bevy table marker",
            "bevy",
            "ns/tagged",
            tagged,
            "QueryState<&mut Hunger, With<Tagged>>: tagged rows are their own table, iterated densely",
            timed(move || e_bevy(black_box(&mut w1), &mut q1)),
        ),
        Variant::new(
            "bevy sparse marker",
            "bevy",
            "ns/tagged",
            tagged,
            "QueryState<&mut Hunger, With<TaggedSparse>>: archetype iteration, rows scattered in the shared table",
            timed(move || e_bevy(black_box(&mut w2), &mut q2)),
        ),
    ];
    run_group(
        cfg,
        "e",
        "sparse filter: hunger += 1 on the ~1% of rows with the tag (per tagged row)",
        n,
        variants,
    )
}

fn pattern_f(cfg: &Config, n: usize, notables: &[NotableAos]) -> Vec<Record> {
    let mut t = Notables::new();
    for a in notables {
        t.insert(notable_row(a));
    }
    let build = || {
        let mut w = World::new();
        for a in notables {
            w.spawn(notable_row(a));
        }
        w
    };
    let mut w: Vec<World> = (0..3).map(|_| build()).collect();
    let mut q: Vec<QWealth> = w.iter_mut().map(|w| w.query()).collect();
    let (mut w3, mut q3) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w2, mut q2) = (w.pop().expect("3"), q.pop().expect("3"));
    let (mut w1, mut q1) = (w.pop().expect("3"), q.pop().expect("3"));
    let mut aos = notables.to_vec();
    let nf = n as f64;
    let variants = vec![
        Variant::new(
            "cols",
            "cols",
            "ns/row",
            nf,
            "column_mut::<Wealth>(): streams 8 bytes per row",
            timed(move || f_cols(black_box(&mut t))),
        ),
        Variant::new(
            "bevy iter",
            "bevy",
            "ns/row",
            nf,
            NOTE_ITER,
            timed(move || f_bevy_iter(black_box(&mut w1), &mut q1)),
        ),
        Variant::new(
            "bevy contig",
            "bevy",
            "ns/row",
            nf,
            NOTE_CONTIG,
            timed(move || f_bevy_contig(black_box(&mut w2), &mut q2)),
        ),
        Variant::new(
            "bevy contig bypass",
            "bevy",
            "ns/row",
            nf,
            NOTE_BYPASS,
            timed(move || f_bevy_bypass(black_box(&mut w3), &mut q3)),
        ),
        Variant::new(
            "aos Vec<352 B struct>",
            "aos",
            "ns/row",
            nf,
            "array of structs reference: one 352-byte stride per row",
            timed(move || f_aos(black_box(&mut aos))),
        ),
    ];
    run_group(
        cfg,
        "f",
        "wide rows: 352-byte notables as four components, wealth += 1 (one 8-byte field)",
        n,
        variants,
    )
}

/// Times building a store of all `rows`, dropping it outside the timer.
fn time_builds<S>(mut build: impl FnMut() -> S) -> impl FnMut(u64) -> Duration {
    move |k| {
        let mut total = Duration::ZERO;
        for _ in 0..k {
            let t0 = Instant::now();
            let s = build();
            total += t0.elapsed();
            drop(black_box(s));
        }
        total
    }
}

fn pattern_h(cfg: &Config, n: usize, rows: &[Agent], notables: &[NotableAos]) -> Vec<Record> {
    let nf = n as f64;
    let mut out = Vec::new();
    let agent_variants = vec![
        Variant::new(
            "cols insert",
            "cols",
            "ns/row",
            nf,
            "ColumnTable::new, then RowStore::insert per row",
            time_builds(|| {
                let mut t = Agents::new();
                for r in rows {
                    black_box(t.insert(*r));
                }
                t
            }),
        ),
        Variant::new(
            "bevy spawn",
            "bevy",
            "ns/row",
            nf,
            "World::new, then World::spawn per row",
            time_builds(|| {
                let mut w = World::new();
                for r in rows {
                    black_box(w.spawn(*r).id());
                }
                w
            }),
        ),
        Variant::new(
            "bevy spawn_batch",
            "bevy",
            "ns/row",
            nf,
            "World::new, then World::spawn_batch over all rows (reserves up front)",
            time_builds(|| {
                let mut w = World::new();
                for e in w.spawn_batch(rows.iter().copied()) {
                    black_box(e);
                }
                w
            }),
        ),
    ];
    out.extend(run_group(
        cfg,
        "h",
        "insert cost: build a store of n agent rows (21 B payload)",
        n,
        agent_variants,
    ));
    let notable_variants = vec![
        Variant::new(
            "cols insert",
            "cols",
            "ns/row",
            nf,
            "ColumnTable::new, then RowStore::insert per row",
            time_builds(|| {
                let mut t = Notables::new();
                for a in notables {
                    black_box(t.insert(notable_row(a)));
                }
                t
            }),
        ),
        Variant::new(
            "bevy spawn",
            "bevy",
            "ns/row",
            nf,
            "World::new, then World::spawn per row",
            time_builds(|| {
                let mut w = World::new();
                for a in notables {
                    black_box(w.spawn(notable_row(a)).id());
                }
                w
            }),
        ),
        Variant::new(
            "bevy spawn_batch",
            "bevy",
            "ns/row",
            nf,
            "World::new, then World::spawn_batch over all rows (reserves up front)",
            time_builds(|| {
                let mut w = World::new();
                for e in w.spawn_batch(notables.iter().map(notable_row)) {
                    black_box(e);
                }
                w
            }),
        ),
        Variant::new(
            "aos Vec push",
            "aos",
            "ns/row",
            nf,
            "Vec::new, then push per row: the plain-memory reference",
            time_builds(|| {
                let mut v = Vec::new();
                for a in notables {
                    v.push(*a);
                }
                v
            }),
        ),
    ];
    out.extend(run_group(
        cfg,
        "h352",
        "insert cost: build a store of n notable rows (352 B payload)",
        n,
        notable_variants,
    ));
    out
}

// ---------------------------------------------------------------------------------------------
// g: memory.

#[allow(clippy::too_many_arguments)]
fn mem_record(
    kind: &str,
    variant: &str,
    backend: &str,
    n: usize,
    payload: usize,
    empty: usize,
    total: usize,
    reported: Option<usize>,
    note: &str,
) -> MemRecord {
    MemRecord {
        kind: kind.to_string(),
        variant: variant.to_string(),
        backend: backend.to_string(),
        n,
        payload_bytes_per_row: payload,
        heap_delta_bytes: total,
        bytes_per_row: total as f64 / n as f64,
        empty_store_bytes: empty,
        marginal_bytes_per_row: total.saturating_sub(empty) as f64 / n as f64,
        reported_heap_bytes: reported,
        note: note.to_string(),
    }
}

/// Builds a store with `build_empty` then `fill`, and measures live heap around both.
fn measure_mem<S>(build_empty: impl FnOnce() -> S, fill: impl FnOnce(&mut S)) -> (usize, usize, S) {
    let before = heap_live();
    let mut s = build_empty();
    let empty = heap_live().saturating_sub(before);
    fill(&mut s);
    let total = heap_live().saturating_sub(before);
    (empty, total, s)
}

fn pattern_g(n: usize, rows: &[Agent], notables: &[NotableAos]) -> Vec<MemRecord> {
    let mut out = Vec::new();
    let (empty, total, t) = measure_mem(Agents::new, |t| {
        for r in rows {
            t.insert(*r);
        }
    });
    out.push(mem_record(
        "agent",
        "cols insert",
        "cols",
        n,
        AGENT_PAYLOAD,
        empty,
        total,
        Some(t.heap_bytes()),
        "columns, ids (6 B) and sparse index (8 B); Vec capacity doubles",
    ));
    drop(t);
    let (empty, total, w) = measure_mem(World::new, |w| {
        for r in rows {
            w.spawn(*r);
        }
    });
    out.push(mem_record(
        "agent",
        "bevy spawn",
        "bevy",
        n,
        AGENT_PAYLOAD,
        empty,
        total,
        None,
        "table columns with added and changed ticks, entity metadata, archetype rows",
    ));
    drop(w);
    let (empty, total, w) =
        measure_mem(
            World::new,
            |w| {
                for _ in w.spawn_batch(rows.iter().copied()) {}
            },
        );
    out.push(mem_record(
        "agent",
        "bevy spawn_batch",
        "bevy",
        n,
        AGENT_PAYLOAD,
        empty,
        total,
        None,
        "as bevy spawn, reserved up front",
    ));
    drop(w);
    let (empty, total, t) = measure_mem(Notables::new, |t| {
        for a in notables {
            t.insert(notable_row(a));
        }
    });
    out.push(mem_record(
        "notable",
        "cols insert",
        "cols",
        n,
        NOTABLE_PAYLOAD,
        empty,
        total,
        Some(t.heap_bytes()),
        "four columns, ids and sparse index",
    ));
    drop(t);
    let (empty, total, w) = measure_mem(World::new, |w| {
        for a in notables {
            w.spawn(notable_row(a));
        }
    });
    out.push(mem_record(
        "notable",
        "bevy spawn",
        "bevy",
        n,
        NOTABLE_PAYLOAD,
        empty,
        total,
        None,
        "four table columns with ticks, entity metadata, archetype rows",
    ));
    drop(w);
    let (empty, total, w) = measure_mem(World::new, |w| {
        for _ in w.spawn_batch(notables.iter().map(notable_row)) {}
    });
    out.push(mem_record(
        "notable",
        "bevy spawn_batch",
        "bevy",
        n,
        NOTABLE_PAYLOAD,
        empty,
        total,
        None,
        "as bevy spawn, reserved up front",
    ));
    drop(w);
    for m in &out {
        println!(
            "  g n={n:<8} {:<8} {:<20} {:>8.1} B/row (payload {} B, empty store {} B)",
            m.kind, m.variant, m.bytes_per_row, m.payload_bytes_per_row, m.empty_store_bytes
        );
    }
    out
}

// ---------------------------------------------------------------------------------------------

fn main() {
    let cfg = Config::from_env();
    let started = Instant::now();
    let (date_utc, stamp) = p4_storage_bench::now_utc();
    let loadavg_before = p4_storage_bench::loadavg();
    println!(
        "P4 storage bench: profile {}, sizes {:?}, {} runs of at least {} ms, load {}",
        cfg.profile,
        cfg.sizes,
        cfg.runs,
        cfg.target.as_millis(),
        loadavg_before.as_deref().unwrap_or("?")
    );
    if cfg!(debug_assertions) {
        println!("WARNING: debug assertions are on; this is not an optimised build");
    }
    if !p4_storage_bench::loops_aligned() {
        println!(
            "WARNING: built without -align-loops=64 (rustflags \"{}\"); tight-loop timings depend on code layout. Run from bench/p4-storage.",
            p4_storage_bench::RUSTFLAGS
        );
    }

    verify();

    let mut results: Vec<Record> = Vec::new();
    let mut memory: Vec<MemRecord> = Vec::new();
    for &n in &cfg.sizes {
        println!("n = {n}");
        let rows = gen_agents(n, SEED_AGENTS);
        if cfg.wants("a") {
            results.extend(pattern_a(&cfg, n, &rows));
        }
        if cfg.wants("b") {
            results.extend(pattern_b(&cfg, n, &rows));
        }
        if cfg.wants("c") {
            results.extend(pattern_c(&cfg, n, &rows, &gen_lookups(n)));
        }
        let fresh = gen_agents((n / 10).max(1), SEED_FRESH);
        if cfg.wants("d") {
            results.extend(pattern_d(&cfg, n, &rows, &fresh, true));
            results.extend(pattern_d(&cfg, n, &rows, &fresh, false));
        }
        let tags = gen_tags(n);
        if cfg.wants("d2") {
            results.extend(pattern_d2(&cfg, n, &rows, &tags));
        }
        if cfg.wants("e") {
            results.extend(pattern_e(&cfg, n, &rows, &tags));
        }
        let needs_notables = cfg.wants("f") || cfg.wants("g") || cfg.wants("h");
        let notables = if needs_notables {
            gen_notables(n)
        } else {
            Vec::new()
        };
        if cfg.wants("f") {
            results.extend(pattern_f(&cfg, n, &notables));
        }
        if cfg.wants("g") {
            memory.extend(pattern_g(n, &rows, &notables));
        }
        if cfg.wants("h") {
            results.extend(pattern_h(&cfg, n, &rows, &notables));
        }
    }

    let tagged: Vec<String> = cfg
        .sizes
        .iter()
        .map(|&n| format!("{} of {n}", gen_tags(n).iter().filter(|&&t| t).count()))
        .collect();
    let report = Report {
        schema: 2,
        bench: "p4-storage".to_string(),
        date_utc,
        profile: cfg.profile.clone(),
        git_commit: p4_storage_bench::git_commit(),
        git_dirty: p4_storage_bench::git_dirty(),
        rustc: p4_storage_bench::RUSTC_VERSION.to_string(),
        bevy_ecs: p4_storage_bench::BEVY_ECS_VERSION.to_string(),
        bevy_ecs_features: "default-features = false, features = [\"std\"]".to_string(),
        build: "cargo bench profile (inherits release: opt-level 3, lto thin, panic unwind, debug line-tables-only)".to_string(),
        rustflags: p4_storage_bench::RUSTFLAGS.to_string(),
        debug_assertions: cfg!(debug_assertions),
        machine: Machine::probe(),
        cpus_allowed: p4_storage_bench::cpus_allowed(),
        nice: p4_storage_bench::nice_level(),
        loadavg_before,
        loadavg_after: p4_storage_bench::loadavg(),
        target_ms_per_run: cfg.target.as_millis(),
        runs: cfg.runs,
        sizes: cfg.sizes.clone(),
        elapsed_s: started.elapsed().as_secs_f64(),
        results,
        memory,
        notes: vec![
            "Single thread. No bevy Schedule, System or executor: QueryState cached and driven directly.".to_string(),
            "Every bevy step calls World::increment_change_tick first, as a system run would; change ticks are written as bevy always does, except in the 'bypass' lines.".to_string(),
            "Variants of one pattern and size are measured interleaved, run by run, after a calibration that doubles as warm-up.".to_string(),
            "Every variant of every pattern was checked to end in the same state on a 20,000-row world before timing.".to_string(),
            format!("Tagged rows (patterns d2, e): {}.", tagged.join(", ")),
            "A counting global allocator is installed for pattern g; every variant pays its atomic add per allocation.".to_string(),
            "Slice loops (a, b, f and the e flag split) run in shared out-of-line kernels, so cols, bevy contig and bypass execute identical code.".to_string(),
            "Built with loop headers aligned to 64 bytes (-C llvm-args=-align-loops=64, bench/p4-storage/.cargo/config.toml). Without it, identical loops ran up to 50% apart between builds that differed only in unrelated code.".to_string(),
        ],
    };
    let text = p4_storage_bench::table(&report);
    println!();
    print!("{text}");
    if cfg.write {
        let dir = p4_storage_bench::results_dir();
        std::fs::create_dir_all(&dir).expect("results dir");
        let base = dir.join(format!("p4-storage-{stamp}-{}", cfg.profile));
        let json = serde_json::to_string_pretty(&report).expect("json");
        std::fs::write(base.with_extension("json"), json + "\n").expect("write json");
        std::fs::write(base.with_extension("txt"), &text).expect("write txt");
        println!("wrote {}.json and .txt", base.display());
    }
}
