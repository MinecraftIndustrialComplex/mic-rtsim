// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kills a process in the middle of its saves (SIGKILL, no cleanup, no unwinding) and checks what
//! it left: the store must open at a complete save, every page must be the one that save wrote,
//! and a pending generation must still roll back to the confirmed one (P29).
//!
//! The test binary runs itself as the child (`saver_child`, ignored unless the parent sets
//! `RTSIM_CRASH_DIR`). The child saves forever with fsync on; the parent kills it after a
//! pseudo-random delay. Every save's contents are a pure function of its number, so the parent
//! rebuilds what any save number must contain.
//!
//! What this covers and what it does not: a killed process loses nothing the kernel has accepted
//! (data in the page cache survives), so this tests the save order against process crashes. Power
//! loss also loses unsynced data; the fsync order is what covers that, and `page.rs`'s fault
//! tests stop a save at each point of it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};

const DIR_ENV: &str = "RTSIM_CRASH_DIR";
const KEYS: u64 = 300;

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// What save `s` writes: puts (with contents) and deletes.
fn changes(s: u64) -> Vec<(u64, Option<Vec<u8>>)> {
    let n = 1 + mix(s) % 40;
    let mut out = Vec::new();
    for i in 0..n {
        let key = mix(s * 1000 + i) % KEYS;
        let len = 200 + (mix(key ^ s) % 6000) as usize;
        let mut x = mix(key * 7919 + s) | 1;
        let bytes = (0..len)
            .map(|j| {
                // Half compressible, half noise, so segments have real size.
                if j % 2 == 0 {
                    (j % 7) as u8
                } else {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                }
            })
            .collect();
        out.push((key, Some(bytes)));
    }
    if s.is_multiple_of(4) {
        out.push((mix(s ^ 0xdead) % KEYS, None));
    }
    out
}

/// The store's pages after saves 1..=n.
fn model(n: u64) -> BTreeMap<u64, Vec<u8>> {
    let mut m = BTreeMap::new();
    for s in 1..=n {
        // A later put of a key in the same save replaces the earlier one, as in SaveBatch.
        let mut batch: BTreeMap<u64, Option<Vec<u8>>> = BTreeMap::new();
        for (k, v) in changes(s) {
            batch.insert(k, v);
        }
        for (k, v) in batch {
            match v {
                Some(b) => {
                    m.insert(k, b);
                }
                None => {
                    m.remove(&k);
                }
            }
        }
    }
    m
}

fn batch(s: u64) -> SaveBatch {
    let mut b = SaveBatch::new();
    for (k, v) in changes(s) {
        match v {
            Some(bytes) => b.put(PageKey(k), &bytes),
            None => b.delete(PageKey(k)),
        }
    }
    b
}

/// Saves forever: every third save is pending and then confirmed, the others confirmed at once;
/// now and then a forced checkpoint.
fn saver(dir: &Path) -> ! {
    let mut store = PageStore::open(dir, Durability::Fsync).expect("open");
    let mut s = store.save_seq();
    loop {
        s += 1;
        if s.is_multiple_of(3) {
            store.commit_pending(batch(s), s).expect("pending save");
            // Leave some pending generations unconfirmed for a while.
            if !s.is_multiple_of(9) {
                store.confirm(s).expect("confirm");
            }
        } else {
            store.commit(batch(s), s).expect("save");
        }
        if s.is_multiple_of(25) {
            store.checkpoint_now().expect("checkpoint");
        }
    }
}

#[test]
#[ignore = "the child half of kill_mid_save; runs only when that test starts it"]
fn saver_child() {
    if let Some(dir) = std::env::var_os(DIR_ENV) {
        saver(Path::new(&dir));
    }
}

fn check(dir: &Path, round: u64) -> u64 {
    let names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let mut store = PageStore::open(dir, Durability::NoSync)
        .unwrap_or_else(|e| panic!("round {round}: open after a kill failed: {e} ({names:?})"));
    let cur = store.current();
    let want = model(cur.save_seq);
    assert_eq!(cur.obs_seq, cur.save_seq, "round {round}");
    assert_eq!(
        store.len(),
        want.len(),
        "round {round}, save {}",
        cur.save_seq
    );
    for (k, v) in &want {
        let got = store.get(PageKey(*k)).unwrap();
        assert_eq!(
            got.as_ref(),
            Some(v),
            "round {round}: key {k} at save {}",
            cur.save_seq
        );
    }
    // No temporary file survives an open; every segment on disk is in the index.
    let after: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        after.iter().all(|n| !n.ends_with(".tmp")),
        "round {round}: {after:?}"
    );
    let segs = after.iter().filter(|n| n.ends_with(".rts")).count();
    assert_eq!(segs, store.segment_count(), "round {round}: {after:?}");
    // A pending generation still rolls back to exactly the confirmed save. On a copy: the next
    // life continues from the original as it was left.
    let conf = store.confirmed();
    if conf != cur {
        drop(store);
        let copy = dir.with_extension("copy");
        let _ = std::fs::remove_dir_all(&copy);
        std::fs::create_dir_all(&copy).unwrap();
        for e in std::fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), copy.join(e.file_name())).unwrap();
        }
        let mut store = PageStore::open_for_journal(&copy, Durability::NoSync, conf.obs_seq)
            .unwrap_or_else(|e| panic!("round {round}: rollback open failed: {e}"));
        let want = model(conf.save_seq);
        assert_eq!(store.current(), conf, "round {round}");
        assert_eq!(store.len(), want.len(), "round {round}");
        for (k, v) in &want {
            let got = store.get(PageKey(*k)).unwrap();
            assert_eq!(got.as_ref(), Some(v), "round {round}: rolled back key {k}");
        }
        drop(store);
        std::fs::remove_dir_all(&copy).unwrap();
    }
    cur.save_seq
}

#[test]
fn kill_mid_save() {
    let exe = std::env::current_exe().unwrap();
    let base: PathBuf =
        std::env::temp_dir().join(format!("rtsim-crash-kill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mut saves = Vec::new();
    for round in 0..16u64 {
        let dir = base.join(format!("r{round}"));
        std::fs::create_dir_all(&dir).unwrap();
        // Two lives per round: kill, check, restart from what was left, kill again, check.
        for life in 0..2 {
            let mut child = Command::new(&exe)
                .args([
                    "--exact",
                    "saver_child",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(DIR_ENV, &dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            let delay = 30 + mix(round * 2 + life) % 300;
            std::thread::sleep(Duration::from_millis(delay));
            child.kill().unwrap();
            let status = child.wait().unwrap();
            assert!(!status.success(), "the saver never stops on its own");
            saves.push(check(&dir, round));
        }
    }
    // The kills must have landed after real work, or the test proves nothing.
    assert!(
        saves.iter().filter(|&&s| s > 5).count() >= 8,
        "too few saves before the kills: {saves:?}"
    );
    eprintln!("saves completed before each kill: {saves:?}");
    std::fs::remove_dir_all(&base).unwrap();
}
