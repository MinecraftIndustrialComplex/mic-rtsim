// SPDX-License-Identifier: AGPL-3.0-or-later
//! Peeks (VIEWER.md §5.3): `&self` reads for a viewer. A peek agrees with the committing read at
//! the same time, and leaves the table's bytes, `touched` and statistics as they were.

use rtsim_kernel::clock::{SlotPeriod, Tick};
use rtsim_kernel::codec::{Reader, Writer};
use rtsim_kernel::lazy::{
    LazyModel, LazyTable, ModelCodec, PEEK_MAX_SLOTS, PagedLazyTable, RegionId,
};
use rtsim_kernel::page::{Durability, PageKey, PageStore, SaveBatch};
use rtsim_kernel::rng::{SlotRng, SystemId};
use rtsim_kernel::rows::PageSource;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const P: u64 = 100;

/// Grows by a random amount each slot and drifts linearly in between.
#[derive(Clone, Debug, Default, PartialEq)]
struct Walk {
    level: u64,
    commits: u64,
}

impl LazyModel for Walk {
    type View = (u64, u64);
    fn slot_period(&self) -> Option<SlotPeriod> {
        Some(SlotPeriod::ticks(P))
    }
    fn settle(&mut self, from: Tick, to: Tick) {
        self.level += to.0 - from.0;
    }
    fn commit_slot(&mut self, _slot: u64, rng: &mut SlotRng) {
        self.level += rng.below(1_000);
        self.commits += 1;
    }
    fn view(&self, anchor: Tick, at: Tick) -> (u64, u64) {
        (self.level + (at.0 - anchor.0), self.commits)
    }
}

struct Codec;

impl ModelCodec<Walk> for Codec {
    fn put(m: &Walk, w: &mut Writer) {
        w.u64(m.level);
        w.u64(m.commits);
    }
    fn get(r: &mut Reader<'_>) -> rtsim_kernel::codec::Result<Walk> {
        Ok(Walk {
            level: r.u64()?,
            commits: r.u64()?,
        })
    }
}

fn bytes(t: &LazyTable<Walk>) -> Vec<u8> {
    let mut w = Writer::new();
    t.encode_with(&mut w, Codec::put);
    w.into_bytes()
}

fn drain(t: &mut LazyTable<Walk>, to: Tick) {
    while t.next_due(to).is_some() {}
}

#[test]
fn a_peek_agrees_with_a_read_and_changes_nothing() {
    let mut t: LazyTable<Walk> = LazyTable::new(7, SystemId::named("test.peek"), Tick::ZERO);
    let hs: Vec<RegionId<Walk>> = (0..5)
        .map(|k| t.insert(k, Walk::default(), Tick(k * 37)))
        .collect();
    for at in [Tick(150), Tick(999), Tick(5_432)] {
        drain(&mut t, at);
        for &h in &hs {
            let before = bytes(&t);
            let (touched, stats) = (t.get(h).unwrap().touched(), t.stats());
            let peek = t.peek(h, at).unwrap();
            assert_eq!(bytes(&t), before);
            assert_eq!(t.get(h).unwrap().touched(), touched);
            assert_eq!(t.stats(), stats);
            assert_eq!(peek.as_of, at);
            assert_eq!(Some(peek.view), t.read(h, at));
        }
    }
    // Never past what the table has drained.
    let h = hs[0];
    let now = t.now();
    let far = t.peek(h, Tick(now.0 + 10 * P)).unwrap();
    assert_eq!(far.as_of, now);
    assert_eq!(Some(far.view), t.read(h, now));
}

#[test]
fn a_long_peek_answers_as_of_the_anchor() {
    let mut t: LazyTable<Walk> = LazyTable::new(7, SystemId::named("test.peek"), Tick::ZERO);
    let h = t.insert(1, Walk::default(), Tick::ZERO);
    let at = Tick((PEEK_MAX_SLOTS + 5) * P);
    drain(&mut t, at);
    let peek = t.peek(h, at).unwrap();
    assert_eq!(peek.as_of, Tick::ZERO);
    assert_eq!(peek.view, (0, 0));
    // Within the cap, a peek still catches up a clone.
    let at = Tick(PEEK_MAX_SLOTS * P);
    let peek = t.peek(h, at).unwrap();
    assert_eq!(peek.as_of, at);
    assert_eq!(peek.view.1, PEEK_MAX_SLOTS);
}

#[test]
fn a_paged_peek_agrees_with_a_read_and_leaves_pages_clean() {
    type Table = PagedLazyTable<Walk, Codec>;
    let mut t = Table::new(7, SystemId::named("test.peek"), Tick::ZERO, 0x7002, 4);
    let hs: Vec<RegionId<Walk>> = (0..9)
        .map(|k| t.insert(k, Walk::default(), Tick(k * 11)))
        .collect();
    let encode = |t: &Table| {
        let mut w = Writer::new();
        t.encode_meta(&mut w);
        t.encode_rows(&mut w);
        w.into_bytes()
    };
    for at in [Tick(250), Tick(3_001)] {
        while t.next_due(at).is_some() {}
        for &h in &hs {
            let before = encode(&t);
            let touched = t.get(h).unwrap().touched();
            let peek = t.peek(h, at).unwrap();
            assert_eq!(encode(&t), before);
            assert_eq!(t.get(h).unwrap().touched(), touched);
            assert_eq!(Some(peek.view), t.read(h, at));
        }
    }
}

fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let bytes = std::fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect()
}

/// The viewer's case: a table the engine saved, opened over a read-only store with no row
/// resident. Each peek loads its page through `&self`, agrees with what the engine saw, leaves the
/// loaded pages clean (a save would write nothing) and the folder as it was.
#[test]
fn a_peek_over_a_read_only_store_loads_saved_pages_clean() {
    type Table = PagedLazyTable<Walk, Codec>;
    const TABLE: u16 = 0x7002;
    const META: PageKey = PageKey::new(0x7001, 0);
    let dir = std::env::temp_dir().join(format!("rtsim-lazy-peek-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut t = Table::new(7, SystemId::named("test.peek"), Tick::ZERO, TABLE, 4);
    let hs: Vec<RegionId<Walk>> = (0..13)
        .map(|k| t.insert(k, Walk::default(), Tick(k * 11)))
        .collect();
    let at = Tick(2_750);
    while t.next_due(at).is_some() {}
    let seen: Vec<_> = hs.iter().map(|&h| t.peek(h, at).unwrap()).collect();

    // The engine's save: its rows' pages and its meta page.
    let store = Arc::new(Mutex::new(
        PageStore::open(&dir, Durability::NoSync).unwrap(),
    ));
    t.rows_mut()
        .attach(Arc::clone(&store) as Arc<dyn PageSource>, 1 << 20);
    let mut batch = SaveBatch::new();
    let ticket = t.rows_mut().save_dirty(&mut batch);
    let mut w = Writer::new();
    t.encode_meta(&mut w);
    batch.put(META, &w.into_bytes());
    store.lock().unwrap().commit(batch, 1).unwrap();
    t.rows_mut().saved(ticket);
    drop(t);
    drop(store);

    // The viewer's open: read only, meta first, rows on demand.
    let before = files(&dir);
    let ro = Arc::new(PageStore::open_read_only(&dir).unwrap());
    let meta = ro.get_shared(META).unwrap().expect("the meta page");
    let (mut v, counts) = Table::decode_meta(&mut Reader::new(&meta), TABLE, 4).unwrap();
    v.open_rows(Arc::clone(&ro) as Arc<dyn PageSource>, 1 << 20, counts);
    assert_eq!(v.rows().resident_pages(), 0, "nothing resident at open");
    for (&h, want) in hs.iter().zip(&seen) {
        assert_eq!(v.peek(h, at).as_ref(), Some(want), "row {}", h.index());
    }
    assert!(v.rows().resident_pages() > 1, "the peeks loaded pages");
    let (dirty, _) = v.rows_mut().snapshot_dirty();
    assert!(dirty.is_empty(), "a peek leaves its pages clean: {dirty:?}");
    drop(v);
    drop(ro);
    assert!(before == files(&dir), "the store's folder changed");
    let _ = std::fs::remove_dir_all(&dir);
}
