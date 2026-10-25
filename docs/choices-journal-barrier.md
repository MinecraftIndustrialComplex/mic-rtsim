# Choices: the journal's chunk barrier, reworked (PATCHES.md row 26)

Branch `fix/journal-barrier`, 2026-10-21. Reworks R21 to R24 (docs/choices-repair.md) after
`aReconcileSurvivesACrashBetweenTheJournalWriteAndTheChunkWrite` (create) failed under load: its barrier
hit the 60 s limit at a load of 50 and waited 43 s at a load of 11 to 17. Reversible choices made without
asking, each with what and why. Ben's decisions are at the end ("For Ben"); none blocks the work.

Verified against the Minecraft 1.21.1 / NeoForge 21.1.249 sources (the neoform `sourcesAndCompiledWithNeoForge`
jar) and the C2ME jar the pack ships (`c2me-neoforge-mc1.21.1-0.4.0-alpha.0.122`, decompiled).

## What was measured

A probe (a temporary mixin on `IOWorker`, commit 6eb35a0 on a throwaway worktree, never merged) logged the
overworld's chunk worker around each journal write of the create suite on the stub engine, with the first
barrier (`IOWorker.synchronize(true)` for every level with a noted chunk).

| At the crash test's first save | Siblings' builds running | The same, plus 16 CPU burners (load about 50) |
|---|---|---|
| Chunk writes pending in the worker at the arm | 1,718 | 1,143 |
| Tasks in the worker's mailbox at the arm | 23,111 | 19,810 |
| One chunk write (`runStore`, with DSYNC), mean | 19.9 ms | 25.2 ms |
| The noted chunks' places in the pending queue | 846 and 1,214 | 640 and 1,049 |
| When they were on disk, after the arm | 16.7 s and 24.5 s | 15.4 s and 26.3 s |
| When `synchronize(true)` returned (the old wait) | 34.4 s (1,719 writes of 19.9 ms) | 29.1 s (1,144 writes of 25.2 ms) |
| How long a foreground task queued at the arm waited to run | 7 ms | 46 ms |

**Why the waits were tens of seconds.** Vanilla's `IOWorker` keeps a level's queued chunk writes in
`pendingWrites` (a `LinkedHashMap`, oldest first) and writes one per background turn: every foreground task
(a store, a read) adds one background `storePendingChunk`, and background tasks run only when no foreground
task is queued. The GameTest server writes with DSYNC (`MinecraftServer.forceSynchronousWrites()` is true; a
dedicated server's `sync-chunk-writes` defaults to true too), so each write is a synchronous disk write, about
20 ms on this laptop while siblings build. The GameTest server generates and saves a lot of terrain (every
test's arena), so at any save the worker had well over a thousand writes queued. `synchronize(true)` waits
for every one of them, then fsyncs every region file: the barrier waited for the whole level's backlog, not
for its own two chunks, which were somewhere in the middle of it. The wait grows with the backlog times the
write time, and both grow with load: hence 34 s here, 43 s at a load of 11 to 17 in the failing run and over
60 s at 50. The server thread was not the problem; a foreground task ran 7 ms (46 ms under the burners)
after it was queued. CPU load matters less than it seems: the burners (at nice 19) made each write a quarter
slower; the backlog and the synchronous writes make the wait. At the heavy run's shutdown save 1,876 writes
were pending at 31.2 ms each: the old barrier waited 58.6 s, 1.4 s short of its limit, although its two
chunks were on disk after 0.4 s and 8.0 s.

**What production did at the limit (the first barrier).** At 60 s, or when a chunk write failed, the journal's
file was written anyway (logged, counted `late`), and the noted chunks had already been forgotten at the arm.
A crash before the chunk reached disk then left the journal saying applied and the chunk without it: the
reconcile's goods (or a transfer's, a package's) lost, never minted (the epoch and op-id dedup stay right, so
nothing is duplicated). In the probe's own run the shutdown save hit it: the overworld's save took 64 s on
the server thread (vanilla's own flush of the same backlog), the barrier gave up at 60 s, and the journal
was written 4 s ahead of its chunk. Besides:
- the IO chain (NeoForge's SavedData chain on `Util.ioPool`) was blocked for the whole wait, and every
  SavedData written after the journal waited with it;
- the server thread waited only at flush saves (shutdown, `/save-all flush`), which join that chain
  (`IOUtilities.waitUntilIOWorkerComplete`): up to 60 s on top of vanilla's own flush;
- with C2ME (the pack ships it), `synchronize(true)` is C2ME's full flush: it drains every queued write,
  spins until their compression is done and fsyncs every open region file, on C2ME's storage thread, which
  meanwhile reads no chunk. That ran at every autosave that had a noted chunk;
- C2ME's asynchronous unload was not covered: a noted chunk it was serializing off the server thread was
  neither loaded (so not saved again) nor written yet, and the journal went ahead of it with no wait at all.

**A limit has to exist**, since the wait runs on a shared IO thread that a flush save joins on the server
thread. What was wrong was the action at the limit (write ahead) and what was waited for (the backlog).

**With the reworked barrier (JB2 to JB5, commit c09dbdd).** The core test queues 1,000 chunk writes of
16 KB on the real worker ahead of its chest's chunk. The journal's file was on disk 59 ms after the save,
the barrier waited 32 ms (1 forced save, nothing held, nothing late), and none of the 1,000 backlog
writes had been made yet. The create crash test under 16 CPU burners waited 0 ms at its first save and
52 ms at its second (2 chunks saved early, 0 writes held, 0 late). Both times the chunk on disk held the
reconcile, and the restart read 12 gold at epoch 2. The wait is now the noted chunks' own writes, so it
no longer grows with the backlog.

## Choices

| Id | Choice | Why |
|---|---|---|
| JB1 | **Never ahead: a journal write whose chunks are not on disk is held, not written late.** The file stays as it was and the journal stays dirty (`JournalSavedData.retry`), so the next world save writes it. A note stays until a write that covered it is confirmed (`confirmedRound`), so a held round's chunks are waited for again. The world-save marker (WORLD_SAVED) reports `min(through, writtenThrough)`, the last sequence actually in the file, instead of the journal in memory. A journal write that fails (an IOException) is retried at the next save the same way and never reported by the marker. | A file older than its chunks is the safe order (docs/boundary.md): the chunks carry their records (`mic_rtsim:pending`, the reconcile epoch) until the engine acknowledges them, and dedup by op id and epoch handles a replay. A file ahead of a chunk loses goods. The marker had the same flaw as the late write: it told the engine a world save held observations its journal file did not have (also for a failed write, which `SavedData.save` only logs: the "Still open" gap in docs/boundary.md). It also fits P29's recommended design (K12): WORLD_SAVED becomes `confirm_through`, so a held journal leaves the older page generation confirmed. |
| JB2 | **Vanilla: the noted chunks are written by foreground tasks, ahead of the background queue.** A mixin on `IOWorker` (PATCHES.md row 40) adds `writeNow`: a foreground task that takes a position's entry out of `pendingWrites` and writes it as vanilla's `runStore` does (`storage.write`, then the entry's future, so vanilla's own store futures and `synchronize` stay right; a failure is logged as vanilla logs it and returned). One position per task, the next queued when the last is done. Only when the worker's class is exactly `IOWorker`. | Foreground tasks run before every background task, so the wait is the foreground queue (7 ms measured) plus the noted chunks' own writes, not the level's backlog. The chunks are written once, earlier, never twice: the entry leaves the map, and the background pass writes the next one. One per task lets a chunk read the server thread is waiting for (a synchronous load is a foreground task too) run between two of them, as it would between two of vanilla's own writes. Rejected: `synchronize(true)` (the backlog: 34 s); writing through a second `RegionFile` handle (two writers of one region file break its sector table); a priority flag on `store` (more of vanilla to change for the same result). C2ME's subclass overrides `store` and keeps its own queue, so the mixin's map is empty there. |
| JB3 | **C2ME: wait for the noted positions to leave its write cache, then fsync their region files.** `C2meWrites` (PATCHES.md row 42) reads C2ME's storage thread by reflection: its `executor`, its `cache` and its `RegionFileStorage`. Every 20 ms a small task on that thread looks whether the positions are still in `cache`; C2ME removes a position and writes it in the same task (`if (cache.remove(pos, future)) invokeWriteChunk`), so a position gone from it is written. C2ME turns DSYNC off, so once written the barrier forces the noted chunks' region files to disk itself (`FileChannel.force`, on the IO chain); with DSYNC on (vanilla's default) it does not. Another `IOWorker` subclass gets `synchronize(true)` (warned); a chunk storage that is not an `IOWorker` cannot be ordered (warned once) and is not waited for. A C2ME whose fields differ falls back to `synchronize(true)` with a warning. | C2ME writes each chunk as soon as its compression is done, in no queue order, so there is nothing to move ahead; waiting for the positions themselves replaces a full flush (every queued write, then every region file fsynced, chunk reads stalled meanwhile) with a poll that costs one tiny task per 20 ms on C2ME's thread. Reflection, not a mixin: C2ME is an alpha with no API, and a mixin on it would fail the launch (defaultRequire 1) on any update; reflection degrades to the old behaviour with a warning. The fsync keeps the order across a power cut (the journal's own write is atomic and flushed by NeoForge); it only costs where it is needed. |
| JB4 | **Two limits: 10 s per wait (`WAIT_MILLIS`), and 3 held world saves in a row (`GIVE_UP_ROUNDS`) before one goes ahead without its chunks** (logged as an error, counted `late`). Both PLACEHOLDER. | The wait blocks a shared IO thread and, at a flush save, the server thread, so it needs a bound; 10 s is about a thousand times the waits measured with JB2 and keeps a shutdown short. Expiry costs nothing but a journal file one world save older (JB1). The second limit is for a chunk that never resolves (an unknown storage, a chunk lost without an unload event): without it the journal's file would never be written again, and a crash would then roll back everything since. 3 saves is 15 minutes of autosaves; a hold that lasts that long needs a disk that writes nothing for 10 s three times in a row. |
| JB5 | **What holds a round without waiting.** A noted chunk that is neither loaded nor seen unloading: C2ME's asynchronous unload is serializing it off the server thread and nothing has reached the storage yet. `ChunkEvent.Unload` (LOWEST) marks it; vanilla posts it before the unload's save in the same task, C2ME after queuing the write, so from then on the round waits for its write as usual. A chunk vanilla could not serialize (`ChunkMap.save` false or throwing) is marked unsaved again and holds. A failed write seen by JB2 holds, and marks the chunk unsaved again (vanilla cleared the flag before writing) so the next save writes it; a chunk that has left its level by then cannot be saved again, and holds every round until JB4's valve. | Waiting for the unload on the IO chain would stall a flush save: the server thread would be waiting for the chain while the chain waited for an event only the server thread posts. A hold is free and the chunk is resolved by the next save. |
| JB6 | **No early retry: a held journal write waits for the next world save.** | Holds are rare: with vanilla's storage only a failure or a 10 s stall holds; with C2ME also a noted chunk caught in its asynchronous unload at the moment of the save, a window of milliseconds. A retry on a timer would add a save path outside vanilla's and, under a stalled disk, more load on it. The cost is a journal file up to one autosave interval older, which until P29 is wired means a crash in that window starts with the engine's pages ahead of the world save (`EngineAhead`, logged), as a crash between a page save and its journal write already did. "For Ben" 3. |
| JB7 | **The cost on a potato and on OmegaSrv (D1).** The server thread: at each journal write with notes, one `ChunkMap.save` per noted chunk still unsaved (the autosave saves every unsaved chunk outside the 10 s cooldown anyway) and one small task per level; it never waits. A save with no notes costs an empty check. The chunk worker (vanilla): the same writes it would have made later, made first. C2ME: a poll task every 20 ms while waiting and an fsync of the noted chunks' region files (usually one to four) on the IO chain, instead of a full flush at each autosave. Flush saves: vanilla's own flush has written everything before the server thread joins the chain, so the barrier adds nothing there. | D1: smoothness on a potato first, and the barrier never stalls the server thread. On OmegaSrv (4 cores, about 90 "Can't keep up" a day, view distance 16, C2ME) the first barrier's full C2ME flush at autosaves stalled chunk reads; that is gone. A slow disk (an SD card, a busy HDD) makes each noted chunk's write slower, not the whole backlog's. |
| JB8 | **The tests.** Core: `aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk` (a chest with random counts, 1,000 chunk writes of 16 KB queued ahead of it on the real worker: the journal's file is written, the chunk on disk has the chest, most of the backlog is still queued); `aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave` (JB5 and JB4's valve: a chunk in flight holds the file with no wait, its unload releases it, a chunk that never resolves holds 2 saves and the third is written late); `theWorldSavedMarkerNeverPassesTheJournalOnDisk` (JB1 on the real session). Create: the crash test waits on the real condition: after each `saveEverything` it reads the journal's file and the region file on the IO chain, and while the file does not yet hold the reconcile (a held round) it saves again, then checks the chunk on disk holds it and that nothing was late. It records every save and load of the far chunk (`ChunkDataEvent`: what each tag held, on which thread) and the region file's write time for the chunk, so a failure says which write is on disk. `-Pc2me` runs the core suite with the pack's C2ME jar (JB3's path); the tests check the strategy the barrier picked. | The old crash test waited for the barrier's first write and failed when it went over its limit; now a hold is a legal outcome and the test asserts the invariant itself (never ahead), under any IO latency. The 1,000-write backlog reproduces the GameTest server's backlog on demand, so the vanilla test fails against the first barrier whatever the load. |
| JB9 | **Not covered.** Entities (R24: a reconcile's spilled items, residents); other mods' SavedData written in the same loop (R24); C2ME's failed writes, which C2ME only logs (the position leaves the cache either way); P29 itself: the engine's pages are still saved at the world save, ahead of a held journal file. When P29 is wired, `Journal.trim` and the drop of pending records at an acknowledgement should be gated by `min(ack, writtenThrough)`. | |

## For Ben

1. **The limits (JB4): 10 s per wait and 3 held world saves in a row, both PLACEHOLDER.** Options: (a) keep
   them (recommended: a hold costs only a journal file one autosave older, and three holds in a row need a
   disk that stalls for 10 s at three autosaves running); (b) no second limit: never write ahead, accept that
   a chunk that never resolves stops the journal's file from being written; (c) a longer wait (30 s or 60 s):
   fewer holds on a very slow disk, a longer worst case at shutdown.
2. **C2ME through reflection, never run against a real C2ME world (JB3).** The C2ME path is read from the
   0.4.0-alpha.0.122 jar and runs in the core suite with `-Pc2me`; nothing has run it on a server with players.
   Options: (a) keep it, and watch the log at OmegaSrv's first start for "C2ME's chunk storage is not the one
   this was written against" (recommended); (b) `-Dcom.ishland.c2me.chunkio.syncDiskWrites=true` on OmegaSrv
   so C2ME writes with DSYNC like vanilla and the barrier fsyncs nothing; (c) drop C2ME from the pack.
3. **No early retry of a held journal write (JB6).** Options: (a) none, the next autosave writes it
   (recommended); (b) one retry a few seconds after a hold caused by C2ME's unload window, which shortens the
   window in which a crash starts with the pages ahead of the world save (P29) from up to 5 minutes to seconds.
4. **The fsync of noted region files when DSYNC is off (JB3).** It only matters for a power cut or an OS
   crash, not a JVM crash. Options: (a) keep it (recommended: one to four files per autosave, on an IO
   thread); (b) skip it and accept that a power cut can leave the journal ahead of a chunk under C2ME.

## RP-285: an industry answer heard with the module off (slice A, SEC-1's option A)

Written for main, ahead of slices T and U-a; the same record on test/engine-ack-never-resends speaks
of U-a's op rings. It amends U10's industry rule (IR15 in docs/choices-indrework.md, and the switch
bullet of docs/industry-boundary.md, which still say an answer heard while off is ignored; their
amendments come with U-a, as does docs/replay-gate.md 9.7).

- *Chosen.* With `Module.INDUSTRY` off, the native link still hears every record that answers a command
  the engine sent: a COMMAND_RESULT for an industry op, an IND_CARRIAGE_RESULT, an IND_RECONCILE_RESULT,
  and an IND_CHAIN_PACKAGE `CHAIN_IN` that carries a nonzero engine op (the write it answers, SC113).
  Every other industry record is still ignored (U10).
  - An answer takes its command at once (`industry::keep_answer`): out of the outbox
    (`Link::complete`), out of the queue that the switch or a session start with the module off moved it
    back to (`set_enabled`, `write_resends`), or out of the engine's commands not drained yet
    (`Link::unqueue`, through rtsim-industry's new `Engine::take_out`).
  - Its book effect waits. The whole record, with its own game time, is kept in the link page
    (`Link::answers`; `LINK_VERSION` 2) and booked at that game time, in the order Java journaled it,
    before the engine's own session start: at the switch on (`set_enabled`), and, for answers an earlier
    session kept and saved while off, at a session start with the module on, ahead of the replay
    (`session_start`; both through `book_kept_answers`). An APPLIED books nothing beyond the taking and
    is not kept.
  - A REASSERT copy of a chunk's carried answer (`Industry.commandResultInChunk`, the chunk's
    `mic_rtsim:pending` record, journaled again by `RtsimSession.reassert` at the chunk's load) takes
    the same path, and so does an answer heard again (`answered_again`): a DUPLICATE is kept, and the
    switch books it as `command_result` does with the module on. On main that only takes the command;
    slice T's RP-199 adds the drop of a station exchange refused DUPLICATE, and with T this path drops it
    at the switch.
  - The engine's own session start never runs with the module off. `simulate` called `begin_session`
    ahead of its switch gate, so a restart with the module off ran the session start at the restart's
    game time and moved the frozen clock (72,000 to 72,502 in the clock case). `begin_session` now
    returns while off: a restart while off no longer starts the engine's session until the switch on,
    which sets its own game time.
  - `set_enabled` still sends the whole outbox again under new sequences once on: what is left there has
    no answer from Java.
- *Why.* Java trims its journal at the engine's acknowledgement whatever the switch (`Journal.trim`),
  and with it the journal's op index that refuses an op Java applied (`Journal.commandOpApplied`,
  `forgetOp`); a chunk that carries an answer drops it at the acknowledgement too
  (`dropFromChunkWhenAcked`). An answer that reached the engine with the module off was acknowledged and
  dropped, its command kept, and the switch on sent it again: a second application of an op Java no
  longer knew. Answers do reach an engine that is off: the switch comes in a hand-over's preamble
  (MODULES) ahead of the journal's drain, so every answer not drained yet lands behind it, and the core
  journals a loaded chunk's carried answers again (REASSERT) whatever the industry module, then drops
  them at the acknowledgement.
  - Every kind of answer counts, not only the COMMAND_RESULT that this record's first form took
    (2a751255; the U-a review's P1-1, P2-1 and P2-2). A chain write's CHAIN_IN is the only thing that
    lowers its store's debt and its cluster's pool (`chain_written`): a write whose CHAIN_IN went unheard
    was written again under a new op once its ten periods ran out (`WRITE_ANSWER_PERIODS`), a second set
    of packages. A reconcile whose IND_RECONCILE_RESULT went unheard stayed pending past the switch, its
    booking waiting for a ChunkLoaded. A carriage's IND_CARRIAGE_RESULT heard off was lost (IR15's stated
    cost), so the exchange stayed pending past the switch, waiting for a result Java had already sent,
    and a carriage short of its request left the district holding goods Java never moved.
  - The deferral. Booked at once, at the engine's frozen time, an answer would land ahead of the
    breakpoints and wakes due before it, and every decision after it would part from those of a restart
    that replays the same answer with the module on, op ids included, since they derive from their ticks
    (J3). Booked at its own tick before the catch-up, it lands where the module on would have put it.
  - Across a crash while off: `take_save` calls `industry::save` with the module off (the engine's
    changed pages, and the link page with its outbox, queue and kept answers), and `Link::load` reads
    them whatever the switch, so the pages that cover an answer hold both its taking and its kept effect.
- *The switch on in Java.* Main has no op rings (RP-209), so the U-a review's P3-1 (the rings' forget of
  this session's answers at the switch on, now only cautious) has no counterpart here: main's switch on
  forgets only what industry announced (`Industry.forgetAnnounced`), which this record does not change.
- *Cost.* Answers heard while off wait in the link page until the switch, at most one record per command
  in flight.
- *Found on the way.* The link's `TABLES` (the tables a restart loads and a fresh start deletes) lacked
  the package stores' table (0x4906), so every restart dropped every store with its debts, pools and
  write marks, and a fresh start left old store pages behind. Fixed in its own commit (4fb70332); found
  by the chain case.
- *Tests.* rtsim-jni `tests/industry.rs`, module `ack_never_resends`, with the checker of the engine's
  promise that it never sends an op again past the acknowledgement of its live answer (the module's doc
  says how it models main's journal op index).
  - `an_answer_that_reached_the_engine_with_industry_off_is_never_sent_again_after_its_ack`: the switch
    in the tick of a station exchange X's APPLIED; at the read of the ack past it the probe drops X from
    the fake world's applied set (`FakeWorld::forget_applied`, as slice T has it), as the journal's op
    index forgets it. Failing first at 37b2dcfb (the checker only): the engine sent X again after the
    ack, the world applied it twice (APPLIED at sequences 384 and 413), and the checker flagged the
    command. Its IND_CARRIAGE_RESULT is kept and booked at the switch.
  - `a_failed_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off`: taken back once,
    at the switch, at its tick.
  - `a_duplicate_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off`: a J3
    re-derivation refused live in the tick industry goes off; it leaves the outbox at once and the
    exchange still waits after the switch, as with the module on (T's RP-199 drops it).
  - `a_carried_answer_heard_with_industry_off_takes_its_queued_command`: a chunk's carried APPLIED for an
    ordered package write (the live answer's own bytes, as `commandResultInChunk` carries them),
    journaled again as a REASSERT in a session that starts off.
  - `a_restart_with_industry_off_leaves_the_engine_clock_where_the_switch_froze_it`.
  - `a_reconcile_result_heard_with_industry_off_is_booked_once_across_a_crash_while_off` (P2-1): booked
    once at the switch, and no reconcile goes again.
  - `a_short_carriage_result_heard_with_industry_off_is_corrected_once_across_a_crash_while_off` (P2-2):
    a carriage holding half of its unload; the district is corrected once.
  - `a_chain_write_answered_with_industry_off_is_taken_once_across_a_crash_while_off` (P1-1): a W17
    WriteChain to a declared store, its CHAIN_IN journaled ahead of its APPLIED as Java journals them;
    the store's two packages are written once, booked at the switch, after a crash and a restart while
    off.
  - The crash cases restart off on a minimum-size buffer, the replay over at least two hand-overs. Each
    checks that the effect is booked once and that no command with the same effect (same op or new) goes
    out after the switch on.
  - `a_restart_with_industry_off_books_its_replayed_answers_as_a_restart_with_it_on`: one crash snapshot
    (a station exchange cut short, and a tick whose journaled records are only Java's answers, with
    industry switched off and a page save right before them). From copies of the page directory and the
    world save, B restarts with the module on and A restarts off and switches on in its second hand-over;
    both run 24,000 ticks, and the industry pages, the clock and the op ids Java received must match. Two
    B restarts are the control and run first. It covers the replay-while-off path; the cases above cover
    the live one.
  - Against the first form's link (2a751255) the probe (on its kept IND_CARRIAGE_RESULT) and the FAILED,
    clock, reconcile, short carriage, chain (first at the store's page, which 4fb70332 fixes) and
    determinism cases fail; the DUPLICATE and carried cases pass, since the first form already took
    their commands. The carried case fails against the link at 37b2dcfb (the command stays queued).
  - Two of the checker's cases wait for slice T and are ignored here
    (`a_replayed_answer_acknowledged_is_sent_again_and_its_refusal_never_after` and its control,
    `the_checker_flags_the_replayed_answer_under_a_java_that_registers_replayed_acks`): they need T's
    keyed loot op (RP-180); on main a raid decided again carries a new op, so X never goes again.
  - rtsim-industry `tests/chain_replay.rs`,
    `a_write_answered_while_industry_is_off_is_taken_at_its_tick_before_the_catch_up`, stands in for a
    native learned-chain fixture. The native iron line learns no chain link, so the native chain case
    orders its write (W17) and never shows the engine writing again on its own. In the engine test Java
    answers a write while industry is off and the switch on comes thirty periods later: booked at its
    tick before the session start, the write counts once and Java is handed what the mine sent; never
    heard (the control, as the link before 35a8294b did), the catch-up hands Java the write's 64 iron a
    second time under new ops. `a_command_not_drained_yet_is_taken_out_by_its_op` covers `take_out`.
- *Adapted from U-a's form.* No `command_duplicate` (T's drop of a station exchange at an APPLIED heard
  off, and at a DUPLICATE); `FakeWorld::forget_applied` and `Engine::exchange_pending` copied from T
  with the same text; the tests derive an ordered command's op at its tick (`IndustryOps::derive`)
  instead of T's `derive_keyed`.
- *Left.*
  - No test answers a deposit delta while off on a running line (the iron line decides only exchanges,
    loot and reconciles).
  - A chain write the link still holds in its queue when the module goes off was marked handed on at
    that drain (`handed_on`), so a catch-up past its ten periods writes the same goods again under a new
    op while the queued one goes too at the switch on. It is an unanswered write, not an unheard answer,
    and predates this record; found here and left to its owner.
