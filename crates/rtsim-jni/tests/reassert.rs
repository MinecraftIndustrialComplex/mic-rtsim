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

/// A copy of the engine's folder (its pages), as a backup takes it.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

/// Pages restored from a backup a session older than the kept JOURNAL_GEN: the replay skips to
/// it, it closes the pages' own generation where the pages left it, and every copy is judged
/// against what those pages hold. Session A: records 2 to 6, the backup taken with the pages at
/// 3. Session B: records 8 to 11, a trim through 9 (the kept JOURNAL_GEN is B's).
#[test]
fn copies_against_pages_restored_from_an_older_backup() {
    let dir = tmpdir("reassert-backup");
    let backup = tmpdir("reassert-backup-copy");
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let mut held = std::collections::HashMap::new();
    for s in 2..=3 {
        held.insert(s, carried(&mut j, s, 1));
    }
    j.idle();
    save_pages(&mut j);
    assert_eq!(j.e.durable_obs_seq(), 3);
    for s in 4..=6 {
        held.insert(s, carried(&mut j, s, 1));
    }
    j.idle();
    let saved = j.world_save();
    let _ = j.crash(); // the pages stay at 3
    copy_dir(&dir, &backup);

    let mut j = start(&dir, saved);
    assert_eq!(j.journal.front().unwrap().0, 4, "the trim kept 4 to 6");
    assert_eq!(j.next_obs, 8, "7 is session B's JOURNAL_GEN");
    for s in 8..=9 {
        held.insert(s, carried(&mut j, s, 1));
    }
    j.idle();
    save_pages(&mut j);
    j.run_until("the acknowledgement", |j| {
        j.journal.front().is_none_or(|(s, _)| *s > 9)
    });
    assert_eq!(j.pinned_gen.as_ref().map(|(s, _)| *s), Some(7));
    j.hold_acks = true;
    for s in 10..=11 {
        held.insert(s, carried(&mut j, s, 1));
    }
    j.idle();
    let saved = stop(j);
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::rename(&backup, &dir).unwrap();

    let mut j = start(&dir, saved);
    let ledgers = |j: &Java| (2..=11).map(|s| ledger(&j.e, s)).collect::<Vec<_>>();
    assert_eq!(
        ledgers(&j),
        [1, 1, 0, 0, 0, 0, 0, 0, 1, 1],
        "the backup's 2 and 3, the replayed 10 and 11"
    );
    for s in [5, 10, 8, 2] {
        reassert(&mut j, &held[&s]);
    }
    j.idle();
    no_error(&j);
    assert_eq!(
        ledgers(&j),
        [1, 1, 0, 1, 0, 0, 0, 0, 1, 1],
        "A's 5 (past the backup's 3) applies; B's replayed 10, B's trimmed 8 and A's 2 do not"
    );
    assert_eq!(copies(&j.e), (1, 3));
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

/// The replay's end and the journal's kept JOURNAL_GEN (docs/replay-gate.md §2.5 test 4, §2.1's
/// pinnedGen): two restarts in a row, each over a journal trimmed past its session's JOURNAL_GEN
/// and a tail the pages lack. Each session hands over the kept JOURNAL_GEN first (the pages
/// include it: `stale_skipped` rises by one a session), then the tail, which is replayed, then
/// its own JOURNAL_GEN, at which the replay ends (the end runs before S4p's JOURNAL_GEN branch):
/// a record journaled after it in the same hand-over is live, not replayed. No module can tell
/// the end right before the JOURNAL_GEN from right after it (a JOURNAL_GEN is never dispatched);
/// what the counts show is the end after the whole tail and before the live record, inside the
/// first hand-over (without the end at the record, the step's top ends it: 4 records, not 3).
#[test]
fn the_replay_ends_right_before_each_sessions_journal_gen() {
    let dir = tmpdir("reassert-pinned-replay");
    let replay = |e: &Engine| {
        (
            e.stats.replays_ended.load(Ordering::Acquire),
            e.stats.replay_handovers.load(Ordering::Acquire),
            e.stats.replay_records.load(Ordering::Acquire),
        )
    };
    let tail = 3;
    let mut saved = WorldSave::fresh();
    for session in 0..3u64 {
        let e = engine(&dir, wire::MIN_CAPACITY);
        let mut j = Java::open(e.clone(), saved);
        let gen_seq = j.journal.back().unwrap().0;
        assert_eq!(
            wire::u16_at(&j.journal.back().unwrap().1, wire::R_KIND),
            obs::JOURNAL_GEN
        );
        let live = 20 + session;
        carried(&mut j, live, 1);
        j.idle();
        no_error(&j);
        assert_eq!(ledger(&e, live), 1, "session {session}: the live record");
        if session == 0 {
            assert_eq!(replay(&e), (1, 0, 0), "nothing to replay: ended at the arm");
            assert_eq!(e.stats.stale_skipped.load(Ordering::Acquire), 0);
        } else {
            assert_eq!(
                e.stats.stale_skipped.load(Ordering::Acquire),
                1,
                "session {session}: the kept JOURNAL_GEN, which the pages include"
            );
            assert_eq!(
                replay(&e),
                (1, 1, tail),
                "session {session}: the tail replayed, and nothing after the JOURNAL_GEN"
            );
            assert_eq!(
                ledger(&e, 10 + session - 1),
                tail as i64,
                "the tail applied"
            );
        }
        save_pages(&mut j);
        let through = j.e.inspect().replay.applied_through().0;
        j.run_until("the acknowledgement", |j| {
            j.journal.front().is_none_or(|(s, _)| *s > through)
        });
        assert_eq!(
            j.pinned_gen.as_ref().map(|(s, _)| *s),
            Some(gen_seq),
            "session {session}: its JOURNAL_GEN outlives the trim"
        );
        j.hold_acks = true;
        for _ in 0..tail {
            carried(&mut j, 10 + session, 1);
        }
        j.idle();
        saved = j.crash();
    }
}

/// SEC-1's meeting-point review, item 2 (docs/replay-gate.md §2.3, RP-50): a copy new by its
/// generation (generation 0 is new each time) whose op id the engine already heard is answered as
/// a duplicate. It applies nothing, so it is counted as dropped, not applied, and is dispatched
/// without the copy bit (`dispatch` asserts the bit only on the apply path). `replaying`: the copy
/// was journaled after the pages and is replayed after a crash, where the replay counts no record
/// for it (§3.2); otherwise it comes live.
fn a_copy_whose_op_id_was_heard(replaying: bool) {
    let dir = tmpdir(if replaying {
        "reassert-dup-op-replayed"
    } else {
        "reassert-dup-op"
    });
    let op = wire::JAVA_OP_BIT | 0x52;
    let e = engine(&dir, wire::MIN_CAPACITY);
    let mut j = Java::open(e.clone(), WorldSave::fresh());
    let seq = j.transfer(op, 1, 7, 5);
    let held = Held {
        nonce: 0,
        bytes: j.journaled(seq).unwrap(),
    };
    j.idle();
    assert_eq!(ledger(&e, 7), 5);
    if replaying {
        save_pages(&mut j);
    }
    reassert(&mut j, &held);
    j.idle();
    no_error(&j);
    assert_eq!(ledger(&e, 7), 5, "answered as a duplicate, live");
    assert_eq!(copies(&e), (0, 1), "dropped, not applied, live");
    if !replaying {
        e.close(true, Duration::from_secs(5)).unwrap();
        return;
    }
    let saved = j.crash();
    let j = start(&dir, saved);
    no_error(&j);
    let e = &j.e;
    assert!(
        e.inspect().replay.knows_op(rtsim_kernel::journal::OpId(op)),
        "the pages hold the op id"
    );
    assert_eq!(e.stats.replays_ended.load(Ordering::Acquire), 1);
    assert_eq!(ledger(e, 7), 5, "answered as a duplicate, replayed");
    assert_eq!(copies(e), (0, 1), "dropped, not applied, replayed");
    assert_eq!(
        e.stats.replay_records.load(Ordering::Acquire),
        0,
        "and no record applied while replaying"
    );
    e.close(true, Duration::from_secs(5)).unwrap();
}

#[test]
fn a_copy_whose_op_id_was_heard_is_dropped_not_applied() {
    a_copy_whose_op_id_was_heard(false);
}

#[test]
fn a_replayed_copy_whose_op_id_was_heard_is_dropped_not_applied() {
    a_copy_whose_op_id_was_heard(true);
}
