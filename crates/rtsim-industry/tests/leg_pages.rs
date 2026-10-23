// SPDX-License-Identifier: AGPL-3.0-or-later
//! The booked train legs are saved in pages of their own, not in the globals page (choice ID10,
//! docs/choices-industry-dirty-pages.md): docs/stress-worlds-results.md 10.9 found the globals page
//! growing with the line legs (91, 383, 636 KB raw on the syn200 arms) and rewritten whole at every
//! save. A save writes only the leg pages a booking, a loot or an expiry changed.

use std::collections::BTreeMap;

use rtsim_industry::engine::Engine;
use rtsim_industry::ids::{BlockPos, DistrictId, Good, GoodInfo, StationId, TrainId};
use rtsim_industry::obs::{Obs, StationDecl, Timed, TrainDeparture};
use rtsim_industry::persist::{GLOBAL_TABLE, LEG_TABLE};
use rtsim_industry::threat::{LEGS_PER_PAGE, LegIndex};
use rtsim_industry::tunables::Tunables;
use rtsim_kernel::clock::{TICKS_PER_DAY, Tick};
use rtsim_kernel::page::PageKey;

const IRON: Good = Good(1);
const WEST: StationId = StationId(10);
const EAST: StationId = StationId(11);
const D1: DistrictId = DistrictId(1);

fn engine() -> Engine {
    let mut e = Engine::new(1, Tunables::default(), Tick(0));
    e.define_good(IRON, GoodInfo::item("iron", 64));
    for (station, district, x) in [(WEST, D1, 0), (EAST, DistrictId(2), 4000)] {
        send(
            &mut e,
            0,
            Obs::Station(StationDecl {
                station,
                district: Some(district),
                dim: 0,
                pos: BlockPos::new(x, 64, 0),
                name: 0,
            }),
        );
    }
    e
}

fn send(e: &mut Engine, at: u64, obs: Obs) {
    e.ingest(&Timed { at: Tick(at), obs });
}

/// The `n`th train leaves the west station for the east one at `at`: one booked leg.
fn depart(e: &mut Engine, at: u64, n: u64) {
    send(
        e,
        at,
        Obs::TrainDeparted(TrainDeparture {
            train: TrainId(n % 7),
            station: WEST,
            district: D1,
            destination: Some(EAST),
            eta_ticks: 200,
            carriages: vec![],
        }),
    );
}

/// A store's industry pages by key, and one save into it as the host makes it.
type Disk = BTreeMap<u64, Vec<u8>>;

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
    let whole: Disk = e.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    assert!(*disk == whole, "the saves' pages are the engine's whole state");
    if let Some(c) = e.page_check() {
        assert_eq!(c.unmarked, 0, "{:?}", c.first_unmarked);
    }
    (put, deleted)
}

fn restore(disk: &Disk) -> Engine {
    let pages = disk.iter().map(|(k, b)| (PageKey(*k), b.clone()));
    let e = Engine::restore(pages, Tunables::default()).expect("restore");
    let back: Disk = e.snapshot().into_iter().map(|(k, b)| (k.0, b)).collect();
    assert!(back == *disk, "a restored engine saves what it was restored from");
    e
}

fn leg_pages(keys: &[u64]) -> Vec<u64> {
    keys.iter()
        .map(|&k| PageKey(k))
        .filter(|k| k.table() == LEG_TABLE)
        .map(|k| k.id())
        .collect()
}

fn globals_len(disk: &Disk) -> usize {
    disk[&PageKey::new(GLOBAL_TABLE, 0).0].len()
}

/// The legs are in the leg table, one page per [`LEGS_PER_PAGE`] ids, and the globals page does not
/// grow with them.
#[test]
fn legs_are_paged_and_the_globals_page_does_not_grow_with_them() {
    let mut e = engine();
    let mut disk = Disk::new();
    for i in 0..20 {
        depart(&mut e, 100 + i, i);
    }
    save(&mut e, &mut disk);
    let small = globals_len(&disk);
    let n = 2 * LEGS_PER_PAGE + 60;
    for i in 20..n {
        depart(&mut e, 100 + i, i);
    }
    assert_eq!(e.legs().len() as u64, n);
    let (put, deleted) = save(&mut e, &mut disk);
    assert!(deleted.is_empty());
    assert_eq!(leg_pages(&put), vec![0, 1, 2], "three pages hold {n} legs");
    let grown = globals_len(&disk);
    assert!(
        grown <= small + 64,
        "the globals page is {small} bytes with 20 legs and {grown} with {n}"
    );
    let back = restore(&disk);
    assert_eq!(back.legs(), e.legs(), "the legs and both lookups come back as booked");
}

/// A save after a booking writes the page being filled and no other leg page; a quiet save writes
/// the globals page alone.
#[test]
fn a_save_writes_only_the_leg_pages_that_changed() {
    let mut e = engine();
    let mut disk = Disk::new();
    let n = 2 * LEGS_PER_PAGE + 10;
    for i in 0..n {
        depart(&mut e, 100 + i, i);
    }
    save(&mut e, &mut disk);
    let globals = PageKey::new(GLOBAL_TABLE, 0).0;
    assert_eq!(save(&mut e, &mut disk), (vec![globals], vec![]), "nothing changed");
    depart(&mut e, 5_000, n);
    let (put, deleted) = save(&mut e, &mut disk);
    assert!(deleted.is_empty());
    assert_eq!(leg_pages(&put), vec![2], "only the page the new leg went into: {put:?}");
    // Filling the page up and starting the next one writes the two.
    for i in 0..LEGS_PER_PAGE {
        depart(&mut e, 5_001 + i, n + 1 + i);
    }
    let (put, _) = save(&mut e, &mut disk);
    assert_eq!(leg_pages(&put), vec![2, 3]);
    restore(&disk);
}

/// Legs a day past their end are dropped: a page left with none is deleted, one left with some is
/// written again, and the disk still holds what a restore needs.
#[test]
fn expired_legs_delete_and_rewrite_their_pages() {
    let mut e = engine();
    let mut disk = Disk::new();
    let n = 2 * LEGS_PER_PAGE + 10;
    for i in 0..n {
        depart(&mut e, 100 + i, i);
    }
    save(&mut e, &mut disk);
    // Two days on, one more train leaves: the first sweep of the new day drops every old leg.
    depart(&mut e, 2 * TICKS_PER_DAY + 5, n);
    let (put, deleted) = save(&mut e, &mut disk);
    assert_eq!(leg_pages(&deleted), vec![0, 1], "empty pages are deleted: {deleted:?}");
    assert_eq!(leg_pages(&put), vec![2], "the page with the one leg left is written again");
    assert_eq!(e.legs().len(), 1);
    let back = restore(&disk);
    assert_eq!(back.legs(), e.legs());
    // The id counter is a globals field: the next booking does not reuse an expired id.
    let mut back = back;
    depart(&mut back, 2 * TICKS_PER_DAY + 9, n + 1);
    let ids: Vec<u64> = back.legs().iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![n, n + 1]);
}

/// A save that did not reach the disk hands its leg pages back: the next save writes them again.
#[test]
fn a_failed_save_writes_its_leg_pages_again() {
    let mut e = engine();
    let mut disk = Disk::new();
    for i in 0..3 {
        depart(&mut e, 100 + i, i);
    }
    save(&mut e, &mut disk);
    depart(&mut e, 200, 3);
    let lost = e.save_changed();
    assert_eq!(leg_pages(&lost.put.iter().map(|(k, _)| k.0).collect::<Vec<_>>()), vec![0]);
    assert!(!lost.ticket.is_empty());
    e.save_failed(lost.ticket);
    assert!(e.legs().changed().contains(&0), "the lost leg page waits for the next save");
    let (put, _) = save(&mut e, &mut disk);
    assert_eq!(leg_pages(&put), vec![0]);
    restore(&disk);
}

/// A leg page of another format, in another page's key, or past the id counter does not restore.
#[test]
fn a_leg_page_that_does_not_fit_its_key_or_counter_is_refused() {
    let mut e = engine();
    for i in 0..(LEGS_PER_PAGE + 5) {
        depart(&mut e, 100 + i, i);
    }
    let pages = e.legs().pages();
    assert_eq!(pages.len(), 2);
    let next = e.legs().next_id();
    assert_eq!(LegIndex::from_pages(next, pages.clone()).expect("as saved"), *e.legs());
    // Page 1's bytes under page 0's key.
    let swapped = vec![(0, pages[1].1.clone()), (1, pages[0].1.clone())];
    assert!(LegIndex::from_pages(next, swapped).is_err());
    // An id counter behind the legs would reuse their ids.
    assert!(LegIndex::from_pages(LEGS_PER_PAGE, pages.clone()).is_err());
    // Another magic.
    let mut other = pages.clone();
    other[0].1[0] ^= 0xff;
    assert!(LegIndex::from_pages(next, other).is_err());
    // Cut short, at every length.
    for cut in 0..pages[1].1.len() {
        let broken = vec![pages[0].clone(), (1, pages[1].1[..cut].to_vec())];
        assert!(LegIndex::from_pages(next, broken).is_err(), "cut at {cut}");
    }
}
