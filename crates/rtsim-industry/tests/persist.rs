// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving and restoring the industry engine (src/persist.rs): a run saved and restored at any
//! moment ends in exactly the state of a run that never stopped.

use rtsim_industry::engine::Engine;
use rtsim_industry::scenario::{IronLine, iron_line, run_iron_line, run_iron_line_until};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};

fn cfg() -> IronLine {
    IronLine {
        seed: 5,
        days: 10,
        threats: true,
        control: false,
        approach: true,
        tick_first: true,
    }
}

fn snapshot(e: &Engine) -> Vec<(u64, Vec<u8>)> {
    let mut v: Vec<(u64, Vec<u8>)> = e.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    v.sort();
    v
}

#[test]
fn a_restored_engine_continues_exactly() {
    let c = cfg();
    let mut whole = iron_line(c, Tunables::default());
    run_iron_line(&mut whole, &c);
    let want = snapshot(&whole.engine);
    for cut_days in [2.5, 5.0, 8.0, 9.5] {
        let mut w = iron_line(c, Tunables::default());
        // 8.0 days plus two ticks: reconciles are in flight.
        let cut = Tick((cut_days * TICKS_PER_DAY as f64) as u64 + 2);
        run_iron_line_until(&mut w, &c, cut);
        let pages = w.engine.snapshot();
        let restored = Engine::restore(pages, Tunables::default()).expect("restore");
        assert_eq!(
            snapshot(&restored),
            snapshot(&w.engine),
            "round trip at day {cut_days}"
        );
        w.engine = restored;
        run_iron_line(&mut w, &c);
        let got = snapshot(&w.engine);
        assert_eq!(
            got.len(),
            want.len(),
            "page count after a restore at day {cut_days}"
        );
        for (a, b) in got.iter().zip(&want) {
            assert_eq!(a.0, b.0);
            assert!(
                a.1 == b.1,
                "page {:#x} differs after a restore at day {cut_days}",
                a.0
            );
        }
        w.conservation().expect("conserves across a restore");
    }
}

#[test]
fn corrupt_pages_are_errors_not_panics() {
    let c = cfg();
    let mut w = iron_line(c, Tunables::default());
    w.run_until(Tick(3 * TICKS_PER_DAY));
    let pages = w.engine.snapshot();
    for (i, (k, b)) in pages.iter().enumerate() {
        for cut in [0, b.len() / 2, b.len().saturating_sub(1)] {
            let mut broken = pages.clone();
            broken[i] = (*k, b[..cut].to_vec());
            assert!(
                Engine::restore(broken, Tunables::default()).is_err(),
                "page {i} cut at {cut}"
            );
        }
    }
}

/// A store's industry pages: what the saves put and deleted, by key.
type Disk = std::collections::BTreeMap<u64, Vec<u8>>;

/// One save into `disk` as the host makes it (docs/choices-industry-dirty-pages.md ID2): the
/// pages changed since the save before, and the deletions. The disk then holds the engine's
/// whole state, page for page. Returns the keys written and deleted.
fn save(e: &mut Engine, disk: &mut Disk) -> (Vec<u64>, Vec<u64>) {
    let c = e.save_changed();
    let deleted: Vec<u64> = c.delete.iter().map(|k| k.0).collect();
    for k in &deleted {
        disk.remove(k);
    }
    let mut put = Vec::new();
    for (k, b) in c.put {
        put.push(k.0);
        disk.insert(k.0, b);
    }
    let whole: Disk = snapshot(e).into_iter().collect();
    assert!(*disk == whole, "the saves' pages are the engine's whole state");
    if let Some(c) = e.page_check() {
        assert_eq!(c.unmarked, 0, "{:?}", c.first_unmarked);
    }
    (put, deleted)
}

fn restore(disk: &Disk) -> Engine {
    let pages = disk
        .iter()
        .map(|(k, b)| (rtsim_kernel::page::PageKey(*k), b.clone()));
    let e = Engine::restore(pages, Tunables::default()).expect("restore");
    let back: Disk = snapshot(&e).into_iter().collect();
    assert!(back == *disk, "a restored engine saves what it was restored from");
    e
}

/// The iron line saved as the host saves it, every 6,000 ticks (an autosave), each save only the
/// pages changed since the one before (replay clusters that dissolve deleted), and restored from
/// those pages at every third save: it ends exactly as the run that never stopped.
#[test]
fn a_run_restored_from_its_changed_pages_continues_exactly() {
    let c = cfg();
    let mut whole = iron_line(c, Tunables::default());
    run_iron_line(&mut whole, &c);
    let want = snapshot(&whole.engine);
    let mut w = iron_line(c, Tunables::default());
    w.check_saves = false;
    let mut disk = Disk::new();
    let end = c.days * TICKS_PER_DAY;
    let (mut saves, mut written, mut restores) = (0u64, 0u64, 0u64);
    let mut t = 6_002;
    while t < end {
        run_iron_line_until(&mut w, &c, Tick(t));
        let (put, deleted) = save(&mut w.engine, &mut disk);
        saves += 1;
        written += (put.len() + deleted.len()) as u64;
        if saves.is_multiple_of(3) {
            w.engine = restore(&disk);
            restores += 1;
        }
        t += 6_000;
    }
    run_iron_line(&mut w, &c);
    assert_eq!(snapshot(&w.engine), want, "the run restored from its saves");
    w.conservation().expect("conserves across the restores");
    // The iron line is busy (its trains and visits touch every district between two autosaves),
    // so its saves write nearly every page: the test is the restores, not the saving.
    assert!(restores >= 10 && written >= saves, "{restores} restores, {written} pages");
}

/// A save writes the globals page and only the pages changed since the one before: nothing else
/// when nothing changed, a district's own pages when it changed, and its pages' deletions when it
/// is removed.
#[test]
fn a_save_writes_only_what_changed() {
    use rtsim_industry::ids::{BlockPos, DistrictId};
    use rtsim_industry::obs::{EditKind, Obs, Timed};
    use rtsim_industry::persist::{DISTRICT_TABLE, GLASS_TABLE, GLOBAL_TABLE};
    use rtsim_kernel::page::PageKey;
    let c = cfg();
    let mut w = iron_line(c, Tunables::default());
    w.check_saves = false;
    run_iron_line_until(&mut w, &c, Tick(3 * TICKS_PER_DAY));
    let mut disk = Disk::new();
    let (first, _) = save(&mut w.engine, &mut disk);
    assert!(first.len() > 3, "the first save writes every page: {first:?}");
    let globals = PageKey::new(GLOBAL_TABLE, 0).0;
    // Nothing changed: the globals page alone.
    assert_eq!(save(&mut w.engine, &mut disk), (vec![globals], vec![]));
    let d = w.districts[0].id;
    let at = w.engine.now();
    let ingest = |e: &mut Engine, obs: Obs| e.ingest(&Timed { at, obs });
    ingest(
        &mut w.engine,
        Obs::Edit {
            district: d,
            kind: EditKind::Placed,
            player: (1, 2),
            pos: BlockPos::default(),
        },
    );
    let (put, deleted) = save(&mut w.engine, &mut disk);
    assert!(deleted.is_empty(), "{deleted:?}");
    assert!(put.contains(&globals) && put.contains(&PageKey::new(DISTRICT_TABLE, d.0).0));
    for k in &put {
        let k = PageKey(*k);
        if k.table() == DISTRICT_TABLE || k.table() == GLASS_TABLE {
            assert_eq!(DistrictId(k.id()), d, "only the edited district's pages: {put:?}");
        }
    }
    assert!(put.len() < first.len(), "{put:?} of {first:?}");
    // Removed: its pages are deleted, and the disk holds what a restore needs (`save` checks).
    let had_glass = disk.contains_key(&PageKey::new(GLASS_TABLE, d.0).0);
    ingest(&mut w.engine, Obs::DistrictRemoved { district: d });
    let (_, deleted) = save(&mut w.engine, &mut disk);
    assert!(deleted.contains(&PageKey::new(DISTRICT_TABLE, d.0).0), "{deleted:?}");
    assert_eq!(
        deleted.contains(&PageKey::new(GLASS_TABLE, d.0).0),
        had_glass,
        "{deleted:?}"
    );
    restore(&disk);
}

/// A save that does not reach the disk hands its pages back: the next save writes them again.
#[test]
fn a_failed_save_is_written_again_by_the_next() {
    let c = cfg();
    let mut w = iron_line(c, Tunables::default());
    w.check_saves = false;
    run_iron_line_until(&mut w, &c, Tick(2 * TICKS_PER_DAY));
    let mut disk = Disk::new();
    save(&mut w.engine, &mut disk);
    run_iron_line_until(&mut w, &c, Tick(2 * TICKS_PER_DAY + 3_000));
    let lost = w.engine.save_changed();
    assert!(!lost.ticket.is_empty(), "the lost save wrote district pages");
    let lost_keys: Vec<u64> = lost.put.iter().map(|(k, _)| k.0).collect();
    w.engine.save_failed(lost.ticket);
    run_iron_line_until(&mut w, &c, Tick(2 * TICKS_PER_DAY + 3_100));
    let (put, _) = save(&mut w.engine, &mut disk);
    for k in &lost_keys {
        assert!(put.contains(k), "page {k:#x} of the lost save is written again");
    }
    restore(&disk);
}
