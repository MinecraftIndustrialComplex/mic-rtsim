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

## RP-290: the switch as a journaled record, in its own slice U-e (slice U-c's design point)

Ben's decision, through the SEC-1 lead, on the question U-c's brief opened: should MODULES be journaled?

- *Chosen.* Yes, as its own slice, U-e, after U-c and U-d land. U-c leaves Java's MODULES path, the
  preamble and SESSION_START's mask semantics as they are, builds its fixes on keep-and-book (RP-291,
  RP-292, RP-294), and tests the order property instead of A/B equality. U-e's scope:
  - a new journaled kind, MODULE_SWITCH (0x0132, beside JOURNAL_GEN 0x0130 and REASSERT 0x0131): the
    journaled prefix (OBS_SEQ, GAME_TIME) and a u32 mask;
  - Java journals it at the switch's real place among the records (`BoundarySession.setModuleMask`),
    not in the preamble, and again with the session's mask right after each session's JOURNAL_GEN;
  - the engine applies it in the journaled dispatch, live and in replay, and the pages hold the mask as
    of their sequence, in the meta pages;
  - SESSION_START no longer sets `enabled` before the replay: the replay starts from the pages' mask,
    and a restart whose config differs from the journal's last mask is that session's own switch;
  - replay-gate.md 9.7's preamble note, U-d's off-path entry and RP-285's test shape are brought to it
    (one A/B determinism test: a restart that replays an off window against one that never had it);
  - no compat work (nothing is released).
- *Why.* MODULES (0x0004) is a latest-wins record: `wire::is_journaled` and `Obs.isJournaled` take
  0x0100 to 0x7FFF only. So a crash and a restart replay a switched-off window as if the module had
  never been off: no switch-off, no records kept, no switch-on reset. The live path and the crash path
  cannot be equal by construction, and RP-285's A/B equality does not hold for the shared-world modules.
  The preamble also writes MODULES ahead of the journal's backlog (`flushPreamble`), so a switch-off
  can reach the engine before answers Java gave while the module was still on. Journaled at its real
  place, a replay reproduces the window exactly. But that is a protocol change (a new journaled kind, a
  Java journal and preamble change, a page format), and doing it inside U-c would have held up the
  fixes, which both options need.
- *Also decided.* No capture of the embodied dragons at the switch-off: it would change what players
  see, and keep-and-book does not need it (RP-292).
- *Cost until U-e.* A restart replays an off window as if the module had been on. The switch still
  jumps the backlog.

## RP-291: a tribute stack's answer heard with the dragons off (slice U-c, part 1)

- *Chosen.* The dragons take a tribute stack's COMMAND_RESULT whatever the switch, at once
  (`dragons::after_core`, `answered_again`).
  - APPLIED or DUPLICATE_OP: the stack leaves `placing` and its coin counts as placed
    (`tribute_placed`).
  - FAILED: the coin goes back to its lair's unplaced coin (`tribute_failed`).
  - Off, the dragons' pager is marked `save_due`, so the pages hold the change at the next save.
  - MSC55 in docs/choices-money-sink.md is amended.
- *Why.* The core takes the stack out of the outbox at its answer whatever the switch, and Java forgets
  the op at the engine's acknowledgement (the journal's op index and the chest's chunk carry). Ignored
  while off (MSC55's first form), the stack stayed `placing`. `write_tribute` wrote it again at the
  switch-on under the same op, and Java, which no longer knew the op, put the coin in the chest a
  second time. MSC55's premise, that Java would answer DUPLICATE_OP, no longer held.
  - Applied at once, not kept for the switch-on (RP-292): neither `tribute_placed` nor
    `tribute_failed` reads the clock (a failed stack's next op comes from the lair's own counter,
    MSC47), and no decision reads a lair's `placed`.
  - A restart needs nothing at `begin_replay`. A stack in `placing` whose answer the journal holds is
    answered by the replay. One written again before that answer is replayed is refused DUPLICATE_OP
    by the journal it is still in.
- *Cost.* None.
- *Tests.* rtsim-jni `tests/protocol.rs`, module `dragons`,
  `a_tribute_stack_answered_with_the_dragons_off_is_placed_once`. ECOLOGY goes off in the tick Java
  applies a lair's 8 tribute stacks, and Java forgets the ops at an ack past their APPLIED, as main's
  journal op index does. Failing first at 9d440b77: 16 writes, 998 coins in the chest for 499. Passes
  at 4fa1173b: 8 writes, 499 coins.

## RP-292: the dragons, the herds and the conflict keep what they hear while off (slice U-c, part 2)

docs/replay-gate.md 9.7 (on U-a): "The switch gates only new decisions; every record of what already
happened is heard; a record applied while off goes in at its own tick, never at the frozen clock."
This record amends U10's rule (docs/overnight-choices-integrate2.md) for the three viewers of the
shared phase 1 world. The ships keep U10 (see *Left*), phase 1 is slice U-d, and industry is RP-285.

- *Chosen.*
  - **Kept.** Each journaled record that `Replay` admits for a module while it is switched off is kept
    whole, with the copy bit it came under (`st.applying_copy`), in the order it came
    (`paging::Kept`). It is still counted in `ignored`. Latest-wins state records are still dropped
    (the dragons' 0x0060 to 0x006F, the herds' 0x0080 to 0x008F, the conflict's 0x0090 to 0x009F):
    the next one replaces them.
  - **Saved.** The kept records are a page of their own, two past the module's meta page, after the
    TEST hash (`paging::kept_key`: 0x4400, 0x4800 and 0x4300, id 2), magic RTKP. The page is written
    at every save while there are any, whether or not the save writes the module (a switched-off
    module's other pages stay as they were, U10). It is read at load whatever the switch. A module
    switched off before its first save may hold that page alone (`holds_pages_besides_kept`).
  - **Applied at the switch-on,** in the resets of every module that shares the phase 1 world, in
    one function (`shared::resets`; U-c's review, P2). MODULES reaches the four in two passes
    (`shared::switch`): first every module's flag, with a switch-off's own work; then the resets due,
    in one order (dragons, ships, herds, conflict) and in three steps:
    1. each module's engine begins its replay (`begin_replay`);
    2. the records kept by every module that began are applied as one list, merged by the observation
       number each came under (the order `Replay` admitted them, and the order a restart's replay
       sends them; a copy's is its REASSERT's), each module's own in the order it came;
    3. with no replay running, each module's replay end.

    That is where a restart's replay would apply the same records: every module in its replay, the
    records in the journal's order, the replay ends after the last.
    - Each as it came. A copy goes in as a copy, so a generation-0 casualty copy is still refused.
    - Each after its own catch-up (`shared::catch_up`), so the events due before the record fire
      first, those of every module that came on with it: the dragons' kept records run the window
      with the herds as their prey.
    - Kept records make the reset due with no dragon, herd or camp too.
    - The session start uses the same function once every module's flag is set, and so does the
      first record after its DICTs. A module's own reset in the middle of a session (a first herd
      or camp) is the same function for that module alone (`shared::reset_these`); it holds no kept
      record.
    - Phase 1's drain (slice U-d) goes into `shared::resets`, after the viewers.
    - *Before* (P2), each module's reset ran as its switch came. ECOLOGY is the dragons' and the
      herds' one bit and MODULES reached the dragons first, so the dragons applied their kept
      records with the herds still off: each record's catch-up ran the window with no prey, and the
      herds' reset then ran it with no predator.
  - **At a session start.** Records an earlier session kept and saved are applied at the resets after
    Java's DICTs, since they name Java's ids. `State::dict_wait` is set at SESSION_START and cleared by
    the first record that is not a DICT, and the module does not run meanwhile (`kept_waiting`).
  - **Epochs.** The dragons (RP-57, RP-75) and the herds (RP-81) still fold a kept record's epoch at
    once while off. The conflict folds it when the record is applied (RP-293).
- *The own tick* (U-c's review, P1). Every kept record is booked at its own tick on its module's
  clock, which froze at the switch-off, before every kept record. While the records are applied
  (`State::applying_kept`), the floor a module that shares the phase 1 world books at and runs its
  events to is its own present, not the world's (`shared::floor`). It holds for every module the
  catch-up runs, not only the one whose record it is: the dragons run inside a herds' record's
  catch-up, and the herds as their prey inside the conflict's. So the floor is State-wide, not a
  flag per module: the conflict's ingest, `run_engine` and rail call, the dragons' ingest and
  `run_engine`, and the ships' `run_engine_at` all take it. The herds' ingest always booked at their
  own present.
  - *Before* (P1), the dragons and the conflict booked no earlier than the world's present
    (`max(at, engine.now(), world.now())`), and the world runs on while they are off if phase 1 or
    another viewer runs. The first kept record's catch-up then ran the whole off window. A band wiped
    out in the switch's tick, with the switch on two days later, was booked after its demand's
    deadline: the demand expired, the envoy went home with its units, and nothing was withdrawn. A
    dragon a player killed in the switch's tick died at the switch-on, its lair vacant from then.
  - *The cost.* From the switch-off to the last kept record, the modules decide at old ticks against
    the world as it is now: its markets, knowledge and legs at its present, so what they read may be
    newer than the tick they decide at. That is safe: `World::run_until` to a tick before its present
    does nothing, so the world never goes back and nothing in it runs twice.
  - *J3* (docs/replay-gate.md 9.7, on U-a). The divergence J3 records goes away for the kept
    records: a switch-on books each at its own tick, as a restart's replay with the module on books
    it, so the op ids derived from that tick (`ops.derive(world_seed, at, ..)`) are the same both
    ways. A late booking at the world's present derived them from another tick.
- *The replay gate during the reset.* The kept records, and the catch-ups between them, run inside the
  module's replay, so its outbox gate (docs/replay-gate.md §4) holds what those catch-ups decide.
  - The conflict drops Notice, Steer, Recall, Settled and AmbushSpot, sends Territory and March, and
    never embodies.
  - So a DEMAND_SETTLED for a demand that comes due in the window, before the last kept record, is
    dropped, as at a restart's replay (RP-12: news only; the envoy it would name is gone with the
    reset). With nothing kept, the replay ends at once and the catch-up runs live, as on main, and
    sends it.
  - The gate is needed: a live catch-up could embody a party before its kept death is heard.
- *Changed on the way.* DIFFICULTY heard while the conflict is off was ignored, and the conflict ran on
  the old difficulty until the plugin's next session start sent it again (MZB-30). It is now kept and
  applied at the switch-on, before the conflict decides anything.
- *Why.* Java goes on reporting what happens to the bodies already in the world after a switch-off.
  - The herds settle the switch's tick's deaths before their capture (`HerdBridge.endOfTick`).
  - The conflict settles its deaths at its switch-off (`ConflictBridge.switchedOff`).
  - The dragons have no switch hook at all, so a dragon body lives until Java's own capture.

  Ignored (U10), those deaths were lost at the switch-on's reset. A dragon killed or tamed in the
  window came back alive, and was materialized again beside its corpse. A sheep killed in the switch's
  tick was picked for the next group. A band wiped out in the switch's tick kept its units and came
  into view again. Applied in the reset before the replay end, a death is booked before the end makes
  the bodies abstract, as a restart's replay would book it.
- *Cost.* One page per module while it holds kept records. It is written when the list changed since a
  save last wrote it, or that save failed (`Kept`'s dirty flag, set again by `drain` on every failed
  save; D1). PF14 makes any save the IO lane commits one built after the step saw every failure, so
  after the flag was set again. The kept list is bounded by what Java still reports while the module is
  off. The conflict's and herds' bridges stop at the switch-off, once the tick's deaths are settled.
  The dragons' bodies report until Java captures them. Past `paging::KEPT_WARN_AT` (4,096) records a
  module's list counts each one more (`Kept::over`) and warns at the first and at each power of two of
  the count, as phase 1's `kept_over` does (U-d). None is dropped: each is a death, a taming or a
  robbery that would come back undone. A list that long is a dragon body Java never captures, or a
  bridge that did not stop.
- *Tests.* rtsim-jni `tests/protocol.rs`.
  - Each case runs in the session, and again after a save and a clean restart while off, which
    carries the kept page across the restart:
    - `a_dragon_killed_while_switched_off_stays_dead` (module `dragons`);
    - `a_dragon_tamed_while_switched_off_does_not_come_back` (`dragons`);
    - `a_herd_animal_killed_in_the_switchs_tick_is_not_picked_again` (`herds`);
    - `a_band_wiped_out_in_the_switchs_tick_does_not_return` (`conflict`);
    - and each one's `_after_a_restart`.
  - Failing first at 3c8b54e7: the dragon alive and materialized again, the sheep alive and picked for
    the next group, the band's units kept and its envoy in view again. All pass at 951276bc.
  - The order property, in module `conflict` (42332592). In both, an envoy is on its road with its
    demand's deadline d0 ahead, the conflict goes off, a mob wipes the band out while it is off (a
    kept record), and the conflict goes on again.
    - `a_kept_wipe_comes_after_the_envoys_arrival_in_the_switch_off_window` (the wipe a day before
      d0): the envoy's arrival at its spot is logged before the withdrawal, the band was not home
      first, and the withdrawal is booked at the wipe's tick or later.
    - `a_deadline_in_the_switch_off_window_comes_due_before_a_kept_wipe` (the wipe after d0): the
      demand expires once, before the wipe's tick; the wipe then finds the band home, a straggler:
      nothing withdrawn, no casualty.
    - Both were written against the fix: before it the wipe was never heard.
  - `on_peaceful_no_war_party_leaves_and_on_normal_the_raid_follows` (`conflict`) now expects the
    DIFFICULTY heard while off to apply at the switch-on.
  - The own tick after a late switch-on (P1), failing first at 56c82bfb (the tests are ce26df60),
    passing at 9660474b:
    - `a_wipe_in_the_switchs_tick_is_booked_at_its_tick_after_a_late_switch_on` (`conflict`): a band
      wiped out in the switch's tick, the switch on two days after the demand's deadline d0. It wants
      nothing expired, the band withdrawn and booked, no units home, the withdrawal logged before d0
      and no "home" before it. Before: 1 expired, 0 withdrawn, nothing booked. Its control
      `a_wipe_in_the_switchs_tick_with_the_switch_on_at_once` passes both ways.
    - `a_dragon_killed_in_the_switchs_tick_stays_dead_after_a_late_switch_on` (`dragons`): killed by
      a player in the switch's tick, on three days later with the player at the lair. Neither alive
      nor materialized again, and the lair vacant from a tick between the death and the switch-on.
      Before: vacant from 73149, the death at 1004.
  - The shared resets (P2): `the_dragons_hunt_the_herds_in_the_window_a_kept_record_reaches`
    (`dragons`). A hungry captured dragon 360 blocks from a flock of 5,000 sheep hunts it within six
    days with everything on (the control). With ECOLOGY off for the six days and a stale hurt kept,
    the switch-on's catch-up to the hurt must hunt it too. Failing first at 9660474b (the test is
    1fe93454): no herd hunt. Passes at 4b859dc6.
  - The bound and the dirty flag (P3), unit tests in `src/engine/paging.rs`:
    `a_kept_list_past_its_bound_warns_at_powers_of_two_and_drops_nothing` and
    `the_kept_page_is_written_only_when_the_list_changed` (1718f4df).
- *Left.*
  - No order test for the dragons or the herds. Neither has an event log a test can read (the herds'
    `log_enabled` is false and has no accessor). Their order rests on the same `shared::catch_up`
    before each kept record that the conflict's tests exercise.
  - No test of the merge across modules: two modules' kept records interleaved in one switch-on. It
    rests on the same catch-up per record, and P2's test covers the step order (every module begun
    before any kept record).
  - The ships (`Module.SHIPS`) still ignore their records while off (U10).
  - No GameTest: no Java changed.
  - A restart still replays an off window as if the module had been on (RP-290, U-e).

## RP-293: the conflict's epoch floors with the module off (RP-57; slice U-c)

- *Chosen.* No fold while off, and nothing built for it. The conflict keeps its records while off
  (RP-292), saved with its pages, and applies each at the switch-on, epoch and all: `ingest` raises the
  floor to the epoch a record names, live or replayed (RP-57). That happens in the reset, before the
  replay end, so before the conflict decides anything. The residual that docs/boundary.md recorded
  under the conflict's switch (fixes6 item 6) is closed.
  - With the shared resets (RP-292, U-c's review P2) another module's kept record can come first in
    the merged list, and its catch-up runs the conflict before the conflict's own records raise the
    floors. That still issues no epoch: the conflict runs inside its replay until every kept record
    is in, and both places that take one (`next_epoch`, in `start_fight` and in `seen`, which
    materializes a party) return at `outbox.defer()` first.
- *Why.* The residual's sequence ran like this. (1) A session crashes with the conflict on, after Java
  journaled records of epochs issued since the last save. (2) The next start has the conflict off: its
  replay ignored those records and saved past the window, so the journal was trimmed of them. (3) A
  later start turns the conflict on, with pages that never saw those epochs. (4) It can issue them
  again. With RP-292, step 2's replay keeps the records on the conflict's kept page, and every save
  writes that page, so a save past the window holds the very records the journal is then trimmed of.
  Step 3's reset applies them and raises the floors before any decision. While off the conflict
  decides nothing, so no epoch is issued between the keeping and the folding.
- *Cost.* None.
- *Tests.* None of its own. The band cases after a restart
  (`a_band_wiped_out_in_the_switchs_tick_does_not_return_after_a_restart`) carry a kept record across a
  save and a restart while off, and apply it at the switch-on. No test plays the whole crash, off start
  and later switch-on sequence.

## RP-294: a chain write held while industry is off is handed on again at the session start (slice U-c, part 3)

- *Chosen.* At the industry engine's own session start (`Link::begin_session`), every IND_WRITE_CHAIN
  in the link's queue or outbox is handed on again at the session start's tick
  (`Engine::hand_on_again`, through `PackageStore::write_handed_on_again`). Its ten periods
  (`WRITE_ANSWER_PERIODS`, SC113) then run from there.
  - Both ways the module comes back on run that session start: the switch-on in a session
    (`set_enabled` moves the outbox to the front of the queue), and a start with the module on after a
    stop with it off (`resume` resends the outbox).
  - An ordinary restart's resends get their ten periods from the restart too. They go to Java again
    there, so their answers are awaited from then.
  - `hand_on_again` touches only the write's own store. A write already answered, or replaced by a
    newer one, is left as it is.
- *Why.* `drain_commands` marks a write handed on (`at` = now) when it leaves the engine. A write the
  link still held at the switch-off (queued, or written and unanswered in the outbox) waited with the
  module off. At the switch-on, the session start's catch-up ran past its ten periods, took its answer
  as lost, and wrote the store's frogport again under a new op, while the held write went to Java too.
  That is the same goods in two sets of packages, since the pool is lowered only at CHAIN_IN
  (`chain_written`). Under 9.7 the held write is not a new decision: it goes, and its answer is
  awaited from when it goes. No keyed ops (slice T) are needed.
- *Cost.* At each session start, one store lookup per chain write the link holds.
- *Tests.* rtsim-jni `src/engine/industry.rs`, unit tests on a mine and a mill linked by a learned
  chain (adapted from rtsim-industry's chain_replay pair). The engine hands a write W of the mine's
  iron on at 20,100, the module goes off, and it comes back thirty periods later.
  - `a_chain_write_queued_at_the_switch_off_is_not_written_again_at_the_switch_on`: W still queued.
  - `a_chain_write_unanswered_at_the_switch_off_is_not_written_again_at_the_switch_on`: W written and
    unanswered.
  - `a_chain_write_unanswered_at_a_stop_with_industry_off_is_not_written_again_at_a_restart_with_it_on`:
    W unanswered in the outbox at a stop with the module off, then a start with it on.
  - Each checks that W alone goes after the switch-on's step, that W still holds the frogport nine
    periods later, and that eleven periods later the frogport is written once under a new op.
  - Failing first: the two switch cases at bf020e42 and the restart case at 9f8df8fd, each sending
    [W, a new op]. All pass at ff0f31ec (the switch cases at 50dfa88d, its first form, which handed on
    again in `set_enabled` and so missed the restart).
- *Overlap with slice A (RP-285).*
  - RP-285's *Left* named this finding; it is closed here.
  - On main (before slice A), a restart with industry off runs the engine's session start at once,
    ahead of the switch gate. That catch-up now hands the held writes on again at the restart, and the
    switch-on hands them on again at its own time. Slice A stops that early session start, so the
    switch-on's covers the writes.
  - Slice A's `begin_session` and `set_enabled` change the same functions: the merge is textual.
  - A write Java applied while off, whose CHAIN_IN came while off, is RP-285's case, not this one.
