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
| JB4 | **Two limits: 10 s per wait (`WAIT_MILLIS`), and 3 held world saves in a row (`GIVE_UP_ROUNDS`) before one goes ahead without its chunks** (logged as an error, counted `late`). Both PLACEHOLDER. | The wait blocks a shared IO thread and, at a flush save, the server thread, so it needs a bound; 10 s is about a thousand times the waits measured with JB2 and keeps a shutdown short. Expiry costs nothing but a journal file one world save older (JB1). The second limit is for a chunk that never resolves (an unknown storage, a chunk lost without an unload event): without it the journal's file would never be written again, and a crash would then roll back everything since. 3 saves is 15 minutes of autosaves; a hold that lasts that long needs a disk that writes nothing for 10 s three times in a row. *Superseded in part by RP-205 (slice U):* the second limit now applies only to a note no later save can clear (a chunk that cannot be serialized or stored, or whose unload is never seen), which is given up alone; a hold for any other reason has no limit, since a give-up opened direction A for every noted chunk, and some of those hold goods twice then. |
| JB5 | **What holds a round without waiting.** A noted chunk that is neither loaded nor seen unloading: C2ME's asynchronous unload is serializing it off the server thread and nothing has reached the storage yet. `ChunkEvent.Unload` (LOWEST) marks it; vanilla posts it before the unload's save in the same task, C2ME after queuing the write, so from then on the round waits for its write as usual. A chunk vanilla could not serialize (`ChunkMap.save` false or throwing) is marked unsaved again and holds. A failed write seen by JB2 holds, and marks the chunk unsaved again (vanilla cleared the flag before writing) so the next save writes it; a chunk that has left its level by then cannot be saved again, and holds every round until JB4's valve (*since RP-205:* it is given up at once, at the arm). | Waiting for the unload on the IO chain would stall a flush save: the server thread would be waiting for the chain while the chain waited for an event only the server thread posts. A hold is free and the chunk is resolved by the next save. |
| JB6 | **No early retry: a held journal write waits for the next world save.** | Holds are rare: with vanilla's storage only a failure or a 10 s stall holds; with C2ME also a noted chunk caught in its asynchronous unload at the moment of the save, a window of milliseconds. A retry on a timer would add a save path outside vanilla's and, under a stalled disk, more load on it. The cost is a journal file up to one autosave interval older, which until P29 is wired means a crash in that window starts with the engine's pages ahead of the world save (`EngineAhead`, logged), as a crash between a page save and its journal write already did. "For Ben" 3. *Superseded in part by RP-205 (slice U):* a flush save (the stop, `/save-all flush`) whose write was held tries once more after every chunk is flushed, then keeps the older file; an autosave's hold still waits for the next world save. |
| JB7 | **The cost on a potato and on LiveSrv (D1).** The server thread: at each journal write with notes, one `ChunkMap.save` per noted chunk still unsaved (the autosave saves every unsaved chunk outside the 10 s cooldown anyway) and one small task per level; it never waits. A save with no notes costs an empty check. The chunk worker (vanilla): the same writes it would have made later, made first. C2ME: a poll task every 20 ms while waiting and an fsync of the noted chunks' region files (usually one to four) on the IO chain, instead of a full flush at each autosave. Flush saves: vanilla's own flush has written everything before the server thread joins the chain, so the barrier adds nothing there. | D1: smoothness on a potato first, and the barrier never stalls the server thread. On LiveSrv (4 cores, about 90 "Can't keep up" a day, view distance 16, C2ME) the first barrier's full C2ME flush at autosaves stalled chunk reads; that is gone. A slow disk (an SD card, a busy HDD) makes each noted chunk's write slower, not the whole backlog's. |
| JB8 | **The tests.** Core: `aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk` (a chest with random counts, 1,000 chunk writes of 16 KB queued ahead of it on the real worker: the journal's file is written, the chunk on disk has the chest, most of the backlog is still queued); `aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave` (JB5 and JB4's valve: a chunk in flight holds the file with no wait, its unload releases it, a chunk that never resolves holds 2 saves and the third is written late); `theWorldSavedMarkerNeverPassesTheJournalOnDisk` (JB1 on the real session). Create: the crash test waits on the real condition: after each `saveEverything` it reads the journal's file and the region file on the IO chain, and while the file does not yet hold the reconcile (a held round) it saves again, then checks the chunk on disk holds it and that nothing was late. It records every save and load of the far chunk (`ChunkDataEvent`: what each tag held, on which thread) and the region file's write time for the chunk, so a failure says which write is on disk. `-Pc2me` runs the core suite with the pack's C2ME jar (JB3's path); the tests check the strategy the barrier picked. | The old crash test waited for the barrier's first write and failed when it went over its limit; now a hold is a legal outcome and the test asserts the invariant itself (never ahead), under any IO latency. The 1,000-write backlog reproduces the GameTest server's backlog on demand, so the vanilla test fails against the first barrier whatever the load. |
| JB9 | **Not covered.** Entities (R24: a reconcile's spilled items, residents); other mods' SavedData written in the same loop (R24); C2ME's failed writes, which C2ME only logs (the position leaves the cache either way); P29 itself: the engine's pages are still saved at the world save, ahead of a held journal file. When P29 is wired, `Journal.trim` and the drop of pending records at an acknowledgement should be gated by `min(ack, writtenThrough)`. *Since slice U:* other mods' SavedData of the same save reach disk before the journal's file (RP-200), and the ops whose effects they hold are kept beside them (RP-204, RP-206, RP-207); the entities R24 names are slice U-b's; what is still not covered is the residual list in docs/replay-gate.md 9.6. | |

## Slice U: the journal's file last, and the ops kept beside their effects

Slice U audited every state-changing op kind against a crash that splits a world save (the audit's
outcomes are in docs/replay-gate.md 9.6). Its two directions:
- **A:** the journal's file on disk, a store from before. The journal knows the op, so the engine never
  sends it again; the effect is missing (or, for an op that took goods, the goods are back).
- **B:** the store on disk, the journal's file from before. The engine sends the op again, and only a
  dedup saved with the effect stands between it and a second application.

Before the slice the journal's applied set was the only dedup of four effects that live in another
SavedData file: Diesel's `cdg_oil_chunks` (IND_DEPOSIT_DELTA), Destroy's level pollution in
`neoforge_data_attachments` (IND_DEPOSIT_DELTA), and Create's `create_tracks` (IND_CARRIAGE_DELTA, and
IND_WRITE_PACKAGE's offline buffer). Vanilla writes a level's SavedData in its HashMap's order and the
overworld before the other levels, so which direction a pair took depended on the map's capacity: both
happened. Slice U makes it one direction (RP-200) and makes that direction exact with a ring saved with
the effect (RP-204, RP-206, RP-207). Slice U-b (below the "For Ben" list) takes the chunk and entity rows.

- **RP-200, the journal's file is the last SavedData a world save writes (JL).**
  - *Chosen.* During `MinecraftServer.saveAllChunks` (every autosave, `/save-all`, the stop) the journal's
    write task and the WORLD_SAVED marker behind it are held by `WorldSaveOrder` and go on NeoForge's
    SavedData IO chain when saveAllChunks returns, behind every level's SavedData
    (`MinecraftServerSaveMixin`, PATCHES.md row 86). The chain runs one task at a time, so a journal file on
    disk means every SavedData file of its save is on disk too. The barrier's wait for its noted chunks
    (JB1 to JB5) still runs first, inside the journal's task. If saveAllChunks throws, the held tasks go out
    at the next one's return: later, the safe side.
  - *Why.* Direction A is the dangerous one for a SavedData pair: the op is never resent, so nothing can
    repair it, and for a draw it is goods from nothing (Diesel's oil delivered and not drawn). Direction B
    can be made exact by a ring in the store's own file (RP-204). One direction for every pair is simpler to
    reason about than an order per pair.
  - *Tests.* dieselgenerators `DepositCrashGameTests`, which read the oil file, its ring and the journal's
    file at one instant of the journal's write on the IO chain and restart from them. The two "after" tests
    (nether, and overworld with the map grown until the journal came first) failed first on c1008dd3 with
    5,000,000 mB left after a 1,234 mB draw (the dupe), and pass with 4,998,766.
  - *Left.* JL orders SavedData only. saveAllChunks returns once the chunk and entity writes are queued on
    their own IOWorkers, and the autosave skips a chunk the background saver wrote less than 10 s earlier,
    so a chunk or entity effect that the barrier does not note can still reach disk after the journal (the
    residual list in docs/replay-gate.md 9.6).
- **RP-201, directory syncs (the coordinator's power-loss option (a)).**
  - *Chosen.* The journal's task syncs every level's `data/` directory before its write, and its own
    directory after it, so WORLD_SAVED never claims a rename a power loss undoes. The barrier syncs the
    region and entity folders of its round's chunks after their files (a region file made in this save is
    a new entry in its folder). `DirectorySync` never throws; on Windows, where Java cannot open a
    directory, the sync is skipped, the first failure is logged once and every one is counted.
  - *Why.* NeoForge's `atomicWrite` forces a file's bytes and then renames it into place, but nothing syncs
    the rename. A power loss could keep the journal's rename and lose an earlier store's: direction A again.
  - *Cost (D1).* A few directory fsyncs per world save, on the IO chain, never on the server thread;
    `/rtsim perf` shows the count, the failures and the time.
  - *Tests.* `DirectorySyncTest` (the sync, and the Windows log-once with three failures counted). A
    GameTest cannot cut the power; crash GameTests read the files the way the IO chain leaves them, and an
    fsync call is not taken as proof.
  - *Left.* Windows: NTFS orders its metadata but does not promise a rename across a power loss.
- **RP-202, the double draw (superseded by RP-204).** With JL alone, a crash right before the journal's
  write leaves the oil file with the draw and the journal without the op: the resend drew the deposit
  again (4,997,532 mB left, measured by `aNetherDrawIsTakenOnceWhenTheServerDiesRightBeforeTheJournal`).
  That is a loss, the safe side, and was first recorded as accepted; RP-204 removes it.
- **RP-203, the journal's file is written only in a world save.**
  - *Chosen.* A save of the overworld outside saveAllChunks (a mod's own `ServerLevel.save`, or one off the
    server thread) leaves the journal dirty and its barrier's notes in place for the next world save. The
    first is logged with its stack; every one is counted ("outside a world save" in `/rtsim perf`).
  - *Why.* Outside a world save the other levels' SavedData are not being written, so a journal written
    then could claim an effect that lives in the nether's `cdg_oil_chunks` while that file is older on
    disk. Vanilla saves a level only inside saveAllChunks, so on the pack this never fires.
  - *Left.* A mod that saves the overworld on its own schedule only delays the journal to the next world
    save.
- **RP-204, rings of applied ops beside the effect.**
  - *Chosen.* `DurableOpRing` (core `journal/`): the engine ops applied to one SavedData store, saved in the
    store's own file under `mic_rtsim:applied_ops`, each with the sequence of the COMMAND_RESULT that
    answered it. The dedup is "the store's ring, or the journal".
    - *Trim.* At the store's save, an entry goes once the journal's file on disk holds its answer: the
      journal is then the dedup, and once the engine has acknowledged a result it never sends the op again.
      For an APPLIED answer that dedup is the journal's applied index. An entry stamped by a DUPLICATE_OP
      answer is not in that index; its guard is the resend's own command sequence, at or below the
      journal's last applied one, which `BoundarySession` skips as stale.
    - *Entries read from disk.* Those at or below what the journal's file held at this start go at the first
      trim. Those above it are what a crash between the store's write and the journal's left; their
      sequences are handed out again in this life, so they wait (PENDING) until the engine resends the op,
      and the answer stamps them with its own sequence.
    - *Cap.* 4,096 entries (TUNABLE): the oldest goes first, and one pushed out before the journal's file
      held it is counted as an overflow (`/rtsim perf`).
    - *Diesel.* A mixin on CDG's `OilChunksSavedData` save and static load (`OilChunksSavedDataRingMixin`,
      PATCHES.md row 88) keeps one ring in each dimension's `cdg_oil_chunks`; the plugin hands it to
      `Industry.depositDelta` through `IndustryPlugin.depositRing`. A resend the ring refuses and the journal
      did not know is counted ("deposit deltas refused by a store's ring"). A plugin that throws while asked
      for its ring is reported and the next plugin asked.
    - *Corrupt.* A ring whose two arrays differ in length keeps every op as PENDING
      (`DurableOpRing.allPending`): their resends are refused, an effect left out (for a draw, ore the
      world keeps), never one applied twice.
    - *The module switch.* The ring is written whatever the industry module's switch, unlike T2's train
      ring and the port's rings, which a save with the module off leaves out (RP-194). A save that left it
      out after a switch off could die before the journal's write, and an op applied before the switch
      would be applied again at its resend. UR-1 does not need the gate: CDG ignores the key at load and
      its save writes into the fresh tag vanilla's `SavedData.save(File)` makes, NeoForge skips an unknown
      level attachment, and the ledger is mic_rtsim_destroy's own file. With the module off the entries
      read from disk stay, trimmed as the journal's file moves, at most 4,096 a store.
  - *Why.* Under JL, B is the only direction for a SavedData pair, and B is exact when the op's dedup is in
    the same file as its effect. "Ring or journal", with the trim by the journal's durable mark, keeps a
    store's ring to about two autosaves of entries however busy the store is; a fixed window sized for
    the busiest store would be a guess. The ring is U's own class: T2's `AppliedOpRing` (create) is a fixed
    window of 256 with no trim, which suits one train, not every Diesel draw of a dimension.
  - *Tests.* `DurableOpRingTest` (6: the trim, entries from disk, the cap and its overflow count, the trim
    before the cap, a corrupt ring's ops kept until their resends, a malformed ring). `aNetherDrawIsTakenOnceWhenTheServerDiesRightBeforeTheJournal`: 4,997,532
    with JL alone (RP-202), 4,998,766 with the ring.
  - *Left.* An entry the cap pushes out before the journal's file holds it (counted): its op's resend
    would be applied again. A PENDING entry whose op is never resent stays until the cap pushes it out; it
    can only refuse that op. UR-1: CDG loads `cdg_oil_chunks` with the extra key and drops it at its next
    save.
    Hole (a), found by slice T's implementer: the trim by the journal's file mark drops an entry the engine
    may still re-decide under the same op id after a replay consumed its answer. A normal restart re-decides
    it in the same step, while the journal's applied index and the ring still hold it; a restart with the
    industry module off, switched on after a world save, applies it again (replay-gate 9.6 residual 7). The
    follow-on is T2's rule (prune by the engine's durable page ack, settle at the session start by the
    journal's newest gen), taken for both ring classes at the merge with T2.
- **RP-205, a hold has no limit; only notes no save can clear are given up (supersedes JB4's second
  limit and JB6 in part; the coordinator's decision).**
  - *Chosen.*
    - A round held for a reason a later save can clear (a noted chunk's write not on disk within
      `WAIT_MILLIS`, a failed write while the chunk is still in its level, a failed fsync) is held at every
      world save, with no round limit. The journal's file only grows older.
    - A note no later save can clear is given up alone, at the arm, and the round goes on without it: a
      chunk that left its level after its write failed, at once; a chunk that cannot be serialized, an
      entity chunk that cannot be stored, or a chunk whose unload is never seen, after `GIVE_UP_ROUNDS`
      (3) world saves in a row. Each is logged at error and counted `late`.
    - A flush save (the stop, `/save-all flush`) whose journal write was held tries once more on the server
      thread after every chunk is flushed. Held again, it keeps the older file, logged at error and counted
      ("kept at a flush save"). The stop never waits longer than two of the barrier's waits, and the second
      try does not count toward a note's give-up. The world-save marker (WORLD_SAVED) is not queued again
      for the second try: it reports the older file it found, so the engine hears of the second try's file
      at the next world save, later than it could (the safe side).
    - The counters, in `/rtsim perf`: held (and in a row, and the last reason), late, the longest wait, kept
      at a flush save, outside a world save (RP-203), ring overflows (RP-204), and the applied ops the
      journal's file is behind the journal, logged once past `BEHIND_WARN` (10,000, PLACEHOLDER). The
      engine counts the kept commands its start resends, every module's (stat 16), shown beside them.
  - *Why.* The audit found that a give-up opens direction A for every noted row, and that some of those
    rows then hold goods twice (a negative reconcile delta, a deposit draw in the chunk, a reconciled block
    destroy, Create's K5 to K7). JB4's valve wrote the journal ahead after three held saves for any reason.
    A hold costs a journal file older by the hold, and while held every SavedData store is in B, which the
    rings make exact; a note that no save can clear never resolves by waiting, so it is the only one let go.
  - *Tests.* Core `aWriteHeldForItsChunksIsHeldAtEverySaveWithNoLimit` (`GIVE_UP_ROUNDS` + 2 holds in a
    row, none written and none late; the next save after the writes land writes the file),
    `aFlushSaveTriesOnceMoreThenKeepsTheOlderFile` (the second try writes the file once the unload is seen;
    when it is never seen the file stays byte for byte, counted, and the give-up count does not move), and
    `aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave` (an unseen unload given up after
    `GIVE_UP_ROUNDS` - 1 holds). rtsim-jni `a_kept_command_is_resent_after_a_crash_with_its_sequence_and_op`
    checks the resend count.
  - *Left.* A given-up note: the outcomes in docs/replay-gate.md 9.6 ("After a give-up"). Taking a
    given-up note's ops out of the applied set and having the engine resend them (turning A into B) needs a
    per-op resend list in the engine; it is a follow-on. A disk that never recovers keeps the journal's
    file old for as long as it lasts: logged once past 10,000 applied ops behind, and a crash then replays
    from the older file. The fixed rings of 256 (T2's train ring, RP-192, and the port's, RP-206) were
    sized for a hold of at most four autosaves; a hold that lasts hours can push a busy train's or port's
    op out before the journal's file holds it, and a crash then applies its resend again. `DurableOpRing`
    (4,096) has the same limit: its trim follows the journal's file, which the hold keeps back, so 4,096
    ops into one store during the hold push its oldest out, counted as overflows (replay-gate 9.6,
    residual 6).
- **RP-206, the station port's rings (PATCHES.md row 87).**
  - *Chosen.* IND_WRITE_PACKAGE's offline buffer is in `create_tracks.dat`, in its port's entry of the
    station's "Ports" list. Each `GlobalPackagePort` keeps two `AppliedOpRing`s of 256 (T2's class,
    unchanged) in that entry, through mixins at the RETURN of `GlobalStation.write` and `read` in T2's
    `require 0` config (`PortRing`): `mic_rtsim:applied_ops` for the ops whose packages went into the
    offline buffer, `mic_rtsim:applied_ops_postbox` for the ops whose packages went into the loaded postbox,
    which Create mirrors into the buffer, unprimed. WritePackages looks the port up on both paths.
    - An offline write is refused when either ring holds the op, and only then: the journal is not asked,
      so a tracks write that failed while the journal's succeeded is written again, once.
    - A loaded write is refused when the postbox's chunk carries the op, the journal knows it, or the
      buffer's ring holds it (packages written offline, then taken into the postbox at its load). The
      postbox's ring is not asked here: the tracks file can hold the ring and the mirror while the
      postbox's chunk is from before the write, and the resend must then write the package again.
    - *Which file is newer.* This record first said a world save writes the tracks file before the
      chunks. That is not the rule (the U-a review's P1): the background saver writes dirty chunks between
      world saves (`ChunkMap.processUnloads`, up to 20 a tick, 10 s apart per chunk), while
      `create_tracks.dat` reaches disk only at a world save. Either file can be the newer, and the chunk
      newer is the usual state for most of an autosave interval. Two hooks cover the chunk newer:
      - *The restore.* When the postbox loads and Create puts a primed buffer back into it
        (`StationBlockEntity.attachPackagePort` calling `GlobalPackagePort.restoreOfflineBuffer`;
        `StationPortRestoreMixin`, PATCHES.md row 87), the postbox's chunk is made to carry each op of the
        buffer's ring whose answer the engine has not acknowledged (`Industry.carryAppliedInChunk`: the
        APPLIED journaled again and carried, not counted again). A chunk saved after the restore then
        refuses the loaded resend of an op whose packages it holds, though the tracks file and the
        journal are from before the write. Without it the resend wrote the packages a second time.
      - *The first offline write into a port.* A port whose buffer this life has not seen Create fill
        from its postbox (`PortMirrorMixin` marks it at the restore of a primed buffer and at Create's
        save of a postbox into its buffer) holds the industry command stream while it reads the postbox's
        chunk as saved (`ChunkMap.read`, off the server thread; at most 10 s, then the write goes on as
        before, logged). The chunk's carried ops go into the postbox's ring, so their resend is refused,
        and an unprimed buffer takes the chunk's postbox inventory, so the write does not prime a copy
        older than the chunk. A primed buffer is left alone: it is Create's newer state. Without it the
        resend primed the tracks file's older mirror, and the restore put it back over the chunk:
        automation's takes since then undone (a dupe), its puts lost.
    - A write records the op in its path's ring and marks the tracks dirty; the loaded path never primes.
    - Without the mixins (probed once with a station written and read back), the old dedup: the journal's
      applied set, and the chunk for a loaded postbox, with one WARN.
  - *Why.* Under JL, K2's direction B (the buffer written twice) is the only one, a dupe either way. One
    ring for both paths would refuse the loaded postbox's resend above, a loss where the journal's dedup
    was exact; no ring on the loaded path would let the offline path write again what the postbox already
    holds, which the restore at the postbox's load makes two copies.
  - *Tests.* Create `PortRingGameTests`: the four crash cases on the buffer (`aCrashThatKept...` one
    package each, `aCrashThatKeptTheBufferAndNotTheJournalWritesAPackageOnce` being K2), the fallback's
    dupe pinned (`withoutTheRingsACrashThatKeptTheBufferAndNotTheJournalWritesAPackageTwice`: two
    packages for one op), `aLoadedPostboxWhoseChunkWasOlderThanTheTracksTakesThePackageOnce` (written again
    while loaded, refused offline with nothing put in the buffer) and
    `withIndustryOffAStationSaveDropsThePortRings` (UR-1). The chunk newer than the tracks file:
    `aRestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageOnce` (the first test that runs Create's
    `restoreOfflineBuffer`) and `anUnloadedPostboxWhoseChunkOutlivedTheTracksTakesEachPackageOnce` (a
    synthetic chunk written through `ChunkMap.write`), each with its twin with the chunk checks off
    (`PortRing.chunkChecksForTest`) pinning the old outcome: two packages for one op, and the stale mirror
    primed.
  - *Left.*
    - The crash above with the postbox unloaded at the resend: refused by the postbox's ring while the
      package is only in the unprimed mirror, a loss. Priming the mirror would revert the postbox to the
      tracks file's copy, which can be older than its chunk: a dupe.
    - A port Create removes and makes anew (the postbox or station broken and placed again) between the
      write and the engine's acknowledgement starts with empty rings.
    - K3's goods moved out of the postbox's chunk before it saved, and K5 (the PostboxAccess mirror),
      which are not journal-ordered: the coordinator's Create slice.
    - The rings' fixed bound under a long hold (RP-205's Left).
    - The chunk newer than the tracks file, where the hooks do not reach (replay-gate 9.6, residual 8):
      a primed buffer whose postbox's chunk is newer (Create's restore overwrites the postbox with the
      buffer, its own behaviour); the restore with the industry module off or the rings unavailable (no
      carry); a chunk read that fails or takes over 10 s; a postbox changed in this life while its station
      did not attach it, after the port's mirror was marked fresh; a chunk saved in the window between
      the postbox's unload and Create's next save of it into the buffer.
    - The module switch: the port's rings are left out of a save with the module off (RP-194's rule), so
      a crash inside the first world save after a switch off, before the journal's write, loses them; an
      op written before the switch is written again if the module is on at the restart. `DurableOpRing` is
      written whatever the switch (RP-204). Both classes take one rule at the move to T2's ring rule.
- **RP-207, Destroy's pollution: one ring per file.**
  - *Chosen.* Level pollution is Destroy's `LEVEL_POLLUTION` level attachment, which NeoForge saves in the
    level's `neoforge_data_attachments`. Its ring is a mic_rtsim level attachment (`mic_rtsim:applied_ops`,
    `RtsimAttachments.LEVEL_APPLIED_OPS`), saved in the same file. The overworld's district ledger
    (`mic_rtsim_destroy_pollution`) keeps its own ring, keyed by op and good (`DurableOpRing.keyOf`), for both
    IND_DEPOSIT_DELTA and a reconcile's chunk-pollution deposits: the plugin passes it to the op
    (`IndustryPlugin.DepositOp.keepIn`) and books the delta only when the ring lacks the key.
    - A resend the level's ring refuses stamps the ledger's entry with the refusal's answer too
      (`IndustryPlugin.depositRefused`, when the ledger holds the key). Left with its first life's
      sequence, the entry turned PENDING at the first trim and stayed until the cap pushed it out, counted
      as an overflow (the U-a review).
  - *Why.* One op changes two files, so a crash can keep either; each file refuses what it already holds.
    For a crash right before the journal's write, with the resend:

    | Files kept | Level pollution | Ledger | Outcome |
    |---|---|---|---|
    | Both | refused by the level's ring | correct | exact |
    | Attachments kept, ledger lost | refused by the level's ring | short by the delta | the under-count below |
    | Ledger kept, attachments lost | applied again | the ledger's ring refuses booking it again | exact |

  - *Tests.* destroy `PollutionCrashGameTests` (4): both files kept (the ledger's entry stamped with the
    level ring's refusal), the ledger lost, the pollution file lost (one rebooking refused), and a crash
    right after the journal's write. Each emulates the restart
    from the files a crash at that instant leaves; all four end at the pollution emitted plus the delta, once.
    Not failing-first: without the rings there is nothing for them to read.
  - *Left.* With the ledger lost, the district's total is short by the delta. The ledger only attributes
    pollution to districts (the sampler reports each total as a deposit the district produces), so the
    engine learns a slightly lower rate; no pollution is created or removed.
- **RP-208, what slice U leaves, and UR-1.**
  - *Residual list.* Kept in docs/replay-gate.md 9.6: the Windows directory sync (RP-201), given-up notes
    (RP-205), chunk and entity effects the barrier does not note, the ledger's under-count (RP-207), ring
    entries pending a resend that never comes (RP-204), the rings' limits, a re-decided op after its answer
    was consumed, and a postbox's chunk newer than the tracks file where RP-206's hooks do not reach. Each item says what it costs and who owns it.
  - *UR-1.* Uninstalling mic_rtsim_dieselgenerators: CDG loads `cdg_oil_chunks` with the extra key and drops
    it at its next save. Uninstalling mic_rtsim: NeoForge skips the unknown `mic_rtsim:applied_ops` level
    attachment in `neoforge_data_attachments` with an ERROR log line and loads the rest. The ledger's ring
    is in mic_rtsim_destroy's own file.

## For Ben

1. **The limits (JB4): 10 s per wait and 3 held world saves in a row, both PLACEHOLDER.** Options: (a) keep
   them (recommended: a hold costs only a journal file one autosave older, and three holds in a row need a
   disk that stalls for 10 s at three autosaves running); (b) no second limit: never write ahead, accept that
   a chunk that never resolves stops the journal's file from being written; (c) a longer wait (30 s or 60 s):
   fewer holds on a very slow disk, a longer worst case at shutdown. *Superseded in part by RP-205 (slice U,
   the coordinator's decision):* the 10 s wait stays; the second limit now applies only to a note no later
   save can clear, given up alone; a hold for any other reason has no limit.
2. **C2ME through reflection, never run against a real C2ME world (JB3).** The C2ME path is read from the
   0.4.0-alpha.0.122 jar and runs in the core suite with `-Pc2me`; nothing has run it on a server with players.
   Options: (a) keep it, and watch the log at LiveSrv's first start for "C2ME's chunk storage is not the one
   this was written against" (recommended); (b) `-Dcom.ishland.c2me.chunkio.syncDiskWrites=true` on LiveSrv
   so C2ME writes with DSYNC like vanilla and the barrier fsyncs nothing; (c) drop C2ME from the pack.
3. **No early retry of a held journal write (JB6).** Options: (a) none, the next autosave writes it
   (recommended); (b) one retry a few seconds after a hold caused by C2ME's unload window, which shortens the
   window in which a crash starts with the pages ahead of the world save (P29) from up to 5 minutes to seconds.
   *Superseded in part by RP-205 (slice U):* a flush save (the stop) retries once after every chunk is
   flushed; an autosave's hold still waits for the next autosave.
4. **The fsync of noted region files when DSYNC is off (JB3).** It only matters for a power cut or an OS
   crash, not a JVM crash. Options: (a) keep it (recommended: one to four files per autosave, on an IO
   thread); (b) skip it and accept that a power cut can leave the journal ahead of a chunk under C2ME.
