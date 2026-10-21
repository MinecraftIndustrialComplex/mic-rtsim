// SPDX-License-Identifier: AGPL-3.0-or-later
//! The ships on the page store (A16; docs/choices-paging-modules.md, the PM-S rows): ship rows
//! (0x5310) and lane threats (0x5311) paged to a real `PageStore` under a tiny budget, saved every
//! simulated half day so pages really evict, run beside the same scenario fully resident.
//!
//! Two scenarios: the `sim ships` coast with its scripted player (who watches, follows the
//! hunted lane, sends an escort, logs off before a raid, loots a hold and sinks a pirate, checking
//! conservation and the indexes every hour), and the ship sweep's ocean grid with a player
//! touring its docks.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_settle::Pos;
use rtsim_ships::NETWORK_KEY;
use rtsim_ships::synth::{self, Coastal, FakeShips, Script};
use rtsim_ships::water::AllWater;
use rtsim_ships::{ShipTunables, Ships};
use rtsim_travel::Medium;
use rtsim_world::World;

/// Where the paged runs keep the ships' meta page (rtsim-jni's ships table, outside the row
/// tables).
const META: PageKey = PageKey::new(0x5300, 1);
const HALF_DAY: u64 = TICKS_PER_DAY / 2;
/// Bytes for both row tables: a few pages of four ships.
const BUDGET: usize = 3_000;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rtsim-ships-paged-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A page store in a temp folder, removed when dropped.
struct Store {
    store: Arc<RwLock<PageStore>>,
    dir: PathBuf,
    commits: u64,
}

impl Store {
    fn new(tag: &str) -> Store {
        let dir = tmpdir(tag);
        Store {
            store: Arc::new(RwLock::new(
                PageStore::open(&dir, Durability::NoSync).expect("a store"),
            )),
            dir,
            commits: 0,
        }
    }

    /// One paged save as the game makes it: save_paged, commit, saved, then the budget.
    fn save(&mut self, s: &mut Ships) {
        let mut b = SaveBatch::new();
        let t = s.save_paged(&mut b, META);
        self.commits += 1;
        self.store
            .write()
            .unwrap()
            .commit(b, self.commits)
            .expect("commit");
        s.saved(t);
        s.enforce_budget();
        assert!(
            s.rows_resident_bytes() <= BUDGET,
            "{} resident bytes after a save, over the budget {BUDGET}",
            s.rows_resident_bytes()
        );
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn reload_world(w: &World, seed: u64) -> World {
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    World::load(synth::world_config(seed), goods, book, &w.save()).expect("the world loads")
}

/// A scenario the tests run in half days.
trait Scenario: Sized {
    fn ships(&mut self) -> &mut Ships;
    fn world(&self) -> &World;
    fn run(&mut self, from: Tick, to: Tick);
    /// The same scenario with the world read back from its save and `ships` in place of the
    /// engine (its plugin and player copied).
    fn fork(&self, ships: Ships) -> Self;
    fn hashes(&mut self) -> (u64, u64) {
        (self.ships().state_hash(), self.world().state_hash())
    }
}

const COAST_SEED: u64 = 7;
const COAST_DAYS: u64 = 12;

struct Coast {
    c: Coastal,
    fake: FakeShips,
    script: Script,
}

impl Coast {
    fn new() -> Coast {
        let c = synth::coastal(COAST_SEED, ShipTunables::default());
        let fake = FakeShips::new(f64::from(c.ships.tun.view_radius));
        Coast {
            c,
            fake,
            script: Script::default(),
        }
    }
}

impl Scenario for Coast {
    fn ships(&mut self) -> &mut Ships {
        &mut self.c.ships
    }

    fn world(&self) -> &World {
        &self.c.world
    }

    fn run(&mut self, from: Tick, to: Tick) {
        self.script.run(&mut self.c, &mut self.fake, from, to);
        assert!(
            self.script.conserved.is_empty(),
            "{:?}",
            self.script.conserved
        );
    }

    fn fork(&self, ships: Ships) -> Coast {
        Coast {
            c: Coastal {
                world: reload_world(&self.c.world, COAST_SEED),
                ships,
                mask: synth::coast(),
                haven: self.c.haven,
                navy: self.c.navy,
            },
            fake: self.fake.clone(),
            script: self.script.clone(),
        }
    }
}

const GRID_SEED: u64 = 0x5CA1_E000;
const GRID_DAYS: u64 = 4;
const GRID_PLAYER: (u64, u64) = (0x6121, 0xD);

/// The ship sweep's ocean grid (5 × 5 ports, four ships a service) with a player who stands at a
/// different dock every hour, so voyages all over the grid are booked, seen and embodied.
struct Grid {
    world: World,
    ships: Ships,
    mask: AllWater,
    fake: FakeShips,
}

impl Grid {
    fn new() -> Grid {
        Grid::with_side(5)
    }

    fn with_side(side: usize) -> Grid {
        let tun = ShipTunables {
            max_ships_per_service: 4,
            headway_ticks: 1,
            ..ShipTunables::default()
        };
        let (world, ships, mask) = synth::ocean_grid(side, GRID_SEED, tun);
        let fake = FakeShips::new(f64::from(ships.tun.view_radius));
        Grid {
            world,
            ships,
            mask,
            fake,
        }
    }
}

impl Scenario for Grid {
    fn ships(&mut self) -> &mut Ships {
        &mut self.ships
    }

    fn world(&self) -> &World {
        &self.world
    }

    fn run(&mut self, from: Tick, to: Tick) {
        let mut t = from;
        while t < to {
            let hour = (t.0 / 1_000) as usize;
            let d = &self.ships.docks[hour * 7 % self.ships.docks.len()];
            let (x, z) = (d.x, d.z);
            self.fake
                .set_player(GRID_PLAYER, f64::from(x), f64::from(z));
            self.ships
                .observe_player(&mut self.world, GRID_PLAYER, Pos::new(0, x, z), t);
            let next = (t + 1_000).min(to);
            synth::drive(
                &mut self.world,
                &mut self.ships,
                &mut self.fake,
                &mut NoInfrastructure,
                &self.mask,
                next,
                50,
            );
            t = next;
        }
        self.ships.check(&self.world).expect("conserved");
    }

    fn fork(&self, ships: Ships) -> Grid {
        Grid {
            world: reload_world(&self.world, GRID_SEED),
            ships,
            mask: AllWater(32),
            fake: self.fake.clone(),
        }
    }
}

/// Small pages, so the scenario's ships span many of them, and the store attached with the first
/// save written.
fn page<S: Scenario>(s: &mut S, store: &mut Store) {
    let sh = s.ships();
    sh.set_page_layout(4, 4);
    sh.attach_store(store.store.clone(), BUDGET);
    store.save(sh);
}

/// The same scenario resident and paged, half day by half day, a paged save after each: equal
/// hashes every two days and at the end, and the paged one really loaded and evicted pages.
fn paged_equals_unpaged<S: Scenario>(make: impl Fn() -> S, days: u64, tag: &str) {
    let mut a = make();
    let mut b = make();
    let mut store = Store::new(tag);
    page(&mut b, &mut store);
    assert_eq!(a.hashes(), b.hashes(), "built the same");
    let ships = a.ships().ships.len();
    for k in 0..days * 2 {
        let (from, to) = (Tick(k * HALF_DAY), Tick((k + 1) * HALF_DAY));
        a.run(from, to);
        b.run(from, to);
        store.save(b.ships());
        if k % 4 == 3 {
            assert_eq!(
                a.hashes(),
                b.hashes(),
                "{tag}: day {}",
                (k + 1) * HALF_DAY / TICKS_PER_DAY
            );
        }
    }
    assert_eq!(a.hashes(), b.hashes(), "{tag}: at the end");
    assert_eq!(a.ships().stats, b.ships().stats);
    let st = b.ships().row_stats();
    eprintln!("{tag}: {ships} ships, {} saves, {st:?}", store.commits);
    assert!(st.evictions > 0, "{tag}: pages evicted: {st:?}");
    assert!(st.loads > 0, "{tag}: pages loaded back: {st:?}");
    assert!(b.ships().is_paged() && !a.ships().is_paged());
}

/// A paged run saved at its midpoint, a second engine opened from the store with nothing
/// resident, both run to the end beside one that never paged: three equal hashes.
fn save_and_restore<S: Scenario>(make: impl Fn() -> S, days: u64, tag: &str) {
    let mut u = make();
    let mut p = make();
    let mut store = Store::new(tag);
    page(&mut p, &mut store);
    let mid = days; // in half days
    for k in 0..mid {
        let (from, to) = (Tick(k * HALF_DAY), Tick((k + 1) * HALF_DAY));
        u.run(from, to);
        p.run(from, to);
        store.save(p.ships());
    }
    let tun = p.ships().tun.clone();
    let opened = Ships::open_paged(store.store.clone(), BUDGET, META, tun).expect("opens");
    assert_eq!(opened.rows_resident_bytes(), 0, "nothing resident on open");
    assert_eq!(opened.ships.len(), p.ships().ships.len());
    let mut q = p.fork(opened);
    assert_eq!(q.hashes(), p.hashes(), "{tag}: read back exactly");
    // From here neither saves: both read the pages the midpoint's save left in the one store.
    for k in mid..days * 2 {
        let (from, to) = (Tick(k * HALF_DAY), Tick((k + 1) * HALF_DAY));
        u.run(from, to);
        p.run(from, to);
        q.run(from, to);
    }
    let h = u.hashes();
    assert_eq!(p.hashes(), h, "{tag}: the paged run that went on");
    assert_eq!(q.hashes(), h, "{tag}: the engine opened at the midpoint");
    assert_eq!(q.ships().stats, u.ships().stats);
    let st = q.ships().row_stats();
    assert!(st.loads > 0 && st.evictions > 0, "{tag}: {st:?}");
}

#[test]
fn paged_equals_unpaged_on_the_coast() {
    paged_equals_unpaged(Coast::new, COAST_DAYS, "coast");
}

#[test]
fn paged_equals_unpaged_on_the_ocean_grid() {
    paged_equals_unpaged(Grid::new, GRID_DAYS, "grid");
}

#[test]
fn a_paged_save_reopens_and_continues_exactly_on_the_coast() {
    save_and_restore(Coast::new, COAST_DAYS, "coast-reopen");
}

#[test]
fn a_paged_save_reopens_and_continues_exactly_on_the_ocean_grid() {
    save_and_restore(Grid::new, GRID_DAYS, "grid-reopen");
}

/// The blob form reads back as before, and a paged engine writes the same blob as a resident
/// one (rows in key order whatever was resident when).
#[test]
fn a_paged_engine_saves_the_same_blob() {
    let mut a = Coast::new();
    let mut b = Coast::new();
    let mut store = Store::new("blob");
    page(&mut b, &mut store);
    a.run(Tick::ZERO, Tick::from_days(3));
    b.run(Tick::ZERO, Tick::from_days(3));
    store.save(b.ships());
    let (ba, bb) = (a.ships().save(), b.ships().save());
    assert_eq!(ba, bb);
    let back = Ships::load(ShipTunables::default(), &ba).expect("loads");
    assert_eq!(back.save(), ba);
    assert!(!back.is_paged());
    assert!(Ships::load(ShipTunables::default(), &ba[..ba.len() - 1]).is_err());
}

/// No pass over every ship on the paths events and players take: a player watching one dock for
/// an hour loads the pages of the ships near it (the lane cells and the services' timetables
/// name them), far fewer than the table; a session start loads only the embodied ships' pages.
#[test]
fn a_watched_hour_loads_only_the_pages_it_needs() {
    let mut g = Grid::with_side(10);
    let mut store = Store::new("bounded");
    page(&mut g, &mut store);
    let pages = g.ships.ships.len().div_ceil(4) as u64;
    g.ships.set_budget(0);
    g.ships.enforce_budget();
    assert_eq!(g.ships.rows_resident_bytes(), 0);
    // A budget too big to evict anything: the loads count distinct pages.
    g.ships.set_budget(1 << 30);
    g.ships.reset_row_stats();
    let d = g.ships.docks[0].clone();
    g.fake
        .set_player(GRID_PLAYER, f64::from(d.x), f64::from(d.z));
    g.ships
        .observe_player(&mut g.world, GRID_PLAYER, Pos::new(0, d.x, d.z), Tick::ZERO);
    synth::drive(
        &mut g.world,
        &mut g.ships,
        &mut g.fake,
        &mut NoInfrastructure,
        &g.mask,
        Tick(1_000),
        50,
    );
    let st = g.ships.row_stats();
    eprintln!("an hour at one dock: {st:?}, of {pages} pages");
    assert!(g.ships.stats.materialized > 0, "{:?}", g.ships.stats);
    assert!(
        st.loads * 4 < pages,
        "{} pages loaded for an hour at one dock, of {pages}",
        st.loads
    );
    // A session start makes the embodied ships abstract from the index, not by a pass.
    let embodied = g.ships.ships.iter().filter(|(_, x)| x.embodied).count() as u64;
    assert!(embodied > 0);
    g.ships.set_budget(0);
    store.save(&mut g.ships);
    assert_eq!(g.ships.rows_resident_bytes(), 0);
    g.ships.reset_row_stats();
    g.ships.session_start();
    let st = g.ships.row_stats();
    assert!(st.loads <= embodied, "{st:?} for {embodied} embodied ships");
    assert!(g.ships.ships.iter().all(|(_, x)| !x.embodied));
    g.ships.check(&g.world).expect("the indexes agree");
    // The session start changed the embodied ships' pages: over a budget of nothing with only
    // changed pages left, the engine asks for a save, and after it has nothing to ask.
    g.ships.enforce_budget();
    assert!(
        g.ships.needs_save(),
        "{} bytes",
        g.ships.rows_resident_bytes()
    );
    store.save(&mut g.ships);
    assert!(!g.ships.needs_save());
    assert!(
        !Grid::with_side(3).ships.needs_save(),
        "resident never asks"
    );
}

/// The network (the flow graph, resident and growing with the world) is written only by the saves
/// after it changed: a port that joins later is in the next save's network page, and an engine
/// opened from it has the port and its shuttle.
#[test]
fn the_network_page_is_written_only_when_it_changes() {
    let mut c = Coast::new();
    let mut store = Store::new("network");
    page(&mut c, &mut store);
    let save = |c: &mut Coast, store: &mut Store| -> bool {
        let mut b = SaveBatch::new();
        let t = c.ships().save_paged(&mut b, META);
        let wrote = t.wrote_network();
        store.commits += 1;
        store
            .store
            .write()
            .unwrap()
            .commit(b, store.commits)
            .unwrap();
        c.ships().saved(t);
        wrote
    };
    c.run(Tick::ZERO, Tick(HALF_DAY));
    assert!(!save(&mut c, &mut store), "nothing joined: no network page");
    // Brightwater (node 1) has no sky dock: one joins, linked by a shuttle.
    let (docks, lanes) = (c.c.ships.docks.len(), c.c.ships.lanes.len());
    let d = c.c.ships.add_port(
        &mut c.c.world,
        1,
        Medium::Sky,
        (1_330, 120, synth::shore(1_300) - 60),
        4,
        &c.c.mask,
        Tick(HALF_DAY),
    );
    assert!(d.is_some());
    assert_eq!(c.c.ships.lanes.len(), lanes + 1, "a shuttle");
    c.run(Tick(HALF_DAY), Tick(2 * HALF_DAY));
    assert!(save(&mut c, &mut store), "a port joined");
    assert!(!save(&mut c, &mut store), "and once written, not again");
    assert!(
        store
            .store
            .read()
            .unwrap()
            .get_shared(NETWORK_KEY)
            .unwrap()
            .is_some()
    );
    let tun = c.c.ships.tun.clone();
    let opened = Ships::open_paged(store.store.clone(), BUDGET, META, tun).expect("opens");
    assert_eq!(opened.docks.len(), docks + 1);
    let mut q = c.fork(opened);
    assert_eq!(q.hashes(), c.hashes());
    q.run(Tick(2 * HALF_DAY), Tick(4 * HALF_DAY));
    c.run(Tick(2 * HALF_DAY), Tick(4 * HALF_DAY));
    assert_eq!(q.hashes(), c.hashes());
}
