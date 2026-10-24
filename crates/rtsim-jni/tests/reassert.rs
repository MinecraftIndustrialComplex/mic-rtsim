// SPDX-License-Identifier: AGPL-3.0-or-later
//! Copies of carried records (REASSERT, docs/boundary.md §3.2) against the fake Java side, on the
//! real host: a chunk or entity keeps a record from the session that journaled it, and journals it
//! again whenever it loads before the engine acknowledged the copy. The engine applies a copy only
//! when its own state lacks the record: never twice, and never not at all after a crash lost it.
//!
//! The records are TRANSFERs into ledger stores: one without an op id (a record only its sequence
//! names) and one with a Java op id, which a durable world save makes the engine forget.

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::*;
use rtsim_jni::engine::Engine;
use rtsim_jni::orders::Order;
use rtsim_jni::wire::{self, obs};

fn ledger(e: &Engine, store: u64) -> i64 {
    e.inspect().ledger.get(&store).copied().unwrap_or(0)
}

/// What a holder keeps: the generation the record was journaled in and its bytes as journaled.
#[derive(Clone)]
struct Held {
    nonce: u64,
    bytes: Vec<u8>,
}

/// Journals an op-less transfer of `amount` into store `to` and returns it as a holder keeps it.
fn carried(j: &mut Java, to: u64, amount: i64) -> Held {
    let seq = j.transfer(0, 1, to, amount);
    Held {
        nonce: j.nonce,
        bytes: j.journaled(seq).unwrap(),
    }
}

fn reassert(j: &mut Java, h: &Held) -> u64 {
    j.reassert(h.nonce, &h.bytes)
}

fn copies(e: &Engine) -> (u64, u64) {
    (
        e.stats.copies_applied.load(Ordering::Acquire),
        e.stats.copies_dropped.load(Ordering::Acquire),
    )
}

/// A world save on disk through `durable` (WORLD_SAVED): the engine forgets the op ids up to it.
fn world_saved(j: &mut Java, durable: u64) {
    let r = j
        .reserve(obs::WORLD_SAVED, obs::world_saved::SIZE, 0)
        .expect("room");
    wire::put_u64(r, obs::world_saved::DURABLE_OBS_SEQ, durable);
}

/// The engine's pages on disk through everything it has read.
fn save_pages(j: &mut Java) {
    let through = j.e.inspect().replay.applied_through().0;
    j.e.order(vec![Order::SaveAtNextStep]).unwrap();
    j.run_until("pages on disk", |j| j.e.durable_obs_seq() >= through);
}

/// A clean stop: the world saves its journal, the engine its pages.
fn stop(j: Java) -> WorldSave {
    let saved = j.world_save();
    j.e.close(true, Duration::from_secs(5)).unwrap();
    saved
}

fn start(dir: &PathBuf, saved: WorldSave) -> Java {
    let e = engine(dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e, saved);
    j.idle();
    j
}

fn no_error(j: &Java) {
    assert!(j.e.protocol_error().is_none(), "{:?}", j.e.protocol_error());
}

/// The ack-lag window: a holder unloads and loads again before the engine acknowledged its
/// records, after a world save made the engine forget the op id. Neither copy applies again.
#[test]
fn a_copy_reasserted_before_the_ack_never_applies_twice() {
    let dir = tmpdir("reassert-acklag");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    j.hold_acks = true; // the holder keeps its records until an acknowledgement it never gets
    let opless = carried(&mut j, 8, 2);
    let seq = j.transfer(wire::JAVA_OP_BIT | 0x51, 1, 7, 5);
    let with_op = Held {
        nonce: j.nonce,
        bytes: j.journaled(seq).unwrap(),
    };
    j.idle();
    assert_eq!((ledger(&e, 8), ledger(&e, 7)), (2, 5));
    let durable = j.next_obs - 1;
    world_saved(&mut j, durable);
    j.idle();
    assert!(
        !e.inspect()
            .replay
            .knows_op(rtsim_kernel::journal::OpId(wire::JAVA_OP_BIT | 0x51)),
        "the world save made the engine forget the op id"
    );
    for _ in 0..2 {
        reassert(&mut j, &opless);
        reassert(&mut j, &with_op);
        j.idle();
    }
    no_error(&j);
    assert_eq!((ledger(&e, 8), ledger(&e, 7)), (2, 5), "applied once each");
    assert_eq!(copies(&e), (0, 4));
    let r = j.received();
    assert_eq!(
        r.iter()
            .filter(|r| wire::u16_at(&r.bytes, wire::R_KIND) == obs::REASSERT && r.skipped)
            .count(),
        4,
        "each copy is logged as received and dropped"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

/// A crash lost the journal the record was in (the holder's chunk was saved on unload, the
/// journal not): the copy is all that is left of it, and it applies once. `gen_saved`: the
/// world save before the crash holds the session's JOURNAL_GEN (the pages never did); otherwise
/// nothing of that session reached any disk but the holder, and its generation is unknown.
fn crash_with_an_unsaved_journal(gen_saved: bool) {
    let dir = tmpdir(if gen_saved {
        "reassert-lost"
    } else {
        "reassert-lost-gen"
    });
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let before = if gen_saved {
        j.world_save()
    } else {
        WorldSave::fresh()
    };
    let held = carried(&mut j, 8, 2);
    j.idle();
    assert_eq!(ledger(&e, 8), 2);
    let _ = j.crash();

    let mut j = start(&dir, before);
    assert_eq!(ledger(&j.e, 8), 0, "the crash lost it with the journal");
    // The holder loads, unloads before the acknowledgement and loads again.
    reassert(&mut j, &held);
    j.idle();
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    assert_eq!(ledger(&j.e, 8), 2, "the copy applied once");
    assert_eq!(copies(&j.e), (1, 1));
    // Once more after the pages took the copy and the server stopped.
    save_pages(&mut j);
    let saved = stop(j);
    let mut j = start(&dir, saved);
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    assert_eq!(ledger(&j.e, 8), 2, "the pages remember the copy applied");
    assert_eq!(copies(&j.e), (0, 1));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_copy_of_a_record_a_crash_lost_applies_once() {
    crash_with_an_unsaved_journal(true);
}

#[test]
fn a_copy_from_a_generation_no_disk_kept_applies_once() {
    crash_with_an_unsaved_journal(false);
}

/// A sim reset between the unload and the load: the pages are gone, so the engine knows the
/// generation only from the JOURNAL_GEN the journal kept past the trim, which admitted nothing
/// after it, and the copy applies (the reinstall guard's RG14 drops such records in Java before
/// they are journaled; the engine on its own takes the world's word).
#[test]
fn after_a_reset_an_unknown_copy_applies() {
    let dir = tmpdir("reassert-reset");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let held = carried(&mut j, 8, 2);
    j.idle();
    save_pages(&mut j);
    // The holder's chunk unloaded before the acknowledgement trimmed the journal, so it keeps
    // its copy; the world's journal does not.
    let through = j.e.inspect().replay.applied_through().0;
    j.run_until("the acknowledgement", |j| {
        j.journal.front().is_none_or(|(s, _)| *s > through)
    });
    let saved = stop(j);
    std::fs::remove_dir_all(&dir).unwrap();
    let mut j = start(&dir, saved);
    assert_eq!(ledger(&j.e, 8), 0, "a fresh engine, and nothing to replay");
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    assert_eq!(ledger(&j.e, 8), 2);
    assert_eq!(copies(&j.e), (1, 0));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// Pages older than the journal's first entry (restored from a backup, say: a fresh folder stands
/// for them), after an acknowledgement trimmed the session's JOURNAL_GEN. The journal keeps that
/// JOURNAL_GEN past the trim, so the engine admits the entries after it in their own generation,
/// and a copy of one of them is dropped: the replay applied it. A copy from the trimmed stretch is
/// dropped too; that stretch is what such pages lose (docs/boundary.md §3.2, Known limit).
#[test]
fn a_copy_of_a_record_replayed_into_older_pages_never_applies_twice() {
    let dir = tmpdir("reassert-older-pages");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let gen_seq = j.journal.front().unwrap().0;
    let trimmed = carried(&mut j, 8, 2);
    j.idle();
    save_pages(&mut j);
    let through = j.e.inspect().replay.applied_through().0;
    j.run_until("the acknowledgement", |j| {
        j.journal.front().is_none_or(|(s, _)| *s > through)
    });
    assert_eq!(
        j.pinned_gen.as_ref().map(|(s, _)| *s),
        Some(gen_seq),
        "the session's JOURNAL_GEN outlives the trim"
    );
    j.hold_acks = true;
    let replayed = carried(&mut j, 7, 5);
    j.idle();
    let saved = stop(j);
    assert!(saved.pinned_gen.is_some(), "and the world save keeps it");
    std::fs::remove_dir_all(&dir).unwrap();

    let mut j = start(&dir, saved);
    assert_eq!(
        (ledger(&j.e, 8), ledger(&j.e, 7)),
        (0, 5),
        "the trimmed stretch is lost, the rest replayed"
    );
    reassert(&mut j, &replayed);
    reassert(&mut j, &trimmed);
    j.idle();
    no_error(&j);
    assert_eq!(
        (ledger(&j.e, 8), ledger(&j.e, 7)),
        (0, 5),
        "the replayed record applied once"
    );
    assert_eq!(copies(&j.e), (0, 2));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

/// A holder unloaded across two later sessions before any acknowledgement reached it: its copy
/// is judged by the session that first journaled the record each time. `copy_saved`: the
/// session that applied the copy saved its pages before it crashed.
fn twice_across_sessions(copy_saved: bool) {
    let dir = tmpdir(if copy_saved {
        "reassert-twice-saved"
    } else {
        "reassert-twice"
    });
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let before = j.world_save();
    let held = carried(&mut j, 8, 2);
    // Applied and on the pages, but the world save that holds the record never came.
    j.idle();
    save_pages(&mut j);
    let _ = j.crash();

    // Session 2: the replay starts past the record; the copy finds it in the pages.
    let mut j = start(&dir, before);
    assert_eq!(ledger(&j.e, 8), 2, "on the pages");
    reassert(&mut j, &held);
    j.idle();
    assert_eq!(ledger(&j.e, 8), 2);
    assert_eq!(copies(&j.e), (0, 1));
    let saved = stop(j);

    // Session 3: pages older than session 1 (a fresh folder stands for them). Session 2's copy,
    // still in the world's journal, applies now; this session's own copy is the same record.
    std::fs::remove_dir_all(&dir).unwrap();
    let mut j = start(&dir, saved);
    reassert(&mut j, &held);
    j.idle();
    assert_eq!(ledger(&j.e, 8), 2, "applied once from the copies");
    assert_eq!(copies(&j.e), (1, 1));
    let w = j.world_save();
    if copy_saved {
        save_pages(&mut j);
    }
    let _ = j.crash();

    let mut j = start(&dir, w);
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    if copy_saved {
        assert_eq!(ledger(&j.e, 8), 2, "the pages hold the copy");
        assert_eq!(copies(&j.e), (0, 1));
    } else {
        // Session 3's pages never reached the disk: the world's journal replays sessions 2's
        // and 3's copies (unacknowledged) before this one, and the first of them applies.
        assert_eq!(
            ledger(&j.e, 8),
            2,
            "the first replayed copy, the others dropped"
        );
        assert_eq!(copies(&j.e), (1, 2));
    }
    j.e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_holder_unloaded_across_two_sessions_applies_once() {
    twice_across_sessions(false);
}

#[test]
fn a_holder_unloaded_across_two_sessions_applies_once_when_the_copy_was_saved() {
    twice_across_sessions(true);
}

/// Nothing falls off: a holder that stays unloaded for more sessions than a block of closed
/// generations holds (a rarely visited base chunk on a server that restarts daily) is still
/// judged by the session that journaled its record.
#[test]
fn a_copy_from_a_generation_many_sessions_old_is_dropped() {
    let dir = tmpdir("reassert-old");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let held = carried(&mut j, 8, 2);
    j.idle();
    save_pages(&mut j);
    let mut saved = stop(j);
    let sessions = rtsim_kernel::journal::GEN_BLOCK + 8;
    let t0 = Instant::now();
    for _ in 0..sessions {
        let j = start(&dir, saved);
        saved = stop(j);
    }
    eprintln!("{sessions} sessions in {:?}", t0.elapsed());
    let mut j = start(&dir, saved);
    assert!(j.e.inspect().replay.closed_len() > rtsim_kernel::journal::GEN_BLOCK);
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    assert_eq!(ledger(&j.e, 8), 2, "applied once, long ago");
    assert_eq!(copies(&j.e), (0, 1));
    j.e.close(true, Duration::from_secs(5)).unwrap();
}
