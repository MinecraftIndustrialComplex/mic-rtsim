// SPDX-License-Identifier: AGPL-3.0-or-later
//! The conflict engine on the page store (A16; docs/choices-paging-modules.md, section Conflict):
//! a run whose row tables page to a real store under a tiny budget, saved every half day so pages
//! really go, is the run that never paged, state hash for state hash; and an engine reopened from
//! a paged save in the middle goes on exactly as the one that kept running.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rtsim_conflict::Conflict;
use rtsim_conflict::scenario::{self, META_KEY, Options, Paging, Variant};
use rtsim_conflict::synth;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageStore, SaveBatch};
use rtsim_kernel::rows::RowStats;
use rtsim_world::World;

/// A budget smaller than one faction page: every clean page goes at every step boundary.
const TINY: usize = 8 << 10;
const HALF_DAY: u64 = TICKS_PER_DAY / 2;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rtsim-conflict-paged-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

type Store = Arc<RwLock<PageStore>>;

fn open_store(dir: &Path) -> Store {
    Arc::new(RwLock::new(
        PageStore::open(dir, Durability::NoSync).unwrap(),
    ))
}

fn save(c: &mut Conflict, store: &Store) {
    let mut b = SaveBatch::new();
    let t = c.save_paged(&mut b, META_KEY);
    store.write().unwrap().commit(b, 0).unwrap();
    c.saved(t);
    c.enforce_budget();
}

/// The bench's run (`rtsim-cli bench scale`'s faction sweep): many factions, one village per
/// camp, the world stepped to each conflict event, a save point every half day. Records both
/// state hashes every `check_every` save points.
struct Run {
    w: World,
    c: Conflict,
    store: Option<Store>,
    t: Tick,
    next_save: Tick,
    saves: u64,
    hashes: Vec<(u64, u64)>,
}

impl Run {
    fn new(factions: usize, seed: u64, store: Option<Store>) -> Run {
        let (w, mut c) = synth::many(factions.max(16), factions, seed);
        if let Some(s) = &store {
            c.attach_store(s.clone(), TINY);
        }
        Run {
            w,
            c,
            store,
            t: Tick::ZERO,
            next_save: Tick(HALF_DAY),
            saves: 0,
            hashes: Vec::new(),
        }
    }

    fn run_to(&mut self, end: Tick, check_every: u64) {
        while self.t < end {
            let next = self
                .c
                .next_due()
                .unwrap_or(end)
                .clamp(self.t + 1, end)
                .min(self.next_save);
            self.w.run_until(next);
            self.c.run_until(&mut self.w, &mut NoInfrastructure, next);
            let _ = self.c.take_commands();
            if next == self.next_save {
                if let Some(s) = &self.store {
                    save(&mut self.c, s);
                }
                self.saves += 1;
                if self.saves.is_multiple_of(check_every) {
                    self.hashes.push((self.c.state_hash(), self.w.state_hash()));
                    self.c.enforce_budget();
                }
                self.next_save = next + HALF_DAY;
            }
            self.t = next;
        }
    }

    fn end_hashes(&mut self) -> (u64, u64) {
        let h = (self.c.state_hash(), self.w.state_hash());
        self.c.enforce_budget();
        h
    }
}

#[test]
fn many_factions_paged_equal_the_resident_run() {
    for (seed, factions) in [(3u64, 48usize), (11, 100)] {
        let end = Tick::from_days(14);
        let mut a = Run::new(factions, seed, None);
        a.run_to(end, 6);
        let dir = tmpdir(&format!("many-{seed}"));
        let mut b = Run::new(factions, seed, Some(open_store(&dir)));
        b.run_to(end, 6);
        assert!(a.c.stats.events > 500, "{} events", a.c.stats.events);
        assert_eq!(a.hashes.len(), 4);
        assert_eq!(a.hashes, b.hashes, "seed {seed}: the hashes every three days");
        assert_eq!(a.end_hashes(), b.end_hashes(), "seed {seed}: at the end");
        assert_eq!(a.c.stats, b.c.stats);
        let rs = b.c.row_stats();
        assert!(rs.evictions > 0 && rs.loads > 0, "pages really went: {rs:?}");
        assert_eq!(a.c.row_stats().evictions, 0, "the resident run pages nothing");
        // At a step boundary after a save the budget holds: nothing is dirty.
        save(&mut b.c, b.store.as_ref().unwrap());
        assert!(
            b.c.resident_bytes() <= TINY,
            "{} resident over {TINY}",
            b.c.resident_bytes()
        );
        b.c.check(&b.w).expect("books");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn a_paged_save_reopened_in_the_middle_goes_on_as_the_run_that_kept_going() {
    let (seed, factions) = (5u64, 64usize);
    let (mid, end) = (Tick::from_days(6) + 7_000, Tick::from_days(13));
    let mut whole = Run::new(factions, seed, None);
    whole.run_to(end, 1_000);
    let dir_a = tmpdir("restore-a");
    let mut a = Run::new(factions, seed, Some(open_store(&dir_a)));
    a.run_to(mid, 1_000);
    // A save at the midpoint, committed; a second engine reads it from a copy of the store (so
    // each goes on saving to its own), with nothing resident, on a copy of the world.
    save(&mut a.c, a.store.as_ref().unwrap());
    let dir_b = tmpdir("restore-b");
    copy_dir(&dir_a, &dir_b);
    let store_b = open_store(&dir_b);
    let c = Conflict::open_paged(
        store_b.clone(),
        TINY,
        META_KEY,
        a.c.tun.clone(),
        a.c.cultures.clone(),
    )
    .unwrap()
    .expect("a meta page");
    assert_eq!(c.resident_bytes(), 0, "opened with nothing resident");
    assert_eq!(c.factions.len(), factions);
    let (goods, ids) = rtsim_world::synth::goods();
    let book = rtsim_world::synth::recipes(&ids);
    let w = World::load(synth::world_config(seed), goods, book, &a.w.save()).unwrap();
    let mut b = Run {
        w,
        c,
        store: Some(store_b),
        t: a.t,
        next_save: a.next_save,
        saves: a.saves,
        hashes: Vec::new(),
    };
    assert_eq!(b.c.state_hash(), a.c.state_hash(), "the same state at once");
    b.c.enforce_budget();
    a.run_to(end, 1_000);
    b.run_to(end, 1_000);
    let h = whole.end_hashes();
    assert_eq!(a.end_hashes(), h, "the run that kept going");
    assert_eq!(b.end_hashes(), h, "the reopened run");
    assert!(b.c.row_stats().loads > 0);
    std::fs::remove_dir_all(&dir_a).unwrap();
    std::fs::remove_dir_all(&dir_b).unwrap();
}

fn walkthrough(paged: Option<&Path>, restart_day: Option<f64>) -> scenario::Outcome {
    scenario::run(&Options {
        restart_day,
        save_every: Some(HALF_DAY),
        paged: paged.map(|d| Paging {
            dir: d.to_path_buf(),
            budget: 2 << 10,
        }),
        ..Options::new(7, 24, Variant::Ignore)
    })
}

fn evictions(r: &RowStats) -> u64 {
    r.evictions
}

#[test]
fn the_walkthrough_paged_equals_the_resident_one_and_restores_from_pages() {
    let a = walkthrough(None, None);
    let dir = tmpdir("walk");
    let b = walkthrough(Some(&dir), None);
    assert!(a.trail.len() >= 47, "{} save points", a.trail.len());
    assert_eq!(a.trail, b.trail, "every half day");
    assert_eq!(a.hashes, b.hashes);
    assert_eq!(a.plugin.commands, b.plugin.commands);
    assert!(evictions(&b.rows) > 0, "pages went: {:?}", b.rows);
    // Reopened from the store on day 9.3 (paged save, open_paged): the run that never stopped.
    let c = walkthrough(Some(&dir), Some(9.3));
    assert_eq!(c.hashes, a.hashes, "restored from pages");
    assert_eq!(c.trail, a.trail);
    assert_eq!(c.plugin.commands, a.plugin.commands);
    std::fs::remove_dir_all(&dir).unwrap();
}
