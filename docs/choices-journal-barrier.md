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

## Choices

| Id | Choice | Why |
|---|---|---|
| JB1 | **Never ahead: a journal write whose chunks are not on disk is held, not written late.** The file stays as it was and the journal stays dirty (`JournalSavedData.retry`), so the next world save writes it. A note stays until a write that covered it is confirmed (`confirmedRound`), so a held round's chunks are waited for again. The world-save marker (WORLD_SAVED) reports `min(through, writtenThrough)`, the last sequence actually in the file, instead of the journal in memory. A journal write that fails (an IOException) is retried at the next save the same way and never reported by the marker. | A file older than its chunks is the safe order (docs/boundary.md): the chunks carry their records (`mic_rtsim:pending`, the reconcile epoch) until the engine acknowledges them, and dedup by op id and epoch handles a replay. A file ahead of a chunk loses goods. The marker had the same flaw as the late write: it told the engine a world save held observations its journal file did not have (also for a failed write, which `SavedData.save` only logs: the "Still open" gap in docs/boundary.md). It also fits P29's recommended design (K12): WORLD_SAVED becomes `confirm_through`, so a held journal leaves the older page generation confirmed. |
| JB2 | **Vanilla: the noted chunks are written by foreground tasks, ahead of the background queue.** A mixin on `IOWorker` (PATCHES.md row 40) adds `writeNow`: a foreground task that takes a position's entry out of `pendingWrites` and writes it as vanilla's `runStore` does (`storage.write`, then the entry's future, so vanilla's own store futures and `synchronize` stay right; a failure is logged as vanilla logs it and returned). One position per task, the next queued when the last is done. Only when the worker's class is exactly `IOWorker`. | Foreground tasks run before every background task, so the wait is the foreground queue (7 ms measured) plus the noted chunks' own writes, not the level's backlog. The chunks are written once, earlier, never twice: the entry leaves the map, and the background pass writes the next one. One per task lets a chunk read the server thread is waiting for (a synchronous load is a foreground task too) run between two of them, as it would between two of vanilla's own writes. Rejected: `synchronize(true)` (the backlog: 34 s); writing through a second `RegionFile` handle (two writers of one region file break its sector table); a priority flag on `store` (more of vanilla to change for the same result). C2ME's subclass overrides `store` and keeps its own queue, so the mixin's map is empty there. |
| JB3 | **C2ME: wait for the noted positions to leave its write cache, then fsync their region files.** `C2meWrites` (PATCHES.md row 42) reads C2ME's storage thread by reflection: its `executor`, its `cache` and its `RegionFileStorage`. Every 20 ms a small task on that thread looks whether the positions are still in `cache`; C2ME removes a position and writes it in the same task (`if (cache.remove(pos, future)) invokeWriteChunk`), so a position gone from it is written. C2ME turns DSYNC off, so once written the barrier forces the noted chunks' region files to disk itself (`FileChannel.force`, on the IO chain); with DSYNC on (vanilla's default) it does not. Another `IOWorker` subclass gets `synchronize(true)` (warned); a chunk storage that is not an `IOWorker` cannot be ordered (warned once) and is not waited for. A C2ME whose fields differ falls back to `synchronize(true)` with a warning. | C2ME writes each chunk as soon as its compression is done, in no queue order, so there is nothing to move ahead; waiting for the positions themselves replaces a full flush (every queued write, then every region file fsynced, chunk reads stalled meanwhile) with a poll that costs one tiny task per 20 ms on C2ME's thread. Reflection, not a mixin: C2ME is an alpha with no API, and a mixin on it would fail the launch (defaultRequire 1) on any update; reflection degrades to the old behaviour with a warning. The fsync keeps the order across a power cut (the journal's own write is atomic and flushed by NeoForge); it only costs where it is needed. |
| JB4 | **Two limits: 10 s per wait (`WAIT_MILLIS`), and 3 held world saves in a row (`GIVE_UP_ROUNDS`) before one goes ahead without its chunks** (logged as an error, counted `late`). Both PLACEHOLDER. | The wait blocks a shared IO thread and, at a flush save, the server thread, so it needs a bound; 10 s is about a thousand times the waits measured with JB2 and keeps a shutdown short. Expiry costs nothing but a journal file one world save older (JB1). The second limit is for a chunk that never resolves (an unknown storage, a chunk lost without an unload event): without it the journal's file would never be written again, and a crash would then roll back everything since. 3 saves is 15 minutes of autosaves; a hold that lasts that long needs a disk that writes nothing for 10 s three times in a row. |
| JB5 | **What holds a round without waiting.** A noted chunk that is neither loaded nor seen unloading: C2ME's asynchronous unload is serializing it off the server thread and nothing has reached the storage yet. `ChunkEvent.Unload` (LOWEST) marks it; vanilla posts it before the unload's save in the same task, C2ME after queuing the write, so from then on the round waits for its write as usual. A chunk vanilla could not serialize (`ChunkMap.save` false or throwing) is marked unsaved again and holds. A failed write seen by JB2 holds, and marks the chunk unsaved again (vanilla cleared the flag before writing) so the next save writes it; a chunk that has left its level by then cannot be saved again, and holds every round until JB4's valve. | Waiting for the unload on the IO chain would stall a flush save: the server thread would be waiting for the chain while the chain waited for an event only the server thread posts. A hold is free and the chunk is resolved by the next save. |
| JB6 | **No early retry: a held journal write waits for the next world save.** | Holds are rare: with vanilla's storage only a failure or a 10 s stall holds; with C2ME also a noted chunk caught in its asynchronous unload at the moment of the save, a window of milliseconds. A retry on a timer would add a save path outside vanilla's and, under a stalled disk, more load on it. The cost is a journal file up to one autosave interval older, which until P29 is wired means a crash in that window starts with the engine's pages ahead of the world save (`EngineAhead`, logged), as a crash between a page save and its journal write already did. "For Ben" 3. |
| JB7 | **The cost on a potato and on LiveSrv (D1).** The server thread: at each journal write with notes, one `ChunkMap.save` per noted chunk still unsaved (the autosave saves every unsaved chunk outside the 10 s cooldown anyway) and one small task per level; it never waits. A save with no notes costs an empty check. The chunk worker (vanilla): the same writes it would have made later, made first. C2ME: a poll task every 20 ms while waiting and an fsync of the noted chunks' region files (usually one to four) on the IO chain, instead of a full flush at each autosave. Flush saves: vanilla's own flush has written everything before the server thread joins the chain, so the barrier adds nothing there. | D1: smoothness on a potato first, and the barrier never stalls the server thread. On LiveSrv (4 cores, about 90 "Can't keep up" a day, view distance 16, C2ME) the first barrier's full C2ME flush at autosaves stalled chunk reads; that is gone. A slow disk (an SD card, a busy HDD) makes each noted chunk's write slower, not the whole backlog's. |
| JB8 | **The tests.** Core: `aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk` (a chest with random counts, 1,000 chunk writes of 16 KB queued ahead of it on the real worker: the journal's file is written, the chunk on disk has the chest, most of the backlog is still queued); `aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave` (JB5 and JB4's valve: a chunk in flight holds the file with no wait, its unload releases it, a chunk that never resolves holds 2 saves and the third is written late); `theWorldSavedMarkerNeverPassesTheJournalOnDisk` (JB1 on the real session). Create: the crash test waits on the real condition: after each `saveEverything` it reads the journal's file and the region file on the IO chain, and while the file does not yet hold the reconcile (a held round) it saves again, then checks the chunk on disk holds it and that nothing was late. `-Pc2me` runs the core suite with the pack's C2ME jar (JB3's path); the tests check the strategy the barrier picked. | The old crash test waited for the barrier's first write and failed when it went over its limit; now a hold is a legal outcome and the test asserts the invariant itself (never ahead), under any IO latency. The 1,000-write backlog reproduces the GameTest server's backlog on demand, so the vanilla test fails against the first barrier whatever the load. |
| JB9 | **Not covered.** Entities (R24: a reconcile's spilled items, residents); other mods' SavedData written in the same loop (R24); C2ME's failed writes, which C2ME only logs (the position leaves the cache either way); P29 itself: the engine's pages are still saved at the world save, ahead of a held journal file. When P29 is wired, `Journal.trim` and the drop of pending records at an acknowledgement should be gated by `min(ack, writtenThrough)`. | |

## For Ben

1. **The limits (JB4): 10 s per wait and 3 held world saves in a row, both PLACEHOLDER.** Options: (a) keep
   them (recommended: a hold costs only a journal file one autosave older, and three holds in a row need a
   disk that stalls for 10 s at three autosaves running); (b) no second limit: never write ahead, accept that
   a chunk that never resolves stops the journal's file from being written; (c) a longer wait (30 s or 60 s):
   fewer holds on a very slow disk, a longer worst case at shutdown.
2. **C2ME through reflection, never run against a real C2ME world (JB3).** The C2ME path is read from the
   0.4.0-alpha.0.122 jar and runs in the core suite with `-Pc2me`; nothing has run it on a server with players.
   Options: (a) keep it, and watch the log at LiveSrv's first start for "C2ME's chunk storage is not the one
   this was written against" (recommended); (b) `-Dcom.ishland.c2me.chunkio.syncDiskWrites=true` on LiveSrv
   so C2ME writes with DSYNC like vanilla and the barrier fsyncs nothing; (c) drop C2ME from the pack.
3. **No early retry of a held journal write (JB6).** Options: (a) none, the next autosave writes it
   (recommended); (b) one retry a few seconds after a hold caused by C2ME's unload window, which shortens the
   window in which a crash starts with the pages ahead of the world save (P29) from up to 5 minutes to seconds.
4. **The fsync of noted region files when DSYNC is off (JB3).** It only matters for a power cut or an OS
   crash, not a JVM crash. Options: (a) keep it (recommended: one to four files per autosave, on an IO
   thread); (b) skip it and accept that a power cut can leave the journal ahead of a chunk under C2ME.
