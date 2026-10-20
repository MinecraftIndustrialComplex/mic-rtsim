// SPDX-License-Identifier: AGPL-3.0-or-later
//! `rtsim-cli bench restore`: what a start costs with the phase 1 world's snapshot
//! (docs/overnight-choices-persist.md S1 to S7) against rebuilding it from its whole input log
//! (choice V6, what every start did before).
//!
//! The world is the scale bench's grid of villages, joined one at a time at tick 0 (as the bridge's
//! adoptions join them, C22) and run for the given days. Measured on this thread:
//! - **replay**: an empty world, every join, then the days: what a start with the whole log did;
//! - **save**: `World::save`, then the page encoding (LZ4 and CRC32) of its 1 MiB pieces;
//! - **restore**: the pieces' decoding and `World::load` (median of the repeats), and the same
//!   through a page store on disk (commit, reopen, read);
//! - **tail**: one more world day after the restore (the longest a tail spans, S3).
//!
//! Every restore is checked: the same state hash, and the same hash one day later.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rtsim_kernel::clock::Tick;
use rtsim_kernel::page::{Durability, EncodedPage, PageKey, PageStore, SaveBatch};
use rtsim_trade::build::Genesis;
use rtsim_world::{World, WorldConfig, synth};
use serde::Serialize;

use crate::sys;

/// The phase 1 link's piece size (rtsim-jni `phase1::PIECE`).
const PIECE: usize = 1 << 20;

#[derive(Serialize)]
struct Report {
    utc: String,
    git_commit: Option<String>,
    git_dirty: Option<bool>,
    rustc: Option<String>,
    build_profile: String,
    machine: sys::Machine,
    nice: Option<i32>,
    loadavg_before: Option<String>,
    loadavg_after: Option<String>,
    markets: usize,
    edges: usize,
    days: u64,
    caravan_rows: usize,
    packets: usize,
    repeats: usize,
    /// Rebuilding from the whole input log (V6): joins, then the days.
    replay_joins_ms: f64,
    replay_days_ms: f64,
    replay_total_ms: f64,
    /// The snapshot.
    save_encode_ms: f64,
    save_raw_bytes: usize,
    save_pieces: usize,
    save_lz4_ms: f64,
    save_stored_bytes: usize,
    /// Reading it back, in memory: LZ4 and CRC32, then `World::load` (median, min, max).
    restore_decompress_ms: f64,
    restore_load_ms: f64,
    restore_total_ms: f64,
    restore_total_min_ms: f64,
    restore_total_max_ms: f64,
    /// Through a page store on disk (no fsync): commit, then reopen and read every piece.
    store_commit_ms: f64,
    store_open_read_ms: f64,
    /// One world day run after the restore (the longest tail, S3).
    tail_day_ms: f64,
    heap_world_bytes: usize,
    heap_restored_bytes: usize,
    hash: String,
    hash_matches: bool,
    hash_matches_a_day_later: bool,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1e3
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn cfg() -> WorldConfig {
    let mut cfg = WorldConfig::new(42);
    cfg.check_every_event = false;
    cfg.genesis = Genesis::Share(500);
    cfg
}

fn parts() -> (WorldConfig, rtsim_trade::Goods, rtsim_trade::labour::RecipeBook) {
    let (goods, ids) = synth::goods();
    (cfg(), goods, synth::recipes(&ids))
}

pub fn cli(rest: &[&str]) -> ExitCode {
    let (mut markets, mut days, mut repeats) = (1_024usize, 100u64, 3usize);
    let mut out: Option<PathBuf> = Some(PathBuf::from("bench/results"));
    let mut it = rest.iter();
    while let Some(&a) = it.next() {
        let num = |v: Option<&&str>| v.and_then(|n| n.parse::<u64>().ok());
        match a {
            "--markets" => match num(it.next()) {
                Some(n) if n > 0 => markets = n as usize,
                _ => return usage("--markets needs a positive number"),
            },
            "--days" => match num(it.next()) {
                Some(n) => days = n,
                None => return usage("--days needs a number"),
            },
            "--repeats" => match num(it.next()) {
                Some(n) if n > 0 => repeats = n as usize,
                _ => return usage("--repeats needs a positive number"),
            },
            "--out" => match it.next() {
                Some(d) => out = Some(PathBuf::from(d)),
                None => return usage("--out needs a folder"),
            },
            "--no-write" => out = None,
            other => return usage(&format!("unknown option {other}")),
        }
    }
    ExitCode::from(run(markets, days, repeats, out) as u8)
}

fn usage(why: &str) -> ExitCode {
    eprintln!(
        "{why}\nusage: rtsim-cli bench restore [--markets N] [--days D] [--repeats R] [--out DIR | --no-write]"
    );
    ExitCode::from(64)
}

fn run(markets: usize, days: u64, repeats: usize, out: Option<PathBuf>) -> i32 {
    let loadavg_before = sys::loadavg();
    let (cfg0, goods, book) = parts();
    let heap0 = sys::heap_live();

    // What a start did with the whole input log: every join, then every day since.
    let t = Instant::now();
    let mut w = World::build(cfg0, goods, book, Vec::new());
    for d in synth::grid(markets, 42) {
        w.join(d, Tick(0));
    }
    let replay_joins_ms = ms(t);
    let t = Instant::now();
    w.run_until(Tick::from_days(days));
    let replay_days_ms = ms(t);
    let _ = w.take_commands();
    let heap_world_bytes = sys::heap_live().saturating_sub(heap0);
    let hash = w.state_hash();
    println!(
        "a grid of {} villages and {} edges after {days} days: replay {:.0} ms (joins {:.0}, days {:.0})",
        w.nodes.len(),
        w.edges.len(),
        replay_joins_ms + replay_days_ms,
        replay_joins_ms,
        replay_days_ms
    );

    // The snapshot.
    let t = Instant::now();
    let raw = w.save();
    let save_encode_ms = ms(t);
    let t = Instant::now();
    let pieces: Vec<EncodedPage> = raw.chunks(PIECE).map(EncodedPage::encode).collect();
    let save_lz4_ms = ms(t);
    let save_stored_bytes: usize = pieces.iter().map(EncodedPage::stored_len).sum();
    println!(
        "save: {} bytes raw in {} pieces, {} stored (LZ4), encode {:.1} ms, LZ4 and CRC32 {:.1} ms",
        raw.len(),
        pieces.len(),
        save_stored_bytes,
        save_encode_ms,
        save_lz4_ms
    );

    // Restores in memory.
    let (mut dec, mut load, mut total) = (Vec::new(), Vec::new(), Vec::new());
    let mut hash_matches = true;
    let mut heap_restored_bytes = 0;
    let mut restored = None;
    for _ in 0..repeats {
        let h0 = sys::heap_live();
        let t0 = Instant::now();
        let mut blob = Vec::with_capacity(raw.len());
        for p in &pieces {
            blob.extend_from_slice(&p.decode().expect("the piece decodes"));
        }
        let t1 = Instant::now();
        let (cfg, goods, book) = parts();
        let back = World::load(cfg, goods, book, &blob).expect("the snapshot loads");
        let (d, l) = (
            (t1 - t0).as_secs_f64() * 1e3,
            t1.elapsed().as_secs_f64() * 1e3,
        );
        drop(blob);
        heap_restored_bytes = sys::heap_live().saturating_sub(h0);
        dec.push(d);
        load.push(l);
        total.push(d + l);
        hash_matches &= back.state_hash() == hash;
        restored = Some(back);
    }
    let mut restored = restored.expect("at least one repeat");
    let (tmin, tmax) = total
        .iter()
        .fold((f64::MAX, 0f64), |a, &x| (a.0.min(x), a.1.max(x)));
    let (restore_decompress_ms, restore_load_ms, restore_total_ms) =
        (median(&mut dec), median(&mut load), median(&mut total));
    println!(
        "restore: {restore_total_ms:.1} ms (decompress {restore_decompress_ms:.1}, load {restore_load_ms:.1}; min {tmin:.1}, max {tmax:.1}), hash {}",
        if hash_matches { "matches" } else { "DIFFERS" }
    );

    // Through a page store on disk.
    let dir = std::env::temp_dir().join(format!("rtsim-bench-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let t = Instant::now();
    let mut store = PageStore::open(&dir, Durability::NoSync).expect("a page store");
    let mut batch = SaveBatch::new();
    for (k, p) in pieces.iter().enumerate() {
        batch.put_encoded(PageKey::new(0x5001, k as u64), p.clone());
    }
    store.commit(batch, 1).expect("the commit");
    drop(store);
    let store_commit_ms = ms(t);
    let t = Instant::now();
    let mut store = PageStore::open(&dir, Durability::NoSync).expect("the store reopens");
    let mut blob = Vec::with_capacity(raw.len());
    for k in 0..pieces.len() {
        blob.extend_from_slice(
            &store
                .get(PageKey::new(0x5001, k as u64))
                .expect("the page reads")
                .expect("the page is there"),
        );
    }
    let (cfg, goods, book) = parts();
    let from_disk = World::load(cfg, goods, book, &blob).expect("the snapshot loads");
    let store_open_read_ms = ms(t);
    hash_matches &= from_disk.state_hash() == hash;
    drop(from_disk);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
    println!(
        "page store on disk: commit {store_commit_ms:.1} ms, reopen, read and load {store_open_read_ms:.1} ms"
    );

    // The longest tail: one more world day.
    let t = Instant::now();
    restored.run_until(Tick::from_days(days + 1));
    let tail_day_ms = ms(t);
    w.run_until(Tick::from_days(days + 1));
    let hash_matches_a_day_later = restored.state_hash() == w.state_hash();
    println!(
        "one world day after the restore: {tail_day_ms:.1} ms; hash a day later {}",
        if hash_matches_a_day_later {
            "matches"
        } else {
            "DIFFERS"
        }
    );

    let (utc, stamp) = super::scale::now_utc();
    let report = Report {
        utc,
        git_commit: super::scale::command_line("git", &["rev-parse", "--short=12", "HEAD"]),
        git_dirty: super::scale::command_line(
            "git",
            &["status", "--porcelain", "--untracked-files=no"],
        )
        .map(|s| !s.is_empty()),
        rustc: super::scale::command_line("rustc", &["--version"]),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        machine: sys::Machine::probe(),
        nice: super::scale::nice_level(),
        loadavg_before,
        loadavg_after: sys::loadavg(),
        markets: w.nodes.len(),
        edges: w.edges.len(),
        days,
        caravan_rows: w.caravans.len(),
        packets: w.packets.len(),
        repeats,
        replay_joins_ms,
        replay_days_ms,
        replay_total_ms: replay_joins_ms + replay_days_ms,
        save_encode_ms,
        save_raw_bytes: raw.len(),
        save_pieces: pieces.len(),
        save_lz4_ms,
        save_stored_bytes,
        restore_decompress_ms,
        restore_load_ms,
        restore_total_ms,
        restore_total_min_ms: tmin,
        restore_total_max_ms: tmax,
        store_commit_ms,
        store_open_read_ms,
        tail_day_ms,
        heap_world_bytes,
        heap_restored_bytes,
        hash: format!("{hash:016x}"),
        hash_matches,
        hash_matches_a_day_later,
    };
    if let Some(dir) = out {
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("restore-{stamp}-{markets}m-{days}d.json"));
        match serde_json::to_string_pretty(&report) {
            Ok(s) => match std::fs::write(&path, s + "\n") {
                Ok(()) => println!("wrote {}", path.display()),
                Err(e) => eprintln!("cannot write {}: {e}", path.display()),
            },
            Err(e) => eprintln!("cannot encode the report: {e}"),
        }
    }
    if hash_matches && hash_matches_a_day_later {
        0
    } else {
        1
    }
}
