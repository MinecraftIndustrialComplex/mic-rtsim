// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dragons on the page store (A16; docs/choices-paging-modules.md, Dragons): paging changes
//! nothing but memory.
//!
//! - **Paged equals unpaged**: the bench's world (`synth::grid`, `synth::bench_lairs`,
//!   `synth::populate`) with a player walking through it and the fake plugin answering (bodies
//!   materialize, are wounded, later killed, fly off and are captured), run twice: resident, and
//!   paged to a real page store under a tiny budget with a save every half day, so row pages
//!   really evict and load back. The same commands every step, the same state hashes along the
//!   way and the same save at the end.
//! - **Save and restore**: at a midpoint the paged run saves; a second engine opens from (a copy
//!   of) the store with nothing resident and both run on: the same state as each other and as a
//!   resident run that never stopped.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rtsim_ecology::boundary::Cmd;
use rtsim_ecology::engine::Dragons;
use rtsim_ecology::infra::NoInfrastructure;
use rtsim_ecology::synth::{self, FakeIaf, Reaction};
use rtsim_ecology::tunables::DragonTunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::fnv1a64;
use rtsim_settle::Pos;
use rtsim_world::{World, WorldConfig};

const PLAYER: (u64, u64) = (5, 6);
/// Where rtsim-jni keeps the dragons' page today; a paged host would put the meta there.
const META: PageKey = PageKey::new(0x4400, 0);
const VILLAGES: usize = 64;
/// Lairs of two dragons each (a parent and its young): pairs, dispersal and respawns happen.
const LAIRS: usize = 48;
/// Ticks between steps: the player reports, the engine runs, the plugin answers.
const STEP: u64 = 500;
const SAVE_EVERY: u64 = TICKS_PER_DAY / 2;
/// A few dozen dragons' rows: almost every page is out at any moment.
const BUDGET: usize = 24 << 10;
const DAYS: u64 = 8;

struct Run {
    w: World,
    d: Dragons,
    iaf: FakeIaf,
    store: Option<Arc<RwLock<PageStore>>>,
}

fn start(seed: u64) -> Run {
    let mut w = synth::grid(VILLAGES, seed);
    let mut d = Dragons::new(seed, DragonTunables::default(), Tick::ZERO);
    let lairs = synth::bench_lairs(&w, LAIRS, 2, seed);
    synth::populate(&mut w, &mut d, &lairs, Tick::ZERO);
    let mut iaf = FakeIaf::new(PLAYER);
    iaf.reaction = Reaction::Wound {
        share: 0.25,
        after: 150,
    };
    Run {
        w,
        d,
        iaf,
        store: None,
    }
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rtsim-dragons-paged-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn page(run: &mut Run, dir: &Path) {
    let store = Arc::new(RwLock::new(
        PageStore::open(dir, Durability::NoSync).expect("the store opens"),
    ));
    run.d.attach_store(store.clone(), BUDGET);
    run.store = Some(store);
}

/// A zigzag over the grid (about 4,000 blocks across), 40 blocks a step east-west and 17
/// north-south, turning at the edges.
fn walk(t: Tick) -> Pos {
    let k = (t.0 / STEP) as i64;
    let span = 3_800;
    let tri = |v: i64| {
        let m = v.rem_euclid(2 * span);
        if m < span { m } else { 2 * span - m }
    };
    Pos::new(0, tri(k * 40) as i32, tri(k * 17 + 600) as i32)
}

/// One step to `t`: the player's report, the engine, the plugin's answers (each at the engine's
/// time at least, as rtsim-jni clamps them), and the step boundary. Returns the commands.
fn step(r: &mut Run, t: Tick) -> Vec<Cmd> {
    r.d.observe_player(&mut r.w, PLAYER, walk(t), t);
    r.d.run_until(&mut r.w, &mut NoInfrastructure, t);
    let mut cmds = r.d.take_commands();
    r.iaf.apply(cmds.clone(), t);
    for (at, o) in r.iaf.due(t) {
        let at = at.max(r.d.now());
        r.d.ingest(&mut r.w, &mut NoInfrastructure, &o, at);
    }
    let more = r.d.take_commands();
    r.iaf.apply(more.clone(), t);
    cmds.extend(more);
    r.d.enforce_budget();
    cmds
}

/// A paged save as a host makes one: the batch, the commit, the tickets back, the budget.
fn save(r: &mut Run) {
    let store = r.store.as_ref().expect("paged");
    let mut b = SaveBatch::new();
    let tickets = r.d.save_paged(&mut b, META);
    store
        .write()
        .unwrap()
        .commit(b, 0)
        .expect("the save commits");
    r.d.saved(tickets);
    r.d.enforce_budget();
}

/// The midpoint of a run switches the player from wounding to killing what it meets.
fn react(r: &mut Run, t: Tick) {
    if t.0 == (DAYS / 2) * TICKS_PER_DAY {
        r.iaf.reaction = Reaction::Kill { after: 300 };
    }
}

fn save_hash(d: &Dragons) -> u64 {
    fnv1a64(&d.save())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}

/// The scenario did what it is for: bodies came and went, were hurt and killed, and the paged
/// engine evicted and loaded pages.
fn assert_exercised(a: &Run, paged: &Run) {
    let s = a.d.stats;
    assert!(
        s.materialized > 0 && s.hurt > 0 && s.deaths_violent > 0 && s.hunts > 0,
        "{s:?}"
    );
    let p = paged.d.paging_stats();
    assert!(p.evictions() > 0 && p.loads() > 0, "{p:?}");
    a.d.check(&a.w, None).expect("the resident run balances");
    paged.d.check(&paged.w, None).expect("the paged run balances");
}

#[test]
fn paged_equals_unpaged() {
    let seed = 21;
    let dir = tmpdir("equal");
    let mut a = start(seed);
    let mut b = start(seed);
    page(&mut b, &dir);
    let mut t = Tick::ZERO;
    let mut compared = 0;
    while t < Tick::from_days(DAYS) {
        t += STEP;
        react(&mut a, t);
        react(&mut b, t);
        let (ca, cb) = (step(&mut a, t), step(&mut b, t));
        assert_eq!(ca, cb, "commands at {t:?}");
        if t.0.is_multiple_of(SAVE_EVERY) {
            save(&mut b);
            assert!(
                b.d.resident_row_bytes() <= BUDGET,
                "{} resident row bytes after a save, budget {BUDGET}",
                b.d.resident_row_bytes()
            );
        }
        if t.0.is_multiple_of(2 * TICKS_PER_DAY) {
            assert_eq!(a.d.state_hash(), b.d.state_hash(), "at {t:?}");
            assert_eq!(a.w.state_hash(), b.w.state_hash(), "at {t:?}");
            b.d.enforce_budget();
            compared += 1;
        }
    }
    assert_eq!(compared, DAYS / 2);
    assert_exercised(&a, &b);
    assert_eq!(a.d.state_hash(), b.d.state_hash());
    assert_eq!(save_hash(&a.d), save_hash(&b.d), "the same save");
    assert_eq!(a.d.stats, b.d.stats);
    drop(b);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_paged_save_restores_and_continues_exactly() {
    let seed = 33;
    let (dir_b, dir_c) = (tmpdir("restore-b"), tmpdir("restore-c"));
    let mut a = start(seed);
    let mut b = start(seed);
    page(&mut b, &dir_b);
    let mid = Tick::from_days(DAYS / 2) + 3 * STEP;
    let mut t = Tick::ZERO;
    while t < mid {
        t += STEP;
        react(&mut a, t);
        react(&mut b, t);
        assert_eq!(step(&mut a, t), step(&mut b, t), "commands at {t:?}");
        if t.0.is_multiple_of(SAVE_EVERY) {
            save(&mut b);
        }
    }
    // The midpoint: a save between the periodic ones, then a second engine from its pages, with
    // the world and the plugin as they were at that moment.
    save(&mut b);
    copy_dir(&dir_b, &dir_c);
    let store_c = Arc::new(RwLock::new(
        PageStore::open(&dir_c, Durability::NoSync).expect("the copy opens"),
    ));
    let d = Dragons::open_paged(store_c.clone(), BUDGET, META, DragonTunables::default())
        .expect("the dragons open from their pages");
    assert_eq!(d.resident_row_bytes(), 0, "nothing resident on open");
    assert_eq!(d.dragons.len(), b.d.dragons.len());
    assert_eq!(d.lairs.len(), b.d.lairs.len());
    let (goods, ids) = synth::goods();
    let mut cfg = WorldConfig::new(seed);
    cfg.check_every_event = false;
    let w = World::load(cfg, goods, synth::recipes(&ids), &b.w.save()).expect("the world loads");
    let mut c = Run {
        w,
        d,
        iaf: b.iaf.clone(),
        store: Some(store_c),
    };
    assert_eq!(c.d.state_hash(), b.d.state_hash(), "restored");
    assert_eq!(c.d.state_hash(), a.d.state_hash(), "restored, against the resident run");
    c.d.enforce_budget();
    while t < Tick::from_days(DAYS) {
        t += STEP;
        for r in [&mut a, &mut b, &mut c] {
            react(r, t);
        }
        let ca = step(&mut a, t);
        assert_eq!(ca, step(&mut b, t), "commands at {t:?}");
        assert_eq!(ca, step(&mut c, t), "the restored engine's commands at {t:?}");
        if t.0.is_multiple_of(SAVE_EVERY) {
            save(&mut b);
            save(&mut c);
        }
        if t.0.is_multiple_of(TICKS_PER_DAY) {
            let h = a.d.state_hash();
            assert_eq!(h, b.d.state_hash(), "at {t:?}");
            assert_eq!(h, c.d.state_hash(), "the restored engine at {t:?}");
            assert_eq!(a.w.state_hash(), c.w.state_hash(), "the restored world at {t:?}");
            b.d.enforce_budget();
            c.d.enforce_budget();
        }
    }
    assert_exercised(&a, &c);
    let h = save_hash(&a.d);
    assert_eq!(h, save_hash(&b.d));
    assert_eq!(h, save_hash(&c.d), "the restored engine saves the same");
    drop((b, c));
    std::fs::remove_dir_all(&dir_b).unwrap();
    std::fs::remove_dir_all(&dir_c).unwrap();
}

/// A save that fails leaves its pages dirty: the next save writes them, and an engine opened
/// from that save is the engine that never failed (a resident twin's state).
#[test]
fn a_failed_save_loses_nothing() {
    let seed = 7;
    let (dir, dir2) = (tmpdir("failed"), tmpdir("failed-copy"));
    let mut a = start(seed);
    let mut r = start(seed);
    page(&mut r, &dir);
    let mut t = Tick::ZERO;
    while t < Tick::from_days(2) + 8 * STEP {
        t += STEP;
        assert_eq!(step(&mut a, t), step(&mut r, t));
        if t.0.is_multiple_of(SAVE_EVERY) {
            save(&mut r);
        }
    }
    // A save whose commit never happens (its changes since the last save are only in memory),
    // then more changes, then a real save.
    let mut lost = SaveBatch::new();
    let tickets = r.d.save_paged(&mut lost, META);
    assert!(tickets.pages() > 0);
    r.d.save_failed(tickets);
    drop(lost);
    while t < Tick::from_days(3) {
        t += STEP;
        assert_eq!(step(&mut a, t), step(&mut r, t), "at {t:?}");
    }
    assert_eq!(r.d.state_hash(), a.d.state_hash(), "the failed save lost a change");
    save(&mut r);
    copy_dir(&dir, &dir2);
    let store2 = Arc::new(RwLock::new(PageStore::open(&dir2, Durability::NoSync).unwrap()));
    let back = Dragons::open_paged(store2, BUDGET, META, DragonTunables::default()).unwrap();
    assert_eq!(back.state_hash(), a.d.state_hash());
    assert_eq!(save_hash(&back), save_hash(&a.d));
    drop(r);
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::remove_dir_all(&dir2).unwrap();
}
