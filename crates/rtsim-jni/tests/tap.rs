// SPDX-License-Identifier: AGPL-3.0-or-later
//! The view tap behind the engine (VIEWER.md §5.2), on the fake Java side of `common`: nothing
//! without a lease; a lease starts an epoch whose store describes itself; the slack encodes whole
//! units within the budget and the period's commit writes only what changed; a hand-over that held
//! two TICK records stands the tap down; leases end, epochs change and `view/` goes as §5.2 says;
//! the archive's pending rows are one more part.

mod common;

use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering::Acquire;
use std::time::{Duration, Instant, SystemTime};

use common::*;
use rtsim_jni::engine::{Engine, tap};
use rtsim_jni::orders::Order;
use rtsim_kernel::page::{PageKey, PageStore};
use rtsim_parts::mark::kind as row_kind;
use rtsim_parts::part::kind as part_kind;
use rtsim_parts::store::{self as vstore, Header, Lease};
use rtsim_parts::{Layers, RegionKey, read_part};

/// The core's four phase 1 modules.
const PHASE1: u32 = 0b1111;
const VILLAGES: [(u64, i32, i32); 3] = [
    (0x0001_0000_0000_0001, 0, 0),
    (0x0001_0000_0000_0002, 400, 0),
    (0x0001_0000_0000_0003, 0, 1400),
];

fn write_lease(dir: &Path, pid: u32, focus: Vec<RegionKey>) {
    fs::create_dir_all(dir).unwrap();
    Lease { pid, focus }.write(dir).unwrap();
}

/// Moves the lease's modification time `ago` into the past.
fn age_lease(dir: &Path, ago: Duration) {
    let f = fs::File::options()
        .write(true)
        .open(dir.join(vstore::LEASE_FILE))
        .unwrap();
    f.set_modified(SystemTime::now() - ago).unwrap();
}

/// A phase 1 world with three villages; `lease` writes the lease before the first step.
fn world(tag: &str, lease: bool) -> (std::path::PathBuf, std::sync::Arc<Engine>, Java) {
    let dir = tmpdir(tag);
    let e = engine(&dir, 256 * 1024);
    if lease {
        write_lease(&dir, std::process::id(), Vec::new());
    }
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    for (v, x, z) in VILLAGES {
        j.adopt(v, "minecraft:village_plains", "human", 90, x, z);
    }
    j.idle();
    (dir, e, j)
}

/// The newest epoch's store, read only, and its header.
fn store(dir: &Path) -> (u64, Header, PageStore) {
    let epoch = *vstore::epochs(dir).last().expect("an epoch folder");
    let s = PageStore::open_read_only(vstore::epoch_dir(dir, epoch)).unwrap();
    let h = s
        .get_shared(PageKey::new(vstore::HEADER_TABLE, 0))
        .unwrap()
        .expect("a header page");
    (epoch, Header::decode(&h).unwrap(), s)
}

fn layer_index(id: &str) -> usize {
    rtsim_layers::specs()
        .iter()
        .position(|s| s.desc.id == id)
        .unwrap()
}

/// Rows of one layer's mark parts in the header's directory.
fn layer_rows(h: &Header, id: &str) -> u64 {
    let i = layer_index(id) as u16;
    h.units
        .iter()
        .filter(|u| u.key.kind == part_kind::MARKS && u.key.layer == i)
        .flat_map(|u| &u.parts)
        .map(|p| u64::from(p.rows))
        .sum()
}

fn stat(e: &Engine, name: &str) -> u64 {
    e.stats
        .tap
        .snapshot()
        .into_iter()
        .find(|(k, _)| k == name)
        .unwrap()
        .1
}

#[test]
fn without_a_lease_nothing_is_encoded_or_written() {
    let (dir, e, mut j) = world("tap-none", false);
    for _ in 0..450 {
        j.flush();
    }
    assert!(stat(&e, "lease_stats") >= 2, "the lease is looked at");
    for k in ["leased", "steps_tapped", "units", "commits", "stood_down"] {
        assert_eq!(stat(&e, k), 0, "{k}");
    }
    assert!(!dir.join(vstore::VIEW_DIR).exists());
    assert!(!dir.join(vstore::LEASE_FILE).exists());
    assert!(e.view_tap_flush().is_err(), "a flush needs a lease");
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_session_start_deletes_a_stale_view_store() {
    let dir = tmpdir("tap-stale");
    let stale = vstore::epoch_dir(&dir, 7);
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("seg-000001.rts"), b"left by a crash").unwrap();
    let e = engine(&dir, 256 * 1024);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(10);
    while dir.join(vstore::VIEW_DIR).exists() {
        assert!(Instant::now() < deadline, "view/ was never deleted");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(stat(&e, "deletes"), 1);
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_lease_starts_an_epoch_and_the_slack_commits_whole_units() {
    let (dir, e, mut j) = world("tap-slack", true);
    // Steps with slack between them, as a server's 50 ms ticks leave.
    let deadline = Instant::now() + Duration::from_secs(30);
    while stat(&e, "commits") < 2 || stat(&e, "cycles") < 1 {
        assert!(Instant::now() < deadline, "{:?}", e.stats.tap.snapshot());
        j.flush();
        std::thread::sleep(Duration::from_millis(1));
    }
    let (epoch, h, s) = store(&dir);
    assert_eq!(stat(&e, "leased"), 1);
    assert_eq!(epoch, stat(&e, "epoch"));
    assert_eq!(h.epoch, epoch);
    assert_eq!(h.build, tap::BUILD);
    assert!(h.cycles >= 1 && h.commit >= 1);
    assert!(h.game_time > 0);
    // The store describes itself: the layer list is the registry's.
    let layers = Layers::from_json(
        &s.get_shared(PageKey::new(vstore::LAYERS_TABLE, 0))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let ids: Vec<String> = rtsim_layers::specs()
        .iter()
        .map(|s| s.desc.id.clone())
        .collect();
    assert_eq!(
        layers
            .layers
            .iter()
            .map(|l| l.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    assert_eq!(h.layers.len(), ids.len());
    assert_eq!(layer_rows(&h, "settle.villages"), VILLAGES.len() as u64);
    // Every part the directory names is there, whole, with its rows; stay-put parts by region.
    let mut parts = 0;
    for u in &h.units {
        assert!(u.resident, "nothing is evicted in a small world");
        for p in &u.parts {
            let b = s
                .get_shared(PageKey::new(vstore::PART_TABLE, p.page))
                .unwrap()
                .expect("the part's page");
            assert_eq!(crc32fast::hash(&b), p.crc);
            let v = read_part(&b).unwrap();
            assert_eq!(v.rows.len() as u32, p.rows);
            assert_eq!(v.key.unit_key(), u.key);
            assert_eq!(v.key.region, p.region);
            assert!(p.rows <= rtsim_parts::MAX_ROWS);
            parts += 1;
        }
    }
    assert!(parts > 0);
    // The budget: at most one job past it.
    let over = stat(&e, "overrun_nanos_max");
    let job = stat(&e, "job_nanos_max");
    assert!(stat(&e, "steps_tapped") > 0);
    assert!(
        over <= job + 5_000_000,
        "the tap ran {over} ns past its budget; its longest job took {job} ns"
    );
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_flush_encodes_everything_and_unchanged_parts_are_not_rewritten() {
    let (dir, e, mut j) = world("tap-flush", true);
    e.view_tap_flush().unwrap();
    let (_, h1, _) = store(&dir);
    // The steps of the world's setup may have committed already (the lease was there first).
    assert!(h1.commit >= 1);
    assert!(h1.cycles >= 1);
    let villages = layer_index("settle.villages");
    assert_eq!(layer_rows(&h1, "settle.villages"), 3);
    // Nothing changed: every part is dropped as unchanged, no layer moves.
    e.view_tap_flush().unwrap();
    let (_, h2, _) = store(&dir);
    assert_eq!(h2.commit, h1.commit + 1);
    assert_eq!(h2.stat("parts_written_now"), Some(0));
    let versions = |h: &Header| h.layers.iter().map(|l| l.version).collect::<Vec<_>>();
    assert_eq!(versions(&h1), versions(&h2));
    // A fourth village: its layer's version moves.
    j.adopt(
        0x0001_0000_0000_0004,
        "minecraft:village_taiga",
        "human",
        90,
        800,
        800,
    );
    j.idle();
    e.view_tap_flush().unwrap();
    let (_, h3, _) = store(&dir);
    assert_eq!(layer_rows(&h3, "settle.villages"), 4);
    assert!(h3.layers[villages].version > h2.layers[villages].version);
    assert!(h3.stat("parts_written_now").unwrap() >= 1);
    e.close(false, Duration::from_secs(5)).unwrap();
}

/// The encode time of full units (VIEWER.md §5.2 item 2): 4,096 villages fill the villages layer's
/// units (500 keys a unit, so eight full ones and one of 96), each written as one part per region
/// its villages are in, and as many prices and messengers; a flush encodes every unit once and the
/// counters time each, by layer. Prints the numbers (run with `--release --nocapture` for
/// VIEWER.md's).
#[test]
fn full_units_encode_whole_and_their_time_is_measured() {
    let dir = tmpdir("tap-full-units");
    let e = engine(&dir, 1 << 20);
    write_lease(&dir, std::process::id(), Vec::new());
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.module_mask = PHASE1;
    const N: u64 = 4_096;
    for i in 0..N {
        let (x, z) = ((i % 64) as i32 * 200, (i / 64) as i32 * 200);
        j.adopt(
            0x0002_0000_0000_0000 + i,
            "minecraft:village_plains",
            "human",
            90,
            x,
            z,
        );
        if i % 64 == 63 {
            j.idle();
        }
    }
    j.idle();
    e.view_tap_flush().unwrap();
    e.stats.tap.reset_measurements();
    // A second flush, every unit warm: its counters are the encode alone.
    e.view_tap_flush().unwrap();
    let (_, h, s) = store(&dir);
    assert_eq!(layer_rows(&h, "settle.villages"), N);
    let villages = layer_index("settle.villages");
    let per = rtsim_layers::specs()[villages].per_unit as u32;
    let units: Vec<_> = h
        .units
        .iter()
        .filter(|u| u.key.kind == part_kind::MARKS && usize::from(u.key.layer) == villages)
        .collect();
    let rows = |u: &vstore::UnitEntry| u.parts.iter().map(|p| p.rows).sum::<u32>();
    let mut sizes: Vec<u32> = units.iter().map(|u| rows(u)).collect();
    sizes.sort_unstable();
    let mut want = vec![per; (N / u64::from(per)) as usize];
    want.insert(0, (N % u64::from(per)) as u32);
    assert_eq!(sizes, want, "the villages in whole units");
    let full_bytes: Vec<usize> = units
        .iter()
        .filter(|u| rows(u) == per)
        .map(|u| {
            u.parts
                .iter()
                .map(|p| {
                    s.get_shared(PageKey::new(vstore::PART_TABLE, p.page))
                        .unwrap()
                        .unwrap()
                        .len()
                })
                .sum()
        })
        .collect();
    let g = |k: &str| stat(&e, k);
    let n = g("units").max(1);
    println!(
        "full units: {} units, {} rows, {} bytes; {:.3} ms a unit on average, longest {:.3} ms \
         (a unit of {per} village rows, {:?} bytes each); {:.0} rows a ms, {:.0} MB/s",
        g("units"),
        g("rows"),
        g("bytes"),
        g("unit_nanos") as f64 / 1e6 / n as f64,
        g("unit_nanos_max") as f64 / 1e6,
        full_bytes,
        g("rows") as f64 / (g("unit_nanos").max(1) as f64 / 1e6),
        g("bytes") as f64 / (g("unit_nanos").max(1) as f64 / 1e9) / 1e6,
    );
    // By layer: every layer with rows has its cost in the header, by id.
    let snap = e.stats.tap.snapshot();
    let mut by_layer = 0;
    for (k, v) in &snap {
        if let Some(id) = k.strip_prefix("unit_nanos_max.") {
            let get = |p: &str| {
                snap.iter()
                    .find(|(k, _)| *k == format!("{p}.{id}"))
                    .unwrap()
                    .1
            };
            println!(
                "full units: {id}: {} rows a unit, {} ns a row, longest unit {:.3} ms",
                get("rows_per_unit"),
                get("nanos_per_row"),
                *v as f64 / 1e6
            );
            by_layer += 1;
        }
    }
    assert!(by_layer >= 3, "the layers' costs: {snap:?}");
    assert!(h.stat("unit_nanos_max.settle.villages").is_some());
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_hand_over_holding_two_ticks_stands_the_tap_down() {
    let (_dir, e, mut j) = world("tap-behind", true);
    e.view_tap_flush().unwrap();
    // Let the after-steps of the flush's steps finish.
    std::thread::sleep(Duration::from_millis(20));
    e.stats.tap.reset_measurements();
    // Java ticked twice before it could hand over: two TICK records in one hand-over.
    j.apply_commands();
    assert!(j.preamble());
    j.game_time += 1;
    j.flush();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(stat(&e, "behind"), 1, "{:?}", e.stats.tap.snapshot());
    assert_eq!(stat(&e, "stood_down"), 1);
    assert_eq!(stat(&e, "steps_tapped"), 0);
    // One TICK: the tap encodes again.
    j.flush();
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(stat(&e, "behind"), 1);
    assert_eq!(stat(&e, "steps_tapped"), 1, "{:?}", e.stats.tap.snapshot());
    e.close(false, Duration::from_secs(5)).unwrap();
}

/// An epoch's first commit can come at a step the tap stood down (it encoded nothing): it still
/// carries the state's time, never the empty mailbox's zero.
#[test]
fn a_commit_at_a_step_the_tap_stood_down_carries_the_states_time() {
    let (dir, e, mut j) = world("tap-stood-down-time", false);
    write_lease(&dir, std::process::id(), Vec::new());
    let from = j.game_time;
    // Every hand-over holds two TICK records, so every leased step stands down; the next lease
    // stat (at most STAT_EVERY steps on) starts the epoch, and its first step commits.
    let mut steps = 0;
    while stat(&e, "commits") == 0 {
        assert!(steps < 3 * tap::STAT_EVERY, "{:?}", e.stats.tap.snapshot());
        j.apply_commands();
        assert!(j.preamble());
        j.game_time += 1;
        j.flush();
        steps += 1;
    }
    // The counter moves once the commit is on disk.
    let (_, h, _) = store(&dir);
    assert_eq!(stat(&e, "steps_tapped"), 0, "{:?}", e.stats.tap.snapshot());
    assert!(stat(&e, "stood_down") > 0);
    assert!(
        h.game_time > from && h.game_time <= j.game_time,
        "the header's time {} is the state's (Java at {}, from {from})",
        h.game_time,
        j.game_time
    );
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn leases_end_epochs_change_and_view_goes_a_minute_after() {
    let (dir, e, _j) = world("tap-epochs", true);
    e.view_tap_flush().unwrap();
    let first = stat(&e, "epoch");
    assert!(vstore::epoch_dir(&dir, first).exists());
    // Another sidecar (another process id) is another lease: a new epoch, the old one deleted.
    write_lease(&dir, std::process::id() + 1, Vec::new());
    e.view_tap_flush().unwrap();
    let second = stat(&e, "epoch");
    assert!(second > first);
    assert_eq!(vstore::epochs(&dir), vec![second]);
    // Not renewed for 40 s: over (30 s), but only 10 s ago, so view/ stays.
    age_lease(&dir, Duration::from_secs(40));
    assert!(e.view_tap_flush().is_err());
    assert_eq!(stat(&e, "leased"), 0);
    assert!(vstore::epoch_dir(&dir, second).exists());
    // Renewed: a new epoch.
    write_lease(&dir, std::process::id(), Vec::new());
    e.view_tap_flush().unwrap();
    let third = stat(&e, "epoch");
    assert!(third > second);
    assert_eq!(vstore::epochs(&dir), vec![third]);
    // Last renewed 100 s ago: over 70 s ago, past the minute, so view/ goes.
    age_lease(&dir, Duration::from_secs(100));
    assert!(e.view_tap_flush().is_err());
    assert!(!dir.join(vstore::VIEW_DIR).exists());
    assert!(stat(&e, "deletes") >= 1);
    // The lease file is the sidecar's: the engine never deletes it.
    assert!(dir.join(vstore::LEASE_FILE).exists());
    e.close(false, Duration::from_secs(5)).unwrap();
}

#[test]
fn the_archives_pending_rows_are_one_more_part() {
    let (dir, e, mut j) = world("tap-archive", true);
    let pending = e.stats.archive_pending_rows.load(Acquire);
    assert!(pending > 0, "adoptions record events");
    e.view_tap_flush().unwrap();
    let (_, h, s) = store(&dir);
    let archive: Vec<_> = h
        .units
        .iter()
        .filter(|u| u.key.kind == part_kind::ARCHIVE_ROWS)
        .collect();
    let (mut rows, mut names) = (0u64, 0u64);
    for u in &archive {
        for p in &u.parts {
            let b = s
                .get_shared(PageKey::new(vstore::PART_TABLE, p.page))
                .unwrap()
                .unwrap();
            for r in read_part(&b).unwrap().rows {
                assert!(r.mark.is_none());
                match r.kind {
                    row_kind::ARCHIVE_ROW => {
                        assert_eq!(r.body.len(), 32, "a raw archive row");
                        rows += 1;
                    }
                    row_kind::ARCHIVE_NAME => names += 1,
                    k => panic!("row kind {k} in an archive part"),
                }
            }
        }
    }
    assert_eq!(rows, pending);
    assert!(names > 0, "the kinds and frames are named");
    assert_eq!(
        h.stat("archive_serial"),
        Some(e.stats.archive_serial.load(Acquire))
    );
    // A save takes the rows into the archive: the row parts go, the serial moves.
    let serial = e.stats.archive_serial.load(Acquire);
    let saves = e.stats.saves_done.load(Acquire);
    e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.idle();
    let deadline = Instant::now() + Duration::from_secs(20);
    while e.stats.saves_done.load(Acquire) == saves {
        assert!(Instant::now() < deadline, "the save never committed");
        std::thread::sleep(Duration::from_millis(2));
    }
    e.view_tap_flush().unwrap();
    let (_, h, _) = store(&dir);
    let left: u64 = h
        .units
        .iter()
        .filter(|u| u.key.kind == part_kind::ARCHIVE_ROWS && u.key.unit != tap::ARCHIVE_NAMES)
        .flat_map(|u| &u.parts)
        .map(|p| u64::from(p.rows))
        .sum();
    assert_eq!(left, e.stats.archive_pending_rows.load(Acquire));
    assert_ne!(h.stat("archive_serial"), Some(serial));
    e.close(false, Duration::from_secs(5)).unwrap();
}
