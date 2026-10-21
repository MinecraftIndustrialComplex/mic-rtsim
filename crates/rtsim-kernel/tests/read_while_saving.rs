// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saves succeed while a viewer reads the store (VIEWER.md §5.3, §16 row V4 (e)). The danger is
//! Windows: there a writer cannot delete or rename over a file another process holds open unless
//! that process opened it with `FILE_SHARE_DELETE`. A read-only store holds no file open between
//! reads, and std opens with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, so every
//! save goes through: these tests hold that on whatever they run on (Linux in CI, Windows through
//! the cross-built test binary).
//!
//! The first test is the worst moment, made certain: before every save a handle is open on every
//! file in the folder (what a read in flight, or a read-only open reading the root and the log,
//! holds), and the save compacts segments, deletes them and folds the log. The second runs a
//! writer saving as fast as it can against readers that open read only and read every page in a
//! loop, and checks no reader ever got a wrong page.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rtsim_kernel::page::{Durability, PageError, PageKey, PageStore, SaveBatch};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "rtsim-read-while-saving-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// A page that names itself: its key and version up front, then bytes derived from both, so a
/// reader can tell a right page from a wrong or torn one without the writer's model.
fn page(key: u64, version: u64, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len.max(16));
    v.extend_from_slice(&key.to_le_bytes());
    v.extend_from_slice(&version.to_le_bytes());
    let mut x = key.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ version;
    while v.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.push(x as u8);
    }
    v
}

/// Whether `bytes` is the page it says it is, for `key`.
fn names_itself(key: PageKey, bytes: &[u8]) -> bool {
    if bytes.len() < 16 {
        return false;
    }
    let k = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    let version = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    k == key.0 && bytes == page(k, version, bytes.len()).as_slice()
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .collect()
}

/// One save of round `round`: rewrites a few pages (so older segments drop under half live and
/// get compacted), deletes one now and then, adds one now and then.
fn round_batch(round: u64, model: &mut BTreeMap<u64, Vec<u8>>) -> SaveBatch {
    let mut b = SaveBatch::new();
    for i in 0..4 {
        let k = (round * 7 + i * 13) % 48;
        let v = page(k, round + 1, 600 + ((round + i) % 5) as usize * 300);
        b.put(PageKey(k), &v);
        model.insert(k, v);
    }
    if round % 5 == 3 {
        let k = (round * 11) % 48;
        b.delete(PageKey(k));
        model.remove(&k);
    }
    if round % 9 == 0 {
        let k = 48 + round;
        let v = page(k, round + 1, 2000);
        b.put(PageKey(k), &v);
        model.insert(k, v);
    }
    b
}

fn seed(s: &mut PageStore, model: &mut BTreeMap<u64, Vec<u8>>) {
    let mut b = SaveBatch::new();
    for k in 0..48 {
        let v = page(k, 0, 2000);
        b.put(PageKey(k), &v);
        model.insert(k, v);
    }
    s.commit(b, 0).unwrap();
}

/// Reads every page of a read-only store; `Err` when the writer moved a file under it (the store
/// is then stale and the viewer reopens). Panics on a page that is not what it says it is.
fn read_every_page(ro: &PageStore) -> Result<usize, PageError> {
    let mut n = 0;
    for key in ro.try_keys()? {
        if let Some(bytes) = ro.get_shared(key)? {
            assert!(names_itself(key, &bytes), "page {key:?} came back wrong");
            n += 1;
        }
    }
    Ok(n)
}

fn assert_matches(dir: &Path, model: &BTreeMap<u64, Vec<u8>>) {
    let ro = PageStore::open_read_only(dir).unwrap();
    let got: BTreeMap<u64, Vec<u8>> = ro
        .try_keys()
        .unwrap()
        .into_iter()
        .map(|k| (k.0, ro.get_shared(k).unwrap().unwrap()))
        .collect();
    assert!(got == *model, "the store and the model differ");
}

/// Before every save a handle is open on every file in the store's folder; the save compacts,
/// deletes segments and (every few saves) folds the log, and every save succeeds. The held
/// handles still read afterwards, and a fresh read-only open reads exactly what was saved.
#[test]
fn saves_succeed_while_every_file_is_held_open() {
    let dir = tmpdir("held");
    let mut s = PageStore::open(&dir, Durability::Fsync).unwrap();
    let mut model = BTreeMap::new();
    seed(&mut s, &mut model);
    let (mut removed, mut compacted, mut folds, mut held_most) = (0, 0, 0, 0);
    for round in 1..=60u64 {
        // What a viewer can hold at any instant: a read-only store (it holds nothing) and, in
        // the worst case, a read in flight on every file at once.
        let ro = PageStore::open_read_only(&dir).unwrap();
        let mut held: Vec<(PathBuf, File)> = files_in(&dir)
            .into_iter()
            .map(|p| {
                let f = File::open(&p).unwrap();
                (p, f)
            })
            .collect();
        held_most = held_most.max(held.len());
        let b = round_batch(round, &mut model);
        let st = s
            .commit(b, round)
            .unwrap_or_else(|e| panic!("save {round} with every file held open: {e}"));
        removed += st.segments_removed;
        compacted += st.pages_compacted;
        if st.checkpoint_bytes > 0 {
            folds += 1;
        }
        if round % 7 == 0 {
            s.checkpoint_now()
                .unwrap_or_else(|e| panic!("fold at save {round} with every file held open: {e}"));
            folds += 1;
        }
        // A handle on a file the save deleted or replaced still reads what it opened.
        for (p, f) in &mut held {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        }
        drop(held);
        drop(ro);
    }
    println!(
        "60 saves with up to {held_most} files held open: {removed} segments removed, \
         {compacted} pages compacted, {folds} log folds"
    );
    assert!(removed > 0, "no segment was deleted under a held handle");
    assert!(compacted > 0, "no page was compacted");
    assert!(folds > 0, "the log was never folded");
    drop(s);
    assert_matches(&dir, &model);
    // The writer reopens it too, and cleans whatever it left.
    let s = PageStore::open(&dir, Durability::Fsync).unwrap();
    drop(s);
    assert_matches(&dir, &model);
    fs::remove_dir_all(&dir).unwrap();
}

/// A writer saving as fast as it can (compacting, deleting, folding) while three readers open
/// the store read only and read every page in a loop: every save succeeds, every page a reader
/// got is the page it asked for, and the readers finished whole reads during the saves.
#[test]
fn saves_succeed_while_viewers_read_in_a_loop() {
    let dir = tmpdir("loop");
    let mut s = PageStore::open(&dir, Durability::NoSync).unwrap();
    let mut model = BTreeMap::new();
    seed(&mut s, &mut model);
    let stop = Arc::new(AtomicBool::new(false));
    let whole = Arc::new(AtomicU64::new(0));
    let stale = Arc::new(AtomicU64::new(0));
    let busy = Arc::new(AtomicU64::new(0));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let (dir, stop) = (dir.clone(), Arc::clone(&stop));
            let (whole, stale, busy) = (Arc::clone(&whole), Arc::clone(&stale), Arc::clone(&busy));
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let ro = match PageStore::open_read_only(&dir) {
                        Ok(ro) => ro,
                        Err(PageError::Busy(_)) => {
                            busy.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        Err(e) => panic!("a read-only open failed: {e}"),
                    };
                    match read_every_page(&ro) {
                        Ok(_) => whole.fetch_add(1, Ordering::Relaxed),
                        Err(_) => stale.fetch_add(1, Ordering::Relaxed),
                    };
                }
            })
        })
        .collect();

    let started = Instant::now();
    let (mut saves, mut removed, mut folds) = (0u64, 0, 0);
    // At least 400 saves and a second, so the readers overlap many of them.
    while saves < 400 || started.elapsed() < Duration::from_secs(1) {
        saves += 1;
        let b = round_batch(saves, &mut model);
        let st = s
            .commit(b, saves)
            .unwrap_or_else(|e| panic!("save {saves} while viewers read: {e}"));
        removed += st.segments_removed;
        if saves % 25 == 0 {
            s.checkpoint_now()
                .unwrap_or_else(|e| panic!("fold at save {saves} while viewers read: {e}"));
            folds += 1;
        }
    }
    let during = whole.load(Ordering::Relaxed);
    stop.store(true, Ordering::Relaxed);
    for r in readers {
        r.join()
            .expect("a reader got a wrong page or a failed open");
    }
    println!(
        "{saves} saves in {:.2} s ({removed} segments removed, {folds} folds): readers finished \
         {during} whole reads during them, {} went stale, {} opens busy",
        started.elapsed().as_secs_f64(),
        stale.load(Ordering::Relaxed),
        busy.load(Ordering::Relaxed)
    );
    assert!(removed > 0 && folds > 0);
    assert!(
        during > 0,
        "no reader finished a whole read during the saves"
    );
    drop(s);
    assert_matches(&dir, &model);
    fs::remove_dir_all(&dir).unwrap();
}
