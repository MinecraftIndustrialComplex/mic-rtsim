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
| JB7 | **The cost on a potato and on OmegaSrv (D1).** The server thread: at each journal write with notes, one `ChunkMap.save` per noted chunk still unsaved (the autosave saves every unsaved chunk outside the 10 s cooldown anyway) and one small task per level; it never waits. A save with no notes costs an empty check. The chunk worker (vanilla): the same writes it would have made later, made first. C2ME: a poll task every 20 ms while waiting and an fsync of the noted chunks' region files (usually one to four) on the IO chain, instead of a full flush at each autosave. Flush saves: vanilla's own flush has written everything before the server thread joins the chain, so the barrier adds nothing there. | D1: smoothness on a potato first, and the barrier never stalls the server thread. On OmegaSrv (4 cores, about 90 "Can't keep up" a day, view distance 16, C2ME) the first barrier's full C2ME flush at autosaves stalled chunk reads; that is gone. A slow disk (an SD card, a busy HDD) makes each noted chunk's write slower, not the whole backlog's. |
| JB8 | **The tests.** Core: `aJournalWriteGoesAheadOfTheChunkQueueButNotAheadOfItsChunk` (a chest with random counts, 1,000 chunk writes of 16 KB queued ahead of it on the real worker: the journal's file is written, the chunk on disk has the chest, most of the backlog is still queued); `aChunkStillBeingUnloadedHoldsTheJournalsFileToTheNextSave` (JB5 and JB4's valve: a chunk in flight holds the file with no wait, its unload releases it, a chunk that never resolves holds 2 saves and the third is written late); `theWorldSavedMarkerNeverPassesTheJournalOnDisk` (JB1 on the real session). Create: the crash test waits on the real condition: after each `saveEverything` it reads the journal's file and the region file on the IO chain, and while the file does not yet hold the reconcile (a held round) it saves again, then checks the chunk on disk holds it and that nothing was late. It records every save and load of the far chunk (`ChunkDataEvent`: what each tag held, on which thread) and the region file's write time for the chunk, so a failure says which write is on disk. `-Pc2me` runs the core suite with the pack's C2ME jar (JB3's path); the tests check the strategy the barrier picked. | The old crash test waited for the barrier's first write and failed when it went over its limit; now a hold is a legal outcome and the test asserts the invariant itself (never ahead), under any IO latency. The 1,000-write backlog reproduces the GameTest server's backlog on demand, so the vanilla test fails against the first barrier whatever the load. |
| JB9 | **Not covered.** Entities (R24: a reconcile's spilled items, residents); other mods' SavedData written in the same loop (R24); C2ME's failed writes, which C2ME only logs (the position leaves the cache either way); P29 itself: the engine's pages are still saved at the world save, ahead of a held journal file. When P29 is wired, `Journal.trim` and the drop of pending records at an acknowledgement should be gated by `min(ack, writtenThrough)`. *Wired in slice U-h (U-h1, U-h9): the world save's pages stay pending until WORLD_SAVED, and the trim is clamped to `writtenThrough`.* *Since slice U:* other mods' SavedData of the same save reach disk before the journal's file (RP-200), and the ops whose effects they hold are kept beside them (RP-204, RP-206, RP-207); the entities R24 names are slice U-b's; what is still not covered is the residual list in docs/replay-gate.md 9.6. | |

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
  - *Amended by RP-209.* The trim by the journal's file mark, the PENDING state and `allPending` below
    are replaced by T2's rule: an entry leaves at the engine's acknowledgement of its answer, and a ring
    another session answered is settled at the session's start. The rest stands.
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
    - *The module switch.* The ring is written whatever the industry module's switch: the rule for every
      ring (docs/replay-gate.md 9.6, "Rings and the module switch"; the coordinator, 2026-10-25). A save
      that left it out after a switch off could die before the journal's write, and an op applied before
      the switch would be applied again at its resend. UR-1 does not need a gate: CDG ignores the key at
      load and its save writes into the fresh tag vanilla's `SavedData.save(File)` makes, NeoForge skips an
      unknown level attachment, and the ledger is mic_rtsim_destroy's own file. With the module off the
      entries read from disk stay, trimmed as the journal's file moves, at most 4,096 a store.
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
    residual 6). *Since RP-250 and RP-209* every ring prunes at the engine's acknowledgement, which the
    hold does not keep back, and holds 4,096: only 4,096 ops into one ring that the engine has not
    acknowledged push one out.
- **RP-206, the station port's rings (PATCHES.md row 87).**
  - *Chosen.* IND_WRITE_PACKAGE's offline buffer is in `create_tracks.dat`, in its port's entry of the
    station's "Ports" list. Each `GlobalPackagePort` keeps two `AppliedOpRing`s of 256 (T2's class,
    unchanged; 4,096 and pruned by T2's rule since RP-209) in that entry, through mixins at the RETURN of
    `GlobalStation.write` and `read` in T2's `require 0` config (`PortRing`): `mic_rtsim:applied_ops` for the ops whose packages went into the
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
        chunk as saved (`ChunkMap.read`, off the server thread). The chunk's carried ops go into the
        postbox's ring, so their resend is refused, and an unprimed buffer takes the chunk's postbox
        inventory, so the write does not prime a copy older than the chunk. A primed buffer is left alone:
        it is Create's newer state. Without it the resend primed the tracks file's older mirror, and the
        restore put it back over the chunk: automation's takes since then undone (a dupe), its puts lost.
        - An op either ring of the port already refuses is answered DUPLICATE before the hold. Ports are
          read one at a time, as their writes come; there is no read ahead for a batch.
        - A read that fails or takes over 10 s lets this write go on as before, logged, and the port's next
          offline write reads again; after three such reads the port is taken as fresh for the life.
        - A read begun before the postbox's chunk loaded is dropped at a write that finds the chunk loaded:
          the chunk can change while loaded.
        - A chunk of another data version (saved before a game upgrade) gives its carried ops, not its
          inventory: vanilla reads it through its fixers, and the hook does not.
        - Why the mirror is not primed when the chunk lacks an op of the postbox's ring (the U-a review's
          P3 4): a chunk's carried results go at the engine's acknowledgement and the port's rings keep
          theirs, so a missing op does not show the chunk is older than the write. Taking the chunk's
          inventory when it is older is a loss; priming the mirror when the chunk is newer is a dupe.
    - *An orphan port.* A port whose postbox's chunk is loaded without the postbox answers FAILED: Create
      drops a port at the postbox's removal only through a loaded station
      (`PackagePortBlockEntity.destroy` calling `deregister`), so a port can outlive its postbox, and its
      buffer would reach no postbox. Before the U-a review's P3 6 the write went into it, APPLIED.
    - A write records the op in its path's ring and marks the tracks dirty; the loaded path never primes.
    - The rings are saved whatever the industry module's switch (replay-gate 9.6, "Rings and the module
      switch"). Until 2026-10-25 they followed RP-194's gate, left out of a save with the module off; a
      crash inside the first world save after a switch off then lost them, and the resend wrote the
      package again.
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
    while loaded, refused offline with nothing put in the buffer, and a new op into the orphan port left
    FAILED) and
    `aSaveWithIndustryOffKeepsThePortRingsAndTheResendIsRefused` (the module switch, and Create's keys
    kept for UR-1). The chunk newer than the tracks file:
    `aRestoredBufferWhoseChunkOutlivedTheTracksTakesAPackageOnce` (the first test that runs Create's
    `restoreOfflineBuffer`; the postbox's chunk far off, forced loaded for the restore) and
    `anUnloadedPostboxWhoseChunkOutlivedTheTracksTakesEachPackageOnce` (a synthetic chunk written through
    `ChunkMap.write`), each with its twin with the chunk checks off (`PortRing.chunkChecksForTest`)
    pinning the old outcome: two packages for one op, and the stale mirror primed.
    - Not tested: a chunk saved by vanilla in the unloaded case (the test builds the chunk's tag by hand,
      with only what the read looks at); a save, load and REASSERT round trip after the restore; that
      `StationPortRestoreMixin` applied, other than through the restore test's outcome; the retries of a
      failed read; the stale read dropped.
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
      carry); three chunk reads of a port that fail or take over 10 s; a chunk of another data version
      (its inventory is not taken); a postbox changed in this life while its station
      did not attach it, after the port's mirror was marked fresh; a chunk saved in the window between
      the postbox's unload and Create's next save of it into the buffer.
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
      as an overflow (the U-a review). Under RP-209 the refusal's answer is what the engine acknowledges, so
      the stamp is what lets the entry leave.
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
    entries whose answer is unknown, waiting for a resend that never comes (RP-204, RP-209), the rings'
    valves, a re-decided op after its answer was consumed (closed by RP-209), and a postbox's chunk newer
    than the tracks file where RP-206's hooks do not reach. Each item says what it costs and who owns it.
  - *UR-1.* Uninstalling mic_rtsim_dieselgenerators: CDG loads `cdg_oil_chunks` with the extra key and drops
    it at its next save. Uninstalling mic_rtsim: NeoForge skips the unknown `mic_rtsim:applied_ops` level
    attachment in `neoforge_data_attachments` with an ERROR log line and loads the rest. The ledger's ring
    is in mic_rtsim_destroy's own file.
- **RP-209, every ring follows T2's rule (the coordinator, at the re-merge with T2; amends RP-204 to
  RP-207).**
  - *Chosen.* The store rings (`DurableOpRing`: Diesel's oil file, Destroy's level pollution and its
    ledger) and the station port's two rings (`AppliedOpRing`s, `PortRing`) keep an op until the engine
    can no longer send it, as T2's train ring does (docs/choices-mowzie-build.md RP-250, RP-251):
    - Each answer the industry gives an op of a ring (APPLIED, or DUPLICATE_OP on a ring hit the journal
      did not know) stamps the entry with that answer's sequence. The engine's acknowledgement of the
      sequence (its durable save holds the answer) removes the entry at the end of the tick, if its answer
      is still that one. The prunes run from session hooks, whatever the industry module's switch
      (`DurableOpRings.SessionHook` for the stores; `TrainRing.SessionHook` for the trains and the ports,
      all in create_tracks.dat).
    - A ring another session answered is settled once per session against one view of the engine's
      pages and the journal file the session loaded (`DurableOpRings.view`, T2's `RingView`, moved to
      core and shared by every ring): at the session's start for the rings loaded with the world, at the
      load for one loaded later. An entry leaves only when that view proves the pages hold its answer;
      every other entry stays with its answer unknown until its op's resend is refused and the refusal
      stamps it. A ring read again in the session that answered it keeps its answers and waits for their
      acknowledgements again.
    - The industry's announce (the module back on, or the engine lost observations) makes every answer
      this session gave unknown (`DurableOpRings.announce`, `PortRing.announce`).
    - Saved per ring: each entry's answer and the generation of the session that settled the ring last
      (`ops`, `seqs` and `gen` in a store's `mic_rtsim:applied_ops` compound; `_at` and `_gen` beside a
      port ring's long array, T2's format).
    - The caps are valves: 4,096 a store ring, as before, and 4,096 a port ring (was 256). The oldest is
      pushed out first, counted (`/rtsim perf`, `/rtsim district perf`) and, for the ports, logged for the
      first eight.
    - Gone: the trim by the journal's file mark, PENDING, `JournalSavedData`'s load mark,
      `DurableOpRing.allPending` (now `allUnknown`). `Industry.commandResultInChunk` returns the answer's
      sequence, so a loaded postbox's write stamps its ring.
  - *Why.* The trim by the journal's file mark dropped an entry the engine could still re-decide under the
    same op id after a replay consumed its answer (RP-204's hole (a), replay-gate 9.6 residual 7). The
    acknowledgement is the moment the engine can no longer send the op. One rule for every ring also gives
    the port's rings a prune (they had none) and frees every ring from the journal's file, which a hold
    keeps back (RP-205).
  - *Tests.* `DurableOpRingTest` (9: the acknowledgement, an unknown answer, the settle against a bound and
    against none, the announce, the valve, an acknowledgement before the valve, a corrupt ring, a malformed
    one). Diesel `aDrawsOpLeavesTheOilRingAtTheEnginesAcknowledgement`; create
    `aPortOpLeavesItsRingAtTheEnginesAcknowledgement` (the ring saved with its answer and generation, read
    back in the session, then pruned at the acknowledgement; the next save holds none of its keys). The
    crash GameTests (Diesel's three, Destroy's four, the port's eleven) hold the prunes
    (`holdPrunesForTest`): the scripted engine acknowledges an answer once its own save holds it, and the
    emulated crash then sends the op again, which a real engine does not.
  - *Left.* An entry whose answer is unknown (a settle that proves nothing, the announce, a corrupt ring,
    or an op a postbox's chunk was found to carry, `PortRing.refreshed`) stays until its op's resend or
    the valve; it can only refuse that op. 4,096 unacknowledged ops in one ring push the oldest out,
    counted.
- **RP-285, an industry answer heard with the module off (slice U, SEC-1's option A; amends U10's industry
  rule, IR15 in docs/choices-indrework.md).**
  - *Chosen.* With `Module.INDUSTRY` off, `industry::journaled` still takes a COMMAND_RESULT for an
    industry op; every other industry record is still ignored (U10). The kept command leaves the outbox
    (`Link::complete`), or the queue that the switch or a session start with the module off moved it back
    to (`Link::unqueue`, new; `set_enabled`, `write_resends`). Its book effect lands at once, at the
    engine's frozen time (`engine.now()`, as `dropped` does), so the answer never runs the engine's clock:
    FAILED takes the exchange back (`command_failed`), DUPLICATE_OP drops it (`command_duplicate`,
    RP-199), and APPLIED drops a station exchange too, since its IND_CARRIAGE_RESULT, journaled just
    before it, was ignored if the switch came between, and none will come. `command_duplicate` only
    removes an entry of the engine's exchanges, which hold carriage deltas alone (rtsim-industry
    `engine.rs`, the station exchange and the loot), so APPLIED for a deposit delta, a package or a chain
    write books nothing. A REASSERT copy of a carried answer takes the same path, and so does an answer
    heard again (`answered_again`). While on, `command_result` also takes a kept command out of the queue.
    `set_enabled` still sends the whole outbox again under new sequences once on: what is left there has
    no answer from Java.
  - *Why.* Java prunes its op rings and trims its journal at the engine's acknowledgement of a live
    answer whatever the switch (RP-209, RP-257). An answer that reached the engine with the module off was
    acknowledged and dropped, its command kept in the outbox, and `set_enabled` sent it again once on: a
    second application of an op Java no longer knew. Answers do reach an engine that is off: the switch
    comes in a hand-over's preamble (MODULES) ahead of the journal's drain, so every answer not drained
    yet lands behind it, and the core journals a loaded chunk's carried answers again (REASSERT,
    `RtsimSession.reassert`) whatever the industry module, then drops them at the acknowledgement.
    Applying the effect at once holds across a crash while off: `take_save` calls `industry::save` with
    the module off (the engine's changed pages, and the link page with its outbox and queue), and
    `Link::load` reads them whatever the switch, so the pages that cover the answer hold its effect.
  - *Cost.* An IND_CARRIAGE_RESULT heard while off is still ignored: an exchange whose result came in that
    window keeps no shortfall correction (IR15's cost), and is now dropped at its APPLIED instead of
    staying pending until a resend's DUPLICATE_OP. A chain write's CHAIN_IN heard while off is ignored the
    same way, so its store's write stays unanswered (SC113), as it did before.
  - *Tests.* rtsim-jni `tests/industry.rs`, module `ack_never_resends`:
    `an_answer_that_reached_the_engine_with_industry_off_is_never_sent_again_after_its_ack` (the switch in
    the tick of a station exchange's APPLIED; failing first at b5f3c55f, the exchange sent again after
    the acknowledgement past its answer); `a_failed_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off`;
    `a_duplicate_answer_heard_with_industry_off_is_booked_once_across_a_crash_while_off` (a J3
    re-derivation refused live in the tick industry goes off);
    `a_carried_answer_heard_with_industry_off_takes_its_queued_command` (a chunk's carried APPLIED for an
    ordered package write, journaled again as a REASSERT in a session that starts off). The three crash
    cases restart off on a minimum-size buffer, the replay over at least two hand-overs. Against the link
    before the fix each fails: FAILED leaves the world's books short (the exchange never taken back, the
    command trimmed from the outbox at the restart), DUPLICATE_OP leaves the exchange pending for ever,
    and the carried package is sent again after the acknowledgement past its copy.
  - *Left.* No test answers a deposit delta or a chain write while off on a running line (the iron line
    decides only exchanges, loot and reconciles); the carried case covers an ordered package write.

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
   Options: (a) keep it, and watch the log at OmegaSrv's first start for "C2ME's chunk storage is not the one
   this was written against" (recommended); (b) `-Dcom.ishland.c2me.chunkio.syncDiskWrites=true` on OmegaSrv
   so C2ME writes with DSYNC like vanilla and the barrier fsyncs nothing; (c) drop C2ME from the pack.
3. **No early retry of a held journal write (JB6).** Options: (a) none, the next autosave writes it
   (recommended); (b) one retry a few seconds after a hold caused by C2ME's unload window, which shortens the
   window in which a crash starts with the pages ahead of the world save (P29) from up to 5 minutes to seconds.
   *Superseded in part by RP-205 (slice U):* a flush save (the stop) retries once after every chunk is
   flushed; an autosave's hold still waits for the next autosave.
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
  - What main's restart did with the store gone, now documented by four rtsim-industry
    `tests/chain_replay.rs` cases on an engine restored without its store pages (`without_stores`). A
    write in flight was written again under a new op once Java declared the store again, its cluster's
    pool restored from its own pages: a pool of one package is written twice. A debt held only in the
    store's `owed` was forgotten. A drawn package (SC141) whose store the engine forgot is neither
    removed nor given back: Java keeps it circling, its goods already the far district's, and a later
    landing is not taken back from the replay, so it becomes a second copy.
  - Guard: rtsim-jni `tests/tables.rs` checks every page table the industry, ecology, herds and conflict
    crates define or key inline against the list that loads it (one allowlisted meta table, conflict's
    0x4300, read by key), rtsim-view's `INDUSTRY_TABLES` against the link's, and every page an industry
    snapshot writes (all six tables reached). Failing first with STORE_TABLE left out.
  - Residual: the same two behaviours whenever the engine meets a store it does not know, which after
    this fix means an industry engine that starts afresh (its pages failed to decode, W23) or an industry
    reset. A fix slice after U-e decides what the engine does with unknown drawn serials and with writes
    in flight to a store declared anew.
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
- *Amended by slice U-e (RP-290).* The switch is a journaled record, so these tests changed shape, not
  intent. `Rig::switch_pick` (tests/industry.rs) journals the MODULE_SWITCH ahead of the tick's answers
  (the tick's records are what follow it), where the old helpers called `Java::modules(0)` after the
  answers and relied on MODULES riding ahead of the hand-over's records. The crash cases' replays now
  apply that switch where it is (pages saved before it hold the module on, so the replay runs the answers
  off, keeps them, and the restart's own config is a no-op or the session's own switch). The A/B
  `a_restart_with_industry_off_books_its_replayed_answers_as_a_restart_with_it_on` gained its third arm,
  N, the same journal with the switch patched to the mask industry was already in ("a run that never had
  the window"): N, a restart on (the replay applies the switch off, the session's own switch-on books the
  answers) and a restart off switched on later all end with the same pages, clock and op ids.
  The duplicate-answer tests (case 5, `a_duplicate_restarted_off` in `ack_never_resends`) take their off
  window from session 1, which switches industry off in the tick the world refuses X (`Rig::switch_pick`
  set before X is sent, the world answering in the tick X arrives): `Window::Off` (off through the crash,
  the restart's config off is the same mask) and `Window::OffThenOn` (a gap of records longer than a
  minimum-size buffer, the switch on, more records: the switch-on is heard inside the replay, in a later
  hand-over than S2's, and books the kept S2 as replayed). The twice test replays S1 on and S2 off.
  `a_station_exchange_whose_trains_save_was_lost_is_applied_again_after_a_restart_with_industry_off` does
  the same through `Java::switch_ahead_of_result_of`. `a_failed_exchange_restarted_with_industry_off`'s A
  arm now decides X again before the window's switch, as B does (it no longer ignores the arrivals). The
  pages-P1 wait of the case-5 helper takes the step with ticks and waits for the IO lane with hand-overs
  only (`Rig::until_durable`): 400 ticks were fewer than a loaded disk needed.
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
    and predates this record; found here, and fixed in slice U-c (its part 3).

## Slice U-b

Branch `fix/slice-u-b`, 2026-10-25. Holders saved ahead of the journal: a chunk or an entity section on disk
newer than the journal and the SavedData (the slice U audit's rows C1, C3, C4, I2 and I1). The rule is
docs/boundary.md's "lose, never double". Each row has a failing-first test: the Rust protocol test for C1,
and `SavedAheadGameTests` (core, batch `mic_rtsim_saved_ahead`) for the rest. The GameTests emulate the
crash on live state: what it keeps stays, what it rolls back is undone by hand, and the engine's resend is
ordered again under the same op id. Direction A is the journal on disk with the store from before; B is the
store on disk with the journal from before.

| Id | Choice | Why, and what is left |
|---|---|---|
| RP-210 | **C1, dragon tribute: Java answers a TRANSFER_TO_CONTAINER whose chunk is not loaded NOT_LOADED (COMMAND_RESULT status 6), and the dragons keep that stack placing under its op id.** The stack is parked (per session, not saved) so it is not resent every step. The lair's next LAIR_CHEST sends it again, to the chest it went to first. FAILED comes with nothing placed: before the chunk is looked at (no such level, an unknown item, a count below 1), or with the chunk loaded and not carrying the op. A stack answered NOT_LOADED forgets its lair's chest only if it is still the stack's own; a chest the lair reported since, elsewhere, is kept. Tests: `dragons::a_tribute_stack_in_a_chunk_saved_ahead_of_the_journal_is_placed_once` (crates/rtsim-jni/tests/protocol.rs) failed first with the coin in the chest twice (998 for 499), through the fake's answer: the fake, as Java did, answered an unloaded chunk FAILED. The engine's half is `dragons::a_tribute_stack_whose_chunk_is_not_loaded_waits_under_its_op_id_for_its_own_chest`, written with the fix. Run against the engine before it (engine.rs, dragons.rs and world.rs of 1844a8cd, with the fix's fake and wire.rs), it fails at "the chest is forgotten until reported again", and with that check off at "parked: not sent again every step" (67 commands for 35: the stacks sent again every step). `an_unloaded_stack_forgets_only_its_own_chest` (crates/rtsim-ecology/tests/tribute.rs) failed first with the lair's newer chest forgotten (None for the chest). | The audit's uncertainty 6.2 is real. FAILED made the dragons put the coin back to wait, and they sent it again under a new op id. Meanwhile the chest's chunk, written ahead of the journal, could already hold the stack and carry its APPLIED record. Its REASSERT came in as new, since no op was registered for FAILED. The other option was for the engine to keep FAILED ops and book a later APPLIED copy as placed. It was rejected because a re-send under a new op may already be out when that copy comes. With the op kept, nothing new is sent. At the chunk's load its REASSERT books the stack placed, and it comes before the chest report that would resend it. A resend under the same op finds the record and answers DUPLICATE. **Left (found at the merge with M9; done as MZB-760, docs/choices-mowzie-build.md):** the Umvuthana grove's tribute (MZB-423) came to main after this row and knew only FAILED for an unloaded chest. The answer takes the op out of the core outbox, so the grove ignoring NOT_LOADED left the stack in flight with nothing to answer it, and the grove sent no more. The merge took NOT_LOADED as FAILED, as main answered it: the stack went back to its stock and went again under a new op once its chest was reported. So a grove stack in a chunk saved ahead of the journal could still land twice, as on main before this row. MZB-760 parks it under its op id, as the dragons do. Test of the merge's fix: `conflict::a_groves_tribute_stack_answered_not_loaded_goes_back_to_its_stock` (crates/rtsim-jni/tests/protocol.rs), which failed first with the stack left in flight ((567, 1, 1) for (567, 0, 1)); MZB-760 reworks it into the parking test. **Outcomes:** A EXACT. B EXACT (was DUPE). After a give-up, LOSS (the engine took APPLIED, and the chunk from before has no coin). |
| RP-211 | **C3, PLACE_HOUSE: every loaded chunk of the house's box carries the APPLIED result and is noted with the barrier, and a build that searches first looks for its op on every candidate plot.** Before, only the origin chunk carried it. A resend searches again from candidate 0. Before it builds anywhere, it looks across all the candidate plots' chunks: loaded ones by `chunkCarriesOp`, the others as saved, read off the server thread (`ChunkMap.read`, the attachments parsed by `RtsimSession.opsCarriedInChunkTag`, the smallest form of U-a's `Industry.opsCarriedInChunkTag`). The op found anywhere answers DUPLICATE_OP and builds nothing. The build waits for the reads at most 10 s (`PLOT_READ_WAIT_NANOS`, PLACEHOLDER), then searches without the chunks not read, logged and counted (`/rtsim status`, refused: plot reads, found by look, looks cut). Tests: `aHouseWhoseOtherChunkIsSavedAheadIsNotBuiltTwice`, `aHouseWhosePlotIsOnlyOnDiskIsNotBuiltTwice`, `aHouseIsNotBuiltTwiceOnAnEarlierPlotFreeAtTheResend`. | A box chunk on disk ahead of the origin chunk, the villages and the journal held half a house that no record spoke for. The resend searched again, found the plot taken and built a second house beside it. It failed first with 8 wall blocks outside the first house. The search already looks for the op in every chunk of the box (`carriesOp`), so the resend now answers DUPLICATE_OP. But `carriesOp` sees only the candidate in hand, and the search skips unloaded candidates and builds at the first free loaded one (the review's P1). With the first house's plot unloaded at the resend (its chunk on disk only), or an earlier plot free at the resend, the resend built a second house. Both tests failed first (1b3ffef5, on the stub): with the first house's plot on disk only, the resend answered APPLIED and built a second house in L (8 wall blocks); with an earlier plot free, it answered APPLIED with 8 wall blocks in E. The look runs once per build and session (not saved; after a reload it looks again). It costs one off-thread read per unloaded candidate chunk of a searching build, a few dozen at the engine's radius of 48, and keeps only each chunk's attachments. The copies are one record, and the engine takes a repeated result by state (`done_ops`, and the outbox and queue are filtered by op). **Left:** the half house. In B the origin chunk is from before and the villages have no House, so the half in the newer chunks stands while the engine has the op done and no HOUSE_PLACED. That is a LOSS, not a second house. The look reads an unloaded candidate as last saved: a chunk still in memory below FULL whose house is not saved yet reads without it. Within a session the villages' House (`houseOwner`) and the journal's applied ops answer such a resend before any search; after a restart, memory and disk agree. A candidate chunk not read within 10 s is searched past: a second house is possible on a plot there, logged and counted. The audit's C3 (a), the villages written before the chunks on the IO chain, is unchanged (LOSS). **Outcomes:** A EXACT. B LOSS (was DUPE). After a give-up, LOSS (the house missing in the chunks not on disk). |
| RP-212 | **C4, SPAWN_RESIDENT, in four parts.** (1) Before spawning, the order waits until the entities of every chunk `standNear` can reach (5 blocks) are loaded. `isPositionEntityTicking` does not imply them. (2) Every tick, not only at the order's next try, an order whose op the journal holds as applied is answered DUPLICATE_OP. (3) The resident carries its result as a noted holder (`RtsimSession.carryOnHolder`, `dependsOnHolder` as camps do). (4) Backstop: the resident's UUID is `nameUUIDFromBytes("mic_rtsim:resident:" + engine life + ":" + op)`. A UUID the entity manager knows (hidden entities too) answers DUPLICATE_OP. Test: `aResidentWhoseEntityChunkIsSavedAheadIsNotSpawnedTwice`. It passes if any one of the four parts works: the parts have no switches of their own, so no test shows each part needed. No test covers a resident that walked out of reach before it was stored (part 4's case). | The resident's entity section is written by its own IO worker, apart from the spawn chunk. One on disk ahead of the spawn chunk and the journal let the resend spawn a second resident: it failed first with 2 golems. The brief's three fixes were not enough against the code. A resident that loads REASSERTs its record, and the journal then holds the op as applied. But once the engine acknowledges the copy, `trim` forgets the op again and the resident drops its record. An order checked only at its next try (every 40 ticks) could miss the window. A resident that loads reasserts in the Post tick before the spawner runs in that tick's `endOfTick`, so a check every tick cannot miss it. The UUID covers a resident that walked out of reach before it was stored. If it is known, the answer is DUPLICATE_OP. If it loads only after a second was spawned, vanilla refuses it as a known UUID ("UUID of added entity already exists") while the second is loaded, and the next save drops it: a lost resident, never two at once. A new engine life numbers its ops afresh and gets other UUIDs. **Left:** that walked-away resident and its second can both exist while never loaded together, and the first time both load, one is lost. `addFreshEntity` refused by another mod's join cancel still answers FAILED. **Outcomes:** A EXACT. B EXACT (was DUPE), with the walked-away corner a LOSS. After a give-up, LOSS. |
| RP-213 | **I2, a reconcile's block breaks: the drops join the level only once the chunks the breaks changed are on disk.** While a reconcile breaks blocks, a HIGHEST `EntityJoinLevelEvent` listener takes every fresh ItemEntity joining that level. It cancels the join and marks the item ported then, in its break's tick. After the epoch is set, `ChunkWriteBarrier.writeNow` writes the reconcile's chunk and the chunks of the other blocks its breaks removed. It saves each through vanilla's `ChunkMap.save`, has the storage write them ahead of its queue as a round does, and forces the region files to disk on the IO pool. Once that has landed, each drop joins the level when its own chunk's entities are loaded. Until its batch ends, a region ticket of its own (`mic_rtsim_drops`, FULL, one per batch, a 6000-tick timeout as a backstop, PLACEHOLDER) holds the chunks its drops lie in loaded, their entities with them. A write that fails or has not landed within 60 s (`WRITE_NOW_MILLIS`, PLACEHOLDER) loses the batch, logged and counted. So does a stop while the drops wait: they are held in memory only, and the session's close counts them as lost and logs them once (`/rtsim district perf`: drops kept, lost, waiting). A level that does not save, or whose writes cannot be ordered, gets them at once, as a round treats it. Tests: `aReconcileThatBreaksAChestDropsItsContentsOnce`; `aReconcilesHeldDropsKeepTheirChunkLoadedUntilTheyJoin` (a test hook holds the drops while their chunk is unforced: the chunk stays loaded with its entities, and the drops join once let go), which failed first (1b3ffef5, on the stub) with C and its entities not loaded while the drops were held. The create project's `aBrokenBlockDropsItsContents` waits for its drops in wall time (paced, at least 60 s): with its 400-tick budget it failed at d576618a, the unthrottled GameTest server running the 400 ticks inside one second, before the write landed (no loss logged). No test stops a server while drops are held. | Drops are item entities, stored in `entities/` apart from their chunk. A drops section on disk with the chunk and the journal from before brought the chest back without the epoch, and the resent reconcile broke it again (JB9's R24). It failed first with 200 diamonds and 2 chests for 100 and 1. Of the brief's options, noting the drops by holder fixes A only: the journal waits for them, but B is the drops ahead of the chunk. An epoch on the drops needs their entities loaded before the reconcile applies at a chunk load, and it leaks: a drop picked up, merged into another stack or despawned takes its epoch with it. Deterministic UUIDs leak through merges in the same way. Holding the drops back makes B impossible, since no drop exists before its chunk is on disk. It costs one chunk save and a targeted write per reconcile that breaks something (damage only, rare), off the server thread but for the serialization. **Left:** the drops are a LOSS on a write failure, or on a stop in the window. The window runs from the break until the write lands, at most 60 s (`WRITE_NOW_MILLIS`), and then until the chunk's entities are loaded, which the ticket brings about (a reconcile soon after a chunk's load can break before the chunk's entities are in). At a stop the world's last save comes before the session's close: the chunks are saved with the blocks broken and the epoch set, the held drops are in no save, and the close counts and logs them. If the ticket's timeout ends first, the drops wait in memory for the chunk to load again. Create's repair schematic (row K8) is written 100 ticks after the break, which in practice is after the chunk is on disk, though nothing orders it. **Outcomes:** A EXACT. B EXACT, or the drops a LOSS if their section was not stored yet (was DUPE). After a give-up, the block is back while booked destroyed, and the drops never joined: no DUPE (was DUPE with the drops on disk). |
| RP-214 | **PORTED is saved with the drop when true** (`serialize(Codec.BOOL, b -> b)`). Test: `aPortedDropKeepsItsMarkThroughASave`. | This settles the audit's uncertainty 6.5: yes, reloaded drops were counted again. PORTED had no serializer, so a drop that unloaded and loaded again (any unload, not only a crash) lost its mark. A player picking it up in a district was then booked as PLAYER_PICKUP: a second outflow for goods the break or the reconcile already booked as a port. The engine's learner reads net production as stock minus cumulative ports (learn.rs), so the extra outflow reads as phantom production. No goods were doubled in the world. A world without rtsim drops the unknown attachment and keeps the item (UR-1). **Left:** when a ported drop and an unmarked one merge, the merged stack keeps the survivor's mark, so a pickup is booked one way or the other for both: a port miscounted, never goods doubled. |
| RP-215 | **I1, a double chest across a chunk border: verified here, fixed in slice U-f** (below): each half of a vanilla double chest across a chunk border is its own inventory of 27 slots. Test, slice U-b's first and then slice U-f's: `aReconcileIntoASplitHalfWritesOnlyThatChunk` (it was `aReconcileThroughADoubleChestAcrossAChunkBorder`). P, the LEFT half (facing north), is at chunk A's east edge, with room for 24 more; Q, the RIGHT half, is at N's west edge, empty. A reconcile of A adds diamonds at P. | Recorded on the pair's item handler: it put the goods into Q, in N. N got no epoch and no note. With N kept and A (no epoch) and the journal from before, the resend put 20 more into Q (40 for 20): **direction B with a positive delta is a DUPE**, as the audit inferred, and a negative delta took the goods twice from Q (0 left of 40 for a booked 20), a loss. The audit gave two fixes: carry and note the reconcile in both halves' chunks, or reconcile each half by its own position. The coordinator chose the first, and slice U-f's first two rounds built it (109caec5 to c985bac5); three review rounds found it inexact for a change split across both halves, for a crash that saves one chunk and not the other, and for a command held across the other chunk's acknowledgement. The coordinator then chose the second, for a vanilla chest across a border: the halves in one chunk stay one inventory. |

The entity half of JB9's R24 (a reconcile's spilled items, residents) is now covered by RP-212 and RP-213.

## Slice U-g: a failed SavedData write holds the journal's file

Branch `fix/slice-u-g` (on slice U-b), 2026-10-26. The hole is RP-317 of docs/choices-mowzie-build.md, found by the
Mowzie lead on `fix/replay-industry-ids` (slice R's branch, which calls it residual 10 of docs/replay-gate.md 9.6; what
this slice leaves is residual 11, so that R's number stays R's).
RP-200 wrote the journal's file last in a world save so that a journal on disk implies every SavedData file of its
save is on disk, "always direction B". That holds only while every write succeeds. Vanilla's `SavedData.save(File,
...)` clears the data's dirty flag before the write runs, and the write, queued on NeoForge's IO chain, catches an
`IOException`, logs it and goes on. A `create_tracks` write that fails while the journal's succeeds, followed by a crash
before the next good tracks write, leaves every train op the restored pages decided gone from the train and on the
books: direction A. This slice turns that A into B, which the rings make exact, by holding the journal's file as it holds
it for a chunk (RP-205). The failing-first evidence is in the tests below. Direction A is the journal on disk with the
store from before; B is the store on disk with the journal from before.

| Id | Choice | Why, and what is left |
|---|---|---|
| U-g1 | **A failed write of a journal-dependent SavedData file holds the journal's file at its last good state until that data writes.** The journal's write task (`JournalSavedData.writeWhenLanded`, last on the IO chain, so after every SavedData write of its save) asks `SavedDataWrites.firstFailing(dirs)` once the chunk barrier has confirmed, and before it writes. `dirs` is `WorldSaveOrder.dataDirs`: the `data/` directory of every level of that world save. A hit keeps the file dirty (`retry`), counts the hold (`savedDataHeld`, in `/rtsim perf`), logs once per streak and returns: no write, no wait, and no round limit of RP-205's kind; the file's own limit is U-g12's give-up. The question comes after a confirmed chunk-barrier round, so that round's notes are dropped though the journal's file did not move: safe, because those chunks are on disk (the barrier's wait is for them to be flushed and fsynced), and the next file's round starts from the notes made since. The next world save marks the failed data dirty again and writes it first (U-g4); once it writes, the same save writes the journal's file. | A, the journal on disk ahead of a file that lacks the effect, becomes B: the file on disk is W0's, the one before the failed save, and every ring in W0's files is from the same save. **Rejected:** (a) writing the data again at once on the IO chain: a full disk fails again, and it is a save path outside vanilla's (JB6's argument against an early retry); (b) reading each file back before the journal's write: it needs every file's name and format, and the failure is already known; (c) stopping the server on a failed write: not undoable, and Ben's to decide. The marker (WORLD_SAVED) reports `min(through, writtenThrough)` and the file did not move, so the engine is told the save is durable only through W0's observations: the rings are not pruned past what is on disk. |
| U-g2 | **The target is `SavedData.lambda$save$0(CompoundTag, File)`; the wrap is a MixinExtras `@WrapOperation` on its one call to `IOUtilities.writeNbtCompressed(CompoundTag, Path)`** (`SavedDataWriteMixin`, PATCHES.md row 95). The wrap runs the call and, for a `JournalDependent` data only, records a throw in `SavedDataWrites` and rethrows it unchanged (Java's precise rethrow: the Operation cannot name the `IOException`, so the catch is `Throwable` and the rethrow keeps the checked type), so vanilla still logs its `Could not save data` line. A return ends the file's streak. | Evidence: `javap -p -c` of the patched `SavedData` in NeoForge 21.1.249's userdev jar (the patch, `SavedData.java.patch`, is the same as 21.1.230's): `save(File, HolderLookup.Provider)` builds the tag, copies it, queues the lambda on `IOUtilities.withIOWorker` and calls `setDirty(false)`, all before the write; the lambda is the write inside `catch (IOException)`. **Rejected:** wrapping `writeNbtCompressed` for every caller (the journal's own file, schematics, other mods: one `instanceof` is cheaper than ever being wrong about whose write it was); an inject into `save(File, ...)` (the server thread, before the write: it knows nothing of the outcome). **Fragile in the lambda's number:** a NeoForge that adds a lambda ahead of it moves the name. The config is `defaultRequire 1`, so the launch fails, which is the contract test (PATCHES.md "When a version moves"). |
| U-g3 | **Which data counts: a marker interface, `JournalDependent` (`void mic_rtsim$remark()`), on Create's `RailwaySavedData` and on our own SavedData files.** Create's, by a mixin in the Create plugin (`RailwaySavedDataMixin`, row 96: only the interface and its one method, `setDirty()`; the core names no pack mod, DECISIONS.md D4). Ours, with `implements`: core `IndustryData`, `VillageRegistry` and `BaseTracker` (the dedup of BASE MARKED and UNMARKED; found by review, U-g11), create `RepairFiles`, destroy `PollutionLedger`, powergrid `PowerLinks.Links` (the dedup of IND_POWER_LINK, in its own plugin, D4; found by review, U-g11), Ice and Fire `LairPlacer.Data` and `Nests.Data`. Nothing else is touched: any other SavedData and the journal's own file (which has its own retry) are written as before. | These are the files the audit's rows name as holding an op's effect or its request: the tracks (K1, K2), the village registry (C3, C4), the repair files (K8), the pollution ledger (I5), the lair and nest requests (D1, D3), and the district index the engine's IND_DISTRICT records come from, the base marks (BASE MARKED and UNMARKED) and the power links (IND_POWER_LINK). The scope is deliberate: a hold on every SavedData in the world would stop the journal's file for any mod's failing file. **Left:** Diesel's `cdg_oil_chunks` (I4) and NeoForge's `neoforge_data_attachments` (Destroy's level pollution, I5, and the rings of every plugin) are not marked: their failed writes are still A (U-g10, 2). |
| U-g4 | **The failed data is marked dirty again at the start of the next world save, on the server thread** (`WorldSaveOrder.begin` calls `SavedDataWrites.remarkAll`, before any level's save): every still-failing entry's data gets `mic_rtsim$remark()`. A world save marks it again until the file is written. | Vanilla cleared the flag before the write, so without a mark the data would not be written again until something changed it: a tracks file with no train moving could stay missing the effect for ever while the journal waits for it. **Not from the IO thread:** `setDirty()` there races the server's own `setDirty(false)` of the next save, and a mark made between a save's tag build and its clear is lost. Marked at the save's start, the same save writes the data, then the journal. |
| U-g5 | **Bounded: one entry per failing file, the data held weakly, never more than one per journal-dependent file in the world.** An entry whose data was dropped (its level unloaded) cannot be written again: the next `remarkAll` abandons it, logged once at error ("is gone"), counted (`abandoned`), and the hold ends. The failure count is a counter, not a list. | The pack runs for weeks (D1: late-stage performance on a weak PC): 10,000 failed writes of one file are one entry (`SavedDataWritesTest.theRecordNeverGrowsPastOneEntryPerFile`). Holding for a file that can never be written would freeze the journal's file for ever; abandoning it goes on without it: A for that file (U-g10, 4). A file that is still there and fails at every save is the other way to freeze it: that is U-g12's give-up, so that no hold is unbounded in time either (the engine replays everything since the journal's file, and that grows with the hold). |
| U-g6 | **The server thread never blocks on it.** `failed` and `written` run on the IO chain, `remarkAll` on the server thread at a save's start, all under one lock held for a few map operations; a write of a file with nothing failing is one int read. The hold itself is a `return` on the IO chain, behind the SavedData writes, which already is where the journal's write waits for chunks. | The journal's write task is the one place that already waits; nothing is added to a tick. |
| U-g7 | **A flush save does not try a SavedData hold a second time.** The hold sets no `heldLast`, so `retryAfterFlush` (the second try after the chunks are flushed, RP-205) does not run for it: the stop, or `/save-all flush`, keeps the older file, said by the streak's one error line (U-g8: a stop that begins the streak says it there; one that joins a streak already logged adds no line). | The second try exists because flushing the chunks changes what the barrier finds. Nothing a flush does mends a file whose write failed, and a second write path on a failing disk is JB6's objection again. |
| U-g8 | **Logged once per streak, counted after.** The first failure of a file: `SavedDataWrites`, at error, naming the file and the exception (vanilla's own line is logged for every failure, as before). The first hold of a streak: the journal, at error, with the lag in applied ops. The end of the streak: info, from both (the journal's line says no failed SavedData write holds it any more: the files were written again, or dropped). Later failures and holds are counted (`/rtsim perf`: "by a failed SavedData write N in S streak(s) (failures, recovered, remarked, abandoned)"); the journal's "applied ops behind" warning names the hold while one is on. A log sink that throws never reaches the IO chain (`say()` swallows a `RuntimeException`). | A full disk fails every write of every save: a line each would be a flood. NeoForge's IO chain stops at an exception thrown inside its task (an existing property of `withIOWorker`), so nothing this slice adds may throw there (`SavedDataWritesTest.aLoggerThatThrowsNeverReachesTheWrite`). |
| U-g9 | **Failure injection in tests: a directory stands where the data's file goes**, so the atomic rename throws a real `IOException` through vanilla's own path; no production hook and no wrap of anything global. `WorldSaveOrder.EXTRA_DATA_DIRS_FOR_TEST` adds a test's own directory to a world save's `dirs`, so a failing file there holds that test's journal and no other. Each GameTest has its own batch: a failing file holds every journal of its window. | Tests: core `SavedDataWriteHoldGameTests` (a Pad's data and the industry data hold the journal's file byte for byte until written; another directory, or an unmarked data, holds nothing; the give-up; the reinstall marker waits for the industry file), create `TracksWriteHoldGameTests` (a real `RailwaySavedData`, the same `save(File, ...)` vanilla calls), powergrid `PowerLinksWriteHoldGameTests` (the plugin's own `Links`), core `SavedDataWritesTest` (the record, the reset, several files, the give-up, a throwing re-mark). Failing first: commit 2d3e7cb8 has the tests and not the hold; the rounds below ran on f2395e0d, the same tests merged with main and still without the hold. **Core** (138 GameTests, `SavedDataWritesTest`'s 6 unit tests pass): 2 failed, `aFailedSavedDataWriteHoldsTheJournalsFileUntilTheDataIsWritten` and `aFailedWriteOfTheIndustryDataHoldsTheJournalsFileToo`; `aFailedWriteOfAnotherDirectoryOrAnUnmarkedDataHoldsNothing` passes (it asserts the scope, which holds with or without the hold). The log line that shows the missing hold: `W1 = 2 journal writes, held by a failed write 0, SavedData writes failed 2, failing now 1`, the journal's file written in the save whose data write failed and was recorded (the write was seen, the premise holds, the journal did not look). **Create** (247 GameTests): 1 failed, `aFailedWriteOfCreateTracksHoldsTheJournalsFileUntilItIsWritten` (so the mixin applied and the failure was seen): `W1 = 2 journal writes, held by a failed write 0, SavedData writes failed 1, failing now 1`. The reporter named a later check each time (core: "the data is written: value 4 ... (-1)"; "the industry data is written again without a change to it: false"; create: "the tracks file is written without any change to it"): `thenExecute` lets a failed step fail the test and goes on, `GameTestInfo.fail` keeps the last error, and the failed step's cleanup deletes the test's directory, so the later checks fail as a cascade. The kit now logs each step that failed, so the first failed check is in the log. The engine side is `held_journal` in crates/rtsim-jni/tests/industry.rs (U-g10, 1). |
| U-g10 | **What this leaves (residual 11 of docs/replay-gate.md 9.6; slice U-h closes (1), the pages; (2) to (5) stay open).** (1) **The engine's pages are ahead (P29). Closed by slice U-h (U-h1 to U-h3): the world save's pages are a pending generation until WORLD_SAVED, which a held journal never sends, so the restart from the older file rolls them back; both exchange cases below are applied once.** *As it stood in U-g:* The pages are requested at the start of a world save, before any write fails, and are written whatever happens to the data; after a failed tracks write and a crash the restart has the failed save's pages with the files of the save before. With the exchange in the pages' outbox (written, not yet answered at the save): exact, the resend is applied once against the tracks file's ring (`a_station_exchange_written_before_the_pages_is_applied_once_with_the_journal_held`: without the hold, slice R's J4, X's load is lost; held, the train and the books are the control's). With it answered inside the pages: lost either way, held or not (`..._answered_inside_the_pages_is_still_lost_with_the_journal_held_p29`: the same shortfall, X's load). That case is P29's: wiring the confirm on WORLD_SAVED so that a held journal keeps the older page generation (docs/choices-leg-purge.md LP12), which is Ben's. (2) **Unmarked files:** Diesel's `cdg_oil_chunks` and NeoForge's `neoforge_data_attachments` (Destroy's level pollution, the rings of every plugin). The change is small (the Diesel ring mixin gets the interface; a core mixin does the same for NeoForge's attachment SavedData) and each needs its plugin's GameTest round: not done: this slice's scope is Create's tracks and our own files. (3) **A stop with a write outstanding** keeps the older journal file (U-g7); the pages the stop saves are ahead of it (the same P29). *Closed by slice U-i (U-i1).* (4) **An abandoned entry** (U-g5) and **a file given up on** (U-g12): the journal goes on without a file that cannot be written, or that failed at the limit of world saves: A for it, logged. (5) **NeoForge's IO chain** stops at an exception inside any task of it: unchanged. | The hold only moves the journal's file; it cannot move the pages. |
| U-g11 | **The review's findings on the hold (Opus review of 32b1d1d3).** (1) **Two of our own files were not marked:** core `BaseTracker` (BASE MARKED and UNMARKED) and powergrid `PowerLinks.Links` (IND_POWER_LINK, in its own plugin, D4; its class is public for its GameTest). Both implement `JournalDependent` with the `setDirty()` re-mark; U-g3 lists them; the core GameTest `aFailedWriteOfTheIndustryDataHoldsTheJournalsFileToo` checks `BaseTracker` as it checks `VillageRegistry`, and a powergrid GameTest runs a failed write of `Links` through the hold. Every `extends SavedData` of the mod was listed: no other is unmarked. (2) **`SavedDataWrites.INSTANCE` is reset when a server stops** (`BridgeEvents.onServerStopped`: entries, counters, thread ids): one record per JVM, and an integrated server's next world must not inherit the last one's failing files. (3) **A re-mark that throws no longer aborts the world save** (`remarkAll` catches per entry, counts it, says it once for the file): it runs at the head of `saveAllChunks`. (4) **The reinstall marker waits for the industry file** (`ReinstallGuard.onOverworldSave`): the marker's deletion task, queued behind the overworld's SavedData on the IO chain, finds `SavedDataWrites.holds(<world>/data/mic_rtsim_industry.dat)` and keeps the marker (logged once per reset), trying again at the next world save. Deleted, it would leave the next start with no reset and no file that remembers what to re-adopt, the same class as RP-317. Only that file counts, not any failing file of the directory: a stray failure must not make every restart reset the sim. (5) **The give-up** is U-g12. (6) Smaller: the mixin's selector carries the descriptor (`lambda$save$0(Lnet/minecraft/nbt/CompoundTag;Ljava/io/File;)V`, javap of the patched class), the "applied ops behind" warning names the two causes separately, the GameTests' kit logs the check a step failed and cleans up every kit its batch left open in an `@AfterBatch` (a test that times out would otherwise leave its failure record holding every later journal, the server's own included), and a log4j appender asserts vanilla still logs "Could not save data" once per failed write (the wrap rethrows into its catch). | Tests, failing first at 531928a3 (they and the API stubs they call, without the behaviour): see U-g9. **Rejected:** (a) resetting at the next server's start as well (one place is enough: a stop always follows a start, and the stop's reset runs before anything of the next world); (b) a marker that waits for any failing file under `data/` (a stray failed file would make every restart reset the sim); (c) retrying a throwing re-mark at each save without a limit on the line (a line per save is the flood U-g8 avoids). |
| U-g12 | **The hold gives up on a file after 12 world saves (PLACEHOLDER; `SavedDataWrites.GIVE_UP_AFTER`).** `holdFor`, the journal's question at its write (once per world save, on the IO chain), counts one more held world save against each failing file under the save's data directories; the one whose count passes the limit stops holding, logged once at error, loudly, naming the file ("GIVING UP holding the journal's file for ...") and counted (`gaveUp` in `/rtsim perf`). The journal then writes and accepts RP-317's risk for that file until the file writes again: a crash before then can leave the ops decided since in the journal and not in that file (A for that file). The record stays, so the data is still marked dirty again at every world save; a write that succeeds ends the streak (info: "had been given up holding for it"), and a new failure holds again from its first save. The pure question `firstFailing` and the marker's `holds` leave out a file given up on. | A file that fails at every world save (a disk that stays full, a file made unwritable) would otherwise hold the journal's file at one save for ever, and the engine's replay of everything decided since would grow without end: the hold must be bounded in time as well as in size (U-g5). The limit is the coordinator's reversible call (the SEC-1 lead's, for Ben to see): at the pack's five-minute autosave 12 saves are an hour. **Rejected:** (a) no limit, accepting the growth (the review's P2); (b) a limit in wall-clock time (a world save is the unit the journal's file moves in, and a stopped server has no clock); (c) dropping the record at the give-up (the data would then never be marked again, and the file would stay lost after the disk came back). **Left:** during the give-up the journal's file moves on over a file that is missing its effect: A for it (residual 11). Tests: core `aFileThatFailsAtEverySaveStopsHoldingTheJournalAfterTheLimit` (N = 2 through `giveUpAfterForTest`: two holds, the third save goes on, once, counted; a new streak holds again) and `SavedDataWritesTest`. |

**Merged with slice R (`fix/replay-industry-ids`, RP-317, residual 10).** R landed on main first, so this
branch did the merge (its commit "Merge origin/main (slice R) into fix/slice-u-g"). R's docs and tests had
described the hole as open; what the merge changed, by file:
- docs/replay-gate.md 9.6: residual 9 stays slice U-b's. R's residual 10 "A failed SavedData write" is now
  "answered inside the pages": it keeps R's mechanism, its pinned numbers (224 crushed raw iron) and its
  inference for K2, D1 and D3, says the hold of U-g1 closes the rest for the files the journal depends on, and
  points at residual 11 for what is left (the P29 pages, the unmarked files, a stop with a write
  outstanding, an abandoned or given-up file: U-g10). R's inference for the deposit deltas (I4, I5) moved to
  residual 11's unmarked files. Slice U-h closes the pages (P29) in (1) of 11.
- Crash class 1 reads as this branch wrote it (the journal's file is held until the data has written; the Diesel
  oil file and NeoForge's level attachments are not marked yet: residual 11).
- The A columns of I4, I5, K1, K2, D1 and D3 read as this branch wrote them. K1, K2, D1 and D3 also name the one
  exception left to a marked file: an op answered inside the engine's pages (residual 10, P29), and their
  owner column named slice U-h for it, which closed it (U-h12; the rows now say so). I4 and I5 keep the unmarked files as their exception (residual 11).
- The J4 discussion in the slices section no longer says J4 "still loses a train op the restored pages
  decided"; it says a failed tracks write now holds the journal's file, and the answered case is residual 10's.
- R's two rtsim-jni tests (`tests/industry.rs`) restart the journal of the failed save (W1) and asserted the
  loss. Both now restart the held one (W0's): `a_station_exchange_written_before_the_pages_is_lost_with_its_
  trains_save` became `a_station_exchange_written_before_the_pages_is_applied_once_when_its_trains_save_fails`
  (X resent once, applied once, the books the control's: B), and `a_station_exchange_answered_before_the_pages_
  is_lost_with_its_trains_save` kept its loss as P29's case (residual 10, which points at 11). `held_journal`
  keeps its three-way comparison (the control, W1's journal, the held one) next to them. Slice U-h flipped
  the answered one to `..._is_applied_once_when_its_trains_save_fails` and the written one's mechanism with
  it (U-h12).

## Slice U-h: the engine's world save is confirmed by WORLD_SAVED (P29, LP12 option a)

Branch `fix/slice-u-h` (on slice U-g), 2026-10-26. Ben approved P29 as slice U-h, with option (a) of docs/choices-leg-purge.md
"For Ben" 2 (LP12). The engine took its pages at a world save's request and acknowledged them at the commit, whether or not the
journal's file ever landed (a held write, U-g1; a crash between the two). After a crash from the older file the pages were
ahead of the world (`EngineAhead`, logged). Now a world save's pages are a pending generation, confirmed by the bridge's
WORLD_SAVED (which reports `min(through, writtenThrough)`), and a start opens the generation the journal's file holds. The
failing-first record is in the commit log (`3c788470`: 13 of the 15 new tests failed on the old engine, F2 only on the new `confirms` counter (its acknowledgement behaviour already held); F4 and F5 guard).

| Id | Choice | Why, and what it costs |
|---|---|---|
| U-h1 | **A world save is a pending generation (`SaveKind::World`, `commit_pending`); WORLD_SAVED confirms it on the IO lane (`confirm_through`).** `durable` (the acknowledgement in the next header), `saves_done` and the archive's confirmed serial (so compaction) move at the confirm, not the commit; the confirm and the hold's flag are set before the save's tickets go back to the step (`job.succeeded()`), so the step that drains them sees both. The confirm job runs on the IO lane in order with the saves, not through `spawn_io`: a refusal is no save failure, and the next commit's confirm covers it. | Java must never trim its journal past a generation a rollback may cut. One fsync'd Confirm record a world save on the IO lane, never on the step thread; the superseded pages of the confirmed generation stay until the confirm. Tests: `confirm.rs` F1 (`a_world_save_is_acknowledged_only_after_its_world_saved`), F9 (the viewer and the fork), F10 (the archive). |
| U-h2 | **The engine remembers the highest WORLD_SAVED it has heard (`world_saved`), and tries a confirm after every commit.** WORLD_SAVED is handled in `observe`, before the step takes its save, so it reaches the engine before the save it covers when both ride one hand-over (the headless host always does), or many hand-overs before it when the engine lags the journal. At start the memory is the world's `nextObsSeq - 1`: the journal's file holds that. | Confirming only "when WORLD_SAVED arrives" would miss both. Tests: F2 (`a_world_saved_ahead_of_its_save_confirms_it_once_committed`, the same hand-over and a lagging engine). |
| U-h3 | **`start` selects by the journal: `nStart` and `Engine.start` take `worldNextObsSeq` (the signature change; Ben's log, DECISIONS.md "P29 (built, slice U-h)"), and the store calls `select_for_journal(worldNextObsSeq - 1)` before anything reads a page.** A generation the journal's file holds is kept and confirmed; one past it is rolled back, and the journal replays it. The selection is before the meta page, the modules' loads and the archive's cut, so everything loads from, and the archive cuts above, the selected generation. **Both generations ahead of the journal** (a world older than its pages): today's answer, not a refusal: what is pending rolls back, the confirmed generation stays, a warning names it, and the replay skips what the pages have (`EngineAhead`, U-g's text of the log line reworded). SESSION_START already carried `worldNextObsSeq`, but after `start` had loaded every module. | A `?` on `select_for_journal`'s error would refuse a start the engine opened before: a regression. A world that is behind its pages is a backup restored over a newer rtsim folder, which the engine has always opened. Tests: F3 (`a_save_whose_journal_never_landed_is_rolled_back_at_start`), F4 (kept and confirmed; guard), F5 (both ahead still opens; guard), F11 (the archive cuts a rolled-back world save's chunks), F12 (`an_autosave_whose_world_saved_never_came_is_rolled_back_and_the_world_conserves`), `held_journal` (below). |
| U-h4 | **A module's budget save waits while a world save waits for its WORLD_SAVED** (option (a)). The store keeps the current and the last confirmed generation and only the current can be confirmed, so a budget generation on top could outrun the journal's file and leave the world save unconfirmable. While held, `enforce_budget` still evicts saved row pages; rows whose pages all changed since the last save stay resident over their budget, as they do while a save is in flight (PF15). `budget_saves_held` counts the steps that wanted one. **Measured** (F6: 128 herds of 20 sheep under a 4 KiB row budget, 80 hand-overs of 1,200 ticks, every herd moving each hand-over): rows 191,730 B at the peak and at the end while held, against 192,162 B at the peak and 1,400 B at the end with budget saves running; the unpaged footprint is 203,970 B. The peak is the same (a first day dirties every row); what the hold costs is the rows staying resident: here all of them, bounded by the unpaged footprint (`held_peak <= unpaged` is asserted). In play the hold lasts one Minecraft world save's WORLD_SAVED, seconds. | D1: a held row page is memory a weak PC does not have for long: bounded by what changes between two world saves (and, past the valve, by 60 s). The alternatives were a kernel change (a third generation, LP12 (c)) or a confirm that lets a later save cover a pressure save (LP12 (b), which never confirms under steady pressure). Test: F6 (`a_budget_save_waits_for_the_world_saved_of_the_save_under_it`). |
| U-h5 | **The valve: `BUDGET_HOLD_HANDOVERS = 1_200` (PLACEHOLDER: 60 s at 20 TPS, a fifth of an autosave interval; no measurement behind it).** A step that begins its 1,201st hand-over in a row waiting for a WORLD_SAVED lets the budget save through, and logs once per streak (`window_grows`: "a world save is not confirmed: no WORLD_SAVED in 1200 hand-overs"). The budget save on top ends the wait (`awaiting` is the current generation being an unconfirmed world save); the next world save's WORLD_SAVED confirms both. Step-based only: a stalled server counts nothing. | A WORLD_SAVED that never comes (a held journal for a long time, U-g1: a hold has no round limit) must not keep the rows over their budget for good. The line is the replay window growing: the journal past the last confirmed save. Test: F7 (`a_world_saved_that_never_comes_lets_the_budget_save_after_the_hold`: held inside it, one line and a save past it, the next world save confirms everything). |
| U-h6 | **A budget save is a pending generation too, and is confirmed if the journal's file already holds its observations** (`SaveKind::Budget`, `confirm_locked` after its commit). In a quiet world budget saves share the world save's observations, so they are confirmed at once; one with observations past the file stays pending, never acknowledged. Consequence: **a start keeps a budget generation the journal holds** (it used to roll back every pending generation), so the replay window after a crash is from the last kept generation, not the last world save. | The journal holds what the generation holds; rolling it back would replay what is already in the pages. The window-length tests (`a_long_replay_takes_no_budget_save_before_its_end`, the herds' and the conflict's replay peaks) now make their window with a live session that takes no budget save; the rollback is tested with a journal file the budget generations are past (`a_budget_save_past_the_journal_file_is_rolled_back_at_start`, `a_rolled_back_pressure_save_takes_its_chunks_with_it`) and the kept case beside it (`a_kept_pressure_save_keeps_its_chunks`). |
| U-h7 | **SaveAtNextStep stays a save committed confirmed at once (`SaveKind::Script`).** Production never sends it; the GameTests, the stub bridge and 26 protocol-test callers (`save_durable`) do, and their crash-split semantics rely on the acknowledgement at the commit. A script save commits with every generation under it (`saves_done` gains the world saves it covers). | A save with no WORLD_SAVED behind it has nothing to wait for. Changing every GameTest that waits on its acknowledgement to send a world save buys no coverage. |
| U-h8 | **`close(save = true)` confirms its final save at once, from one place (`Engine::close_kind`).** No WORLD_SAVED reaches a closed engine, and the folder a viewer or the next start reads must hold the stop's save. **What it leaves:** a stop whose last journal write was held (U-g7: a flush save keeps the older file) leaves the pages ahead of it; the restart finds both generations ahead and logs `EngineAhead`, as before U-h. That is **slice U-i**: `nClose` carries `writtenThrough` and the engine confirms the final save only through it (a pending generation otherwise); a clean stop still confirms at once. `close_kind` is the one line it changes. *Built in slice U-i (U-i1 to U-i6): `close_kind` takes the final save's sequence and the file's, and F8 stands as the clean stop's test.* | Ben's log: the close choice (A: always pending, no interface change, but the viewer and every host test lag a stop; B: at once; C: through `writtenThrough`, a JNI change). B is built; C is its own slice. Test: F8 (`a_stop_confirms_its_final_save_at_once`). |
| U-h9 | **JB9 wired: `BoundarySession.trimsClampedTo(LongSupplier)`, set by `RtsimSession` to `JournalSavedData::writtenThrough`.** At each command header the acknowledgement is `min(ack, max(sent, pagesAtStart), writtenThrough)`, and that one value trims the journal and reaches the ack listener, so the holders' pending records and the rings follow it. The default is no clamp. In the running game the clamp never binds on a world save's acknowledgement (WORLD_SAVED reports `min(through, writtenThrough)` and the engine confirms through it, so the ack is at most that); it binds on an acknowledgement the engine takes without a world save (a script save, a stop). | A trim past the journal's file would drop observations a crash needs from the older file. Tests: `BoundaryProtocolTest.theTrimStopsAtTheJournalOnDisk` (J1), `theSessionStartsTheEngineWithTheWorldsNextSequence` (J2); the fake Java side mirrors it (`written_through`, `Java::save_world`). |
| U-h10 | **`saves_done` (`/rtsim perf`) counts confirmed saves.** A world save counts when it is confirmed (a script save at once, with the world saves it covers), so it trails `saves_committed` by the wait for WORLD_SAVED. New stats, not in Java's array: `saves_committed`, `committed_obs` (`Engine::committed_obs_seq()`, what a test waits on), `confirms`, `confirm_failures`, `budget_saves_held`. | Java's array and `/rtsim perf` keep their meaning: saves the acknowledgement covers. |
| U-h11 | **A confirm that fails (its log record torn) leaves the generation pending and the engine running.** `confirm_failures` counts it, the store reopens from its files (`recover_store`: the torn record is cut), and the next WORLD_SAVED or commit tries again. | The same recovery as a failed commit (LP10), without a restart. Test: F14 (`a_confirm_that_fails_keeps_the_save_pending_and_the_next_world_saved_confirms_it`). |
| U-h12 | **What this closes and what it leaves (residual 11 of docs/replay-gate.md 9.6).** Closes its P29 bullet (U-g10 (1)): a held or failed journal write leaves the engine's save pending and the restart from the older file rolls it back; `held_journal`'s answered case (the P29 case U-g left lost) is now applied once, by the answered case's assertions: the restart from the older file rolls the pages back to W0, and X is decided again from W0's pages under the same op with a new cmdSeq, no longer resent from their outbox; both held tests (the answered and the written case) re-derive X that way and the books equal the control's. Slice R's two J4 tests in `tests/industry.rs` take the same outcome: the answered one, which asserted the loss, is now `a_station_exchange_answered_before_the_pages_is_applied_once_when_its_trains_save_fails`, and both J4 tests assert that the start opened W0's pages, that X was sent once under a new sequence, and that train, totals and books are the control's (the written case no longer resends from the outbox). **Harness limit:** the fake world's production stays in its clock, so its W0 save holds every page at W0; a world whose W0 pages are the whole world is the only one the re-derivation is shown on. **Left, still residual 11:** (2) the unmarked files (Diesel's `cdg_oil_chunks`, NeoForge's `neoforge_data_attachments`); (3) ~~**a stop with a write outstanding** (U-h8: slice U-i's)~~ *closed by slice U-i (U-i1)*; (4) an abandoned entry (U-g5); (5) NeoForge's IO chain stops at an exception. The viewer and the fork lag one world save (they read the confirmed generation); the fork's pages and journal are always a pair the start accepts. **Costs (D1):** a crash in the window replays up to one more autosave interval than before (RP-49: the replay's rows are bounded by the unpaged footprint); the pages a held streak leaves pending, and the kernel's `prev` map of changed pages, grow toward one extra copy of every page changed since the last confirm; Java's holders' pending records, the rings and the journal live one marker longer, and in a long held streak BEHIND_WARN logs at 10,000 applied ops and the rings' valve (residual 6) becomes reachable on a busy ring. None of it is on the server thread. | The hold only moves the journal's file; this moves the pages with it. **UR-1:** no new SavedData, attachment, NBT key or mixin; a pending generation lives only in `world/rtsim`, and an older build reading the store rolls it back at its start. **No format changed:** Commit, Confirm and Rollback records already exist in the store (K11, K12); the journal's NBT and WORLD_SAVED are unchanged. |

**Results (the branch merged with main's slices R and U-g, 2026-10-27).** The Rust workspace passes (2,106 tests, clippy with
`-D warnings` clean). The merge brought tests that ask the engine for pages with `request_save` and wait for the acknowledgement
without the bridge's WORLD_SAVED (eight in `industry.rs`, among them `ack_never_resends`' restarts with industry off); each now
sends it, as `mc_tick_until_durable` does. Slice R's two J4 tests took the P29 outcome (U-h12). The core Java unit tests pass (300,
`BoundaryProtocolTest`'s trim clamp and start argument among them). GameTests: core 142 passed (G1 `aHeldJournalHoldsTheEnginesAcknowledgement`
held the engine's acknowledgement at the file's 551 while the journal was at 582, then followed it; G2
`theEnginesAcknowledgementNeverPassesTheWorldSavedMarker` held at 185), create 247 passed. The first core round had one failure that was
no engine fault: G1 shared the batch `mic_rtsim_saved_data_write_hold_d` with U-g12's give-up test, which lowers the give-up limit to 2
for every file on record, so G1's failing Pad was counted against that test's saves and its third save found the journal's file
written a save early. G1 has a batch of its own. No store or save format changed, so the viewer's fixtures
(crates/rtsim-view/tests/data, from main's rounds) are unchanged and the viewer and host tests pass on them.

## Slice U-i: the stop's final save is confirmed through the journal's file (U-h8, option C)

Branch `fix/slice-u-i` (on main after slice U-h and U-g), 2026-10-27. Ben chose option C of U-h8's choice (the close
choice: A always pending, B confirm at once, C through `writtenThrough`, a JNI change). U-h built B and left the stop
whose last journal write was held: `close(save = true)` confirmed its final save at once, so the pages were ahead of the
older journal file (U-g7: a flush save keeps the older file when its chunks or a SavedData write hold it), the restart
found both generations ahead of the journal and logged `EngineAhead`, and the engine kept a world the journal lacked.
This slice is C. The failing-first record is in the commit log (`09158f3e` has the tests and a `close_through` that
ignores its argument); the text of each failure is in U-i5.

| Id | Choice | Why, and what is left |
|---|---|---|
| U-i1 | **The stop's final save is confirmed at once only if the journal's file holds every observation it includes; otherwise it is a pending generation, and nothing confirms it.** `Engine::close_through(save, written_through, timeout)` replaces the one line of `close_kind` U-h8 named: `close_kind(final_obs, written_through)` is `SaveKind::Script` (committed confirmed, with every generation under it) when `max(written_through, world_saved) >= final_obs`, else `SaveKind::World` (`commit_pending`; its `confirm_locked` finds `world_saved < final_obs` and does nothing). `final_obs` is the sequence the save will carry (`replay.applied_through()`), read under the same lock as the save. The comparison is `>=`: a file that holds exactly the save's last observation holds the save. A pending final save logs one warning naming the two sequences. `world_saved` joins the file's word because a WORLD_SAVED the engine heard says the same of the file (it can only be at or below `writtenThrough`; a test that names a smaller `writtenThrough` than a WORLD_SAVED it sent still gets the larger). `Engine::close(save, timeout)` stays, as `close_through(save, u64::MAX, timeout)`: a caller with no journal file (the 300 test call sites) names a file that holds everything. | No WORLD_SAVED reaches a closed engine (the last tick has run, so no hand-over carries one), which is why U-h8 confirmed at once. The restart from the older file now finds the pending final save past the file and the confirmed generation (the last world save whose WORLD_SAVED came) at or below it, rolls the pending one back (`select_for_journal`, U-h3) and the replay brings the world to what the file holds, as it does for a held WORLD_SAVED (U-h). **Why a clean stop still confirms at once:** the engine applied nothing past the last tick's hand-over, and the stop's world save serialized the journal after it (`through = nextObsSeq - 1` at `saveLevelData`), so the file that lands holds at least what the engine applied; the flush save waits for the IO chain, so the value read at `ServerStoppedEvent` is final. **Rejected:** (a) always pending (U-h8's A: the viewer and every host test lag a stop); (b) the stop sends a WORLD_SAVED through the observation buffer (nothing hands it over once the ticks end, and a step would have to run for it); (c) comparing against `through` of the last world save instead of the file (the file is what a restart reads: `through` is what the save asked, not what landed). |
| U-i2 | **JNI, Java and the host follow, with one source for "what the file holds".** `nClose(long handle, boolean save, long writtenThrough, int timeoutMs)` (the signature change; negative values read as 0, and the value is unused when `save` is false). `Engine.close(long writtenThrough)` is a default method that closes as `close()` does (the stub keeps one generation); `NativeEngine.close()` is `close(Long.MAX_VALUE)` and `close(long)` calls the native. `BoundarySession.closeEngine()` calls `engine.close(writtenThrough.getAsLong())` with the supplier `RtsimSession` already set for the trim clamp (`JournalSavedData::writtenThrough`, U-h9), and `RtsimSession.close()` calls it; the clamp's doc says so. `rtsim-host`'s `Host::close`, which has just written the journal's file, names `next_obs - 1`. | The trim clamp and the stop must never disagree about what the file holds, so there is one supplier. It is read at the stop, not cached at the session's start. The crash path (`crashAndReopen`, a failed open) closes without a save and passes 0. The native library and the mod jar ship together, so the signature needs no compatibility (no release yet). |
| U-i3 | **Tests: Rust (rtsim-jni `tests/confirm.rs`), Java unit (`BoundaryProtocolTest`) and a core GameTest pair (`JournalGameTests`).** Rust, on the U-h harness: `a_stop_whose_journal_file_holds_its_final_save_confirms_it_at_once` (`writtenThrough` equal to the save's last observation, the edge, and past it: confirmed, and the restart from that file keeps the save), `a_stop_whose_journal_write_was_held_leaves_its_final_save_pending` (the older file's, and one observation short: pending, the confirmed generation is the first world save), `a_restart_from_the_older_file_rolls_back_the_pages_of_a_stop_whose_journal_write_was_held` (the open case, with its control: the same world stopped with the file landed keeps ten transfers on the books; the held one opens at the older generation and its books are the file's five), `a_stop_of_a_new_world_whose_first_journal_write_was_held_rolls_back_to_empty_pages` (`writtenThrough` 0: the world starts over from empty pages). F8 (`a_stop_confirms_its_final_save_at_once`) is unchanged and passes. Java: `theStopNamesTheJournalsFileToTheEngine` (the supplier is read at the stop, not before) and `aStopWithNoJournalFileNamedTakesItToHoldEverything`. GameTest: `aStopWhoseJournalFileLacksItsFinalSaveLeavesItPending` and its control `aStopWhoseJournalFileHoldsItsFinalSaveConfirmsItAtOnce`: the test's own two sessions on a native engine, as `journalReplayAppliesExactlyOnceAfterACrash` runs them, `RtsimSession.close()` for the stop (so Java's `closeEngine` and the real `nClose`), and the next session started from the older file (a `roundTrip` of the journal at the last landed save) opens that save's pages and keeps its ledger. | The GameTest does not hold the server's own journal with a failing Pad (G1's pattern) and restart it: `restartGlobalForTest` restarts on the same in-memory `JournalSavedData`, whose `nextObsSeq` is past the held tail, so the new session would keep and confirm the pending save instead of starting from the older file, and the test could not tell U-i from U-h. The sessions are the test's own and touch no server state, so it needs no batch of its own. Native engine only (the stub keeps one generation; the test records a skip). |
| U-i4 | **No store, page or save format changed.** A pending final save is a Commit record with no Confirm, the same bytes a world save leaves while it waits for WORLD_SAVED (K11, K12); the journal's NBT, WORLD_SAVED and every page are unchanged. The viewer's fixtures (crates/rtsim-view/tests/data) are not regenerated. **UR-1:** no new SavedData, attachment, NBT key or mixin; an older build reading the store rolls a pending generation back at its start. | The only wire change is `nClose`'s argument list, between two halves that ship together. |
| U-i5 | **Failing first** (`09158f3e`: the tests and a `close_through` that ignores `written_through`; `fn close_kind` still returned `Script`). Three of the four new Rust tests failed (10 passed, 3 failed, with the other confirm.rs tests): `a_stop_whose_journal_write_was_held_leaves_its_final_save_pending` ("older-file: the final save is pending, not confirmed": current and confirmed were both `Generation { save_seq: 4, obs_seq: 11 }`, the stop's save confirmed at once); `a_restart_from_the_older_file_rolls_back_the_pages_of_a_stop_whose_journal_write_was_held` ("the start selected the generation the older journal file holds, not the final save's (11)": left 11, right 6, the pages ahead of the file, `EngineAhead`); `a_stop_of_a_new_world_whose_first_journal_write_was_held_rolls_back_to_empty_pages` (current and confirmed both `Generation { save_seq: 2, obs_seq: 6 }`). | The control tests (`..._confirms_it_at_once`, F8, F4) passed on that commit, as they should: they assert what U-h8 built and U-i keeps. |
| U-i6 | **What it leaves, and what it costs (D1).** (1) **The viewer and the fork lag a held stop**: they read the confirmed generation, which is the last world save whose WORLD_SAVED came (as they lag one world save while running, U-h12). (2) **The restart after a held stop replays from the older file's save**: the same cost a crash in the hold has (U-h12's costs); the final save's own page writes (the IO of one save) are spent for nothing. (3) **A stop during the session's replay** still takes no save (the pages stay, RP-42), unchanged. (4) **A stop whose engine step did not finish in time** still saves nothing (the timeout path), unchanged. (5) A world older than both generations (a backup restored over a newer rtsim folder) is still `EngineAhead`, as before U-g. (6) Residual 11's other items stay: the unmarked files, an abandoned entry, NeoForge's IO chain (docs/replay-gate.md 9.6). | The hold only moves the journal's file; this moves the stop's pages with it. |

**Results (slice U-i on main at 8703d216, 2026-10-27 and 28).** The Rust workspace passes: 2,159 tests, 0 failed, 27 ignored (the
four new `confirm.rs` tests among them, with F8 and the other 8 of that file), and clippy with `-D warnings` is clean. The first
workspace run, made while the machine's load average was 45 to 65 with other sessions' rounds, had 9 failures, all in
`industry.rs`'s `ack_never_resends` and all the same line, the test rig's own 60 s wall-clock limit ("the iron line's end: 60 s of
wall clock", "the world's clock: 60 s of wall clock"); the module alone with 4 test threads passes (25 of 25), and the full run
repeated on a quieter machine passes. Those tests restart with `close(true, ...)` and `close(false, ...)`, which name a file
that holds everything, so U-i does not change their path. The core Java unit tests pass (313 test lines, 0 failed;
`BoundaryProtocolTest`'s `theStopNamesTheJournalsFileToTheEngine` and `aStopWithNoJournalFileNamedTakesItToHoldEverything` among
them). GameTests: core 144 passed (the pair `aStopWhoseJournalFileLacksItsFinalSaveLeavesItPending` and `aStopWhoseJournalFileHoldsItsFinalSaveConfirmsItAtOnce`
among them: held, the next session opened pages through 2, the first save's, with the stop's through 3, and the engine's ledger
was 5; landed, it opened pages through 3 and the ledger was 8), create 262 passed. **One round failed, and it was the test's
fault:** the first core round had one failure, the held GameTest ("the start opened the pages of the older save, which the file
holds: 0"): it took the older file before any world save had been made, so the pages it rolled back to were the empty ones and the
engine was right. The test now makes a real first save (the same `RtsimTests.acknowledge` the other tests use) before the
second transfer, and the round was run again. **No store or save format changed**, so the viewer's fixtures
(crates/rtsim-view/tests/data) are unchanged and the viewer and host tests pass on them.

## Slice U-e: the module switch is a journaled record (RP-290)

Branch `feat/slice-u-e` (on main 8703d216), 2026-10-27. Ben's decision RP-290 (below) as its own slice, after U-c and U-d. MODULES
(0x0004) was a latest-wins record: `wire::is_journaled` and `Obs.isJournaled` take 0x0100 to 0x7FFF only, the preamble wrote it ahead
of the journal's backlog (`BoundarySession.flushPreamble`), and SESSION_START set every module's `enabled` before the replay. So a
crash and a restart replayed a switched-off window as if the module had never been off, and a switch-off could reach the engine
before answers Java gave while the module was still on. Now the switch is a journaled kind, MODULE_SWITCH (0x0132), in the journal at
its real place; the engine applies it in the journaled dispatch, live and in a replay; the pages hold the mask as of their sequence.
Rows marked **OPEN** are choices RP-290 does not settle and that are not easily reversed (a protocol shape or a format): each says
what was recommended and built. The coordinator took each recommendation (nothing is released, so each can still change
before a release without compat work) and listed them for Ben on 2026-10-28; Ben may still change any of them. Everything else is
reversible and was decided without asking.

| Id | Choice | Why, test and cost |
|---|---|---|
| U-e1 | **The kind: MODULE_SWITCH 0x0132, journaled, aux 0.** Layout: the journaled prefix (OBS_SEQ at 8, GAME_TIME at 16), `u32 mask` at 24 (bit = `Module.ordinal()`, as MODULES' was), 28 reserved, size 32. **OPEN (layout only):** RP-290 fixes the kind, the prefix and the u32; the offset 24 and the reserved word are mine (the shape every journaled kind has: JOURNAL_GEN is the same size). It is admitted by `Replay` by sequence like any journaled kind, carries no op id, is never copied (no holder carries one, so no REASSERT holds one), and is not pinned by the trim (U-e5). | The prefix gives it a place among the records and a game time. Tests: the two halves of the layout meet in the tests, as for every other kind (no automated cross-check of the constant tables exists): Java's `BoundaryProtocolTest` reads the record `BoundarySession` wrote at `Obs.ModuleSwitch.MASK`, and the Rust protocol tests apply a record the double wrote at `obs::module_switch::MASK`. |
| U-e2 | **OPEN, protocol shape: MODULES (0x0004) is retired.** Java no longer writes it and the engine no longer reads it; the number is not reused. The mask's only carriers are MODULE_SWITCH and SESSION_START's `moduleMask` (U-e6). *Recommended and built.* The alternative is to keep MODULES as an informational latest-wins copy of the current mask. | Two carriers for one fact would let a stale MODULES undo a journaled switch (a latest-wins record is resent and dropped on a full buffer; a journaled one is not). The kind is internal to one jar (Java and the engine ship together, and nothing is released), so nothing reads it. Test: the Java test `anUnchangedMaskJournalsNothingAndNoModulesRecordIsSent` asserts a session sends no kind 0x0004; the Rust `wire` module no longer has the constant, and `Obs.MODULES` is gone from Java. |
| U-e3 | **Java journals the switch at its real place.** `BoundarySession.setModuleMask(mask, gameTime)` (the game time is new) commits a MODULE_SWITCH to the journal in sequence with every other record, and no longer touches the preamble (`modulesDirty` and the MODULES write in `flushPreamble` go). A mask equal to the session's current one journals nothing, as before. The constructor journals the session's own mask, with the session's game time, right after its JOURNAL_GEN (so at JOURNAL_GEN's sequence plus 1), whatever the mask and even when it is the previous session's: every session's switch is therefore in the journal beside its generation. `RtsimSession.onModulesChanged` passes `gameTime()`. | RP-290. The record is a function of the journal alone, so a replay sees the switch where the live engine saw it. A session that records its mask even when it did not change lets a restart see a changed config as that session's own switch (U-e7) and lets the journal be read without the pages. Tests: `BoundaryProtocolTest` (the session's switch follows its JOURNAL_GEN with the constructor's mask and game time; a switch between two transfers sits between them in the journal and in the engine's received order; an unchanged mask journals nothing; the preamble sends no MODULES; sequences that tests pinned move up by one). |
| U-e4 | **The engine applies it in the journaled dispatch** (`dispatch`, its own arm ahead of the catch-all `Verdict::Apply`), live and in a replay, as the sequence of modules that MODULES used to reach: the world's mask, industry, phase 1, then `shared::switch` for the dragons, ships, herds and conflict. A record that changes nothing is admitted and does nothing. A record the pages already include (`AlreadyApplied`) does nothing: its effect is in the pages' mask and pages. **Before the switch, the modules run to its tick:** `shared::catch_up` (the viewer modules' events and phase 1's kept inputs due before it fire first, as for any record of theirs) and, for the industry engine, `begin_session` if the record is this session's, the deferred records before it and `Engine::run_in_step` to its game time (a new public method of rtsim-industry that is `advance_to` without ending the op window's step: a replay is one step); a switch-on takes its game time from the record (the engine's own session start waits for `max(record time, engine clock)`), never the hand-over's. | A switch is a point in the module's life. If the frozen clock were wherever the last hand-over left it, a replay (which never runs the clock to a hand-over's time and batches records differently) would freeze the engine at another tick than the live run did, and every tick-derived op id after the switch-on would differ (J3). Run to the record's tick first, live and replay freeze at the same place, because running an engine to a tick is split-invariant. Tests: protocol `module_switch` below. Cost: one `advance_to` per real switch. |
| U-e5 | **OPEN, format: the pages hold the mask in the engine's meta page, RTJM version 2 to 3.** `encode_meta` appends `u32 mask` after the outbox: the mask `Engine::start` finds the world in as of `applied_through`, the sequence the pages include; `take_save` writes the engine's current mask, which is the mask of the records applied through that sequence because saves are taken at step boundaries. A MODULE_SWITCH is not pinned in Java's journal the way a JOURNAL_GEN is: the trim may drop one, and the pages' mask is the answer for everything trimmed. *Recommended and built.* The alternative is the mask in every module's meta page (six formats to bump, and the conflict's is Mowzie M10's to change on its own branch); the module pages keep their versions, so no module reader or viewer decoder changes. An older save is refused by the version check (`start` returns "meta page version 2, this build reads 3"), as every earlier version bump was; nothing is released, so no migration. | One value per snapshot, in the one page every save writes, whatever the modules do (a switched-off module's pages stay as they were, U10). The meta page is the engine's and the viewer does not decode it, so the viewer's own readers are not touched; its fixtures hold the old engine meta page, and the tests that start an engine on a fixture fail on the version (listed in the report). Test: a unit test round-trips the page with a mask (`meta_round_trips`) and refuses version 2 (`a_meta_page_without_the_mask_is_refused`), and the protocol test `the_pages_hold_the_mask_as_of_their_sequence` saves with phase 1 off and restarts with it on: `Engine::module_mask` (new, tests only) reads off before any step, and the session's own switch-on books the record kept in the pages. |
| U-e6 | **SESSION_START no longer sets `enabled`; the flags come from the pages' mask, and a start whose pages hold no mask takes SESSION_START's. OPEN, protocol semantics:** RP-290 settles the first half (the replay starts from the pages' mask). For a start with no mask (a new world, or pages deleted), SESSION_START's `moduleMask` field stays and is the starting mask: Java fills it with the mask its session starts in (the constructor's, not a later one), so a new world is exactly what it was. *Recommended and built.* The alternative is every flag off at a start with no pages, so that the session's own MODULE_SWITCH switches the modules on at the replay's end. `Engine::start` sets the flags from the page's mask before any step; each module's `session_start` keeps its per-session work (dictionaries, queues, the ships' lifting of closures when off) and drops only the assignment of `enabled`. | A new world has no pages to carry a mask and no history to replay, and the alternative would run the first switch-on path (resets, kept answers, the industry engine's session start moved to the record) on every new world, a behaviour change in the commonest start for no gain. The fallback is only read when no mask is known; a world with pages never reads the field, which is why it can stay a single u32. Test: every protocol test that opens a new world with `j.module_mask = X` (more than a hundred, unchanged) runs on the fallback; the pages' mask test above runs on the other path. |
| U-e7 | **A restart whose config differs from the journal's last mask is that session's own switch.** The constructor's MODULE_SWITCH has the session's first sequence plus 1, so `replay_end_before` has ended the replay when it arrives: the engine applies it live, in that first step, from the mask the replay left it in. The replay itself ran with the mask the pages and the journaled switches gave it, as the crashed session did. | The old start applied the new config to the whole replay: a module the config now disables ignored (kept) records that had been heard with it on, and a module it enables applied records that had been heard with it off. Now the replay is the crashed session's and the config is a switch like any other, at the session's start. Tests: `a_restart_whose_config_switches_a_module_off_replays_the_window_with_it_on` and the opposite case (below). |
| U-e8 | **The rtsim-host `Java` (the Rust bridge double) journals as the bridge does.** `Java::modules(mask)` commits a MODULE_SWITCH to its journal (nothing when the mask is the current one); `open` journals the session's own switch right after the JOURNAL_GEN, and the mask in it is made final (`Java::fix_start`) when the session first talks to the engine (its first hand-over, or its first `modules`): the tests set `j.module_mask = X` right after `open`, as the field was SESSION_START's, and the hundreds of them keep their form. SESSION_START carries that mask, and so does a `world_save` taken before it. | Tests set the mask after `open` (the field was SESSION_START's); the bridge's own constructor takes it as an argument. Writing the mask into the record `open` journaled puts it where the constructor would have, with the same mask and the session's game time, and costs one flag. `world_save`, `journaled` and the sequence arithmetic tests see it from their first use of the session. Two hooks put a switch where the old preamble used to put MODULES, ahead of an answer: `Java::switch_ahead_of_transfer_answers` (before the next TRANSFER_TO_CONTAINER is answered) and `Java::switch_ahead_of_result_of` (at the front of the industry tick whose output holds the result of an op); the industry rig has the same for any answer (`Rig::switch_pick`). |
| U-e9 | **The docs brought to it.** docs/replay-gate.md 9.7: its preamble note ("The preamble order stays") now says the switch is a record and its place is the journal's, and the sweep's rows say what U-e changed; section 2.1 and the SESSION_START arm say what SESSION_START still does. docs/boundary.md: the kind table (MODULE_SWITCH added, MODULES retired, SESSION_START's mask field described), the topology preamble's "after the dictionary and MODULES", and the dragons' switch note. U-d's off-path entry (RP-300) and RP-285's test shape: the A/B tests that restarted "with the module on" against "off, then on later" are restated for what the replay now does (below), and RP-292's *Left* line ("a restart still replays an off window as if the module had been on") is closed. | The slice is the decision RP-290 names; the docs would otherwise state the old protocol. |
| U-e10 | **The tests, failing first on main's engine** (the records below say what each printed). Rust, `crates/rtsim-jni/tests/protocol.rs` module `module_switch`: the live order (a record before the switch is heard on, after it off), the replay of the same window after a crash (config on and config off), a restart whose config differs, the pages' mask, the switch-on of a viewer module that keeps records; `crates/rtsim-jni/tests/industry.rs` module `ack_never_resends`: the A/B determinism test (a restart that replays an off window against a run that never restarted: books, totals and state hashes equal) and the freeze clock. Java: `BoundaryProtocolTest`. GameTest: `ModuleSwitchGameTests` (core), which already switches modules on a live server and asserts what the engine received; it now asserts the journaled record, in order. | Each behaviour has its own test, written against the double with the engine unchanged. **Failing first** (commits "tests first", run on main's engine in a scratch worktree): `a_record_before_a_switch_off_is_heard_on_and_one_after_it_is_kept` ("the robbery before the switch is booked, the one after it is not", left 2, right 1: main's engine heard the switch ahead of the records, so the second robbery was booked); `a_restart_replays_the_window_before_a_switch_off_with_the_module_on` ("the robbery before the switch is booked in the replay", left 0, right 1: the restart's start mask applied to the whole replay); `the_pages_hold_the_mask_as_of_their_sequence` (left 15, right 7: the restart re-enabled the module the pages had off); `a_restart_that_replays_an_off_window_makes_the_world_a_run_that_never_had_it_makes` ("the session's own switch and the window's: 1", main journals no switch); `a_window_of_exchanges_after_the_pages_makes_the_same_world_restarted_off_or_on` (the books of the two restarts differ: "world totals off by: create:crushed_raw_iron -96, minecraft:iron_ingot +2735" restarted on, "create:crushed_raw_iron +128, minecraft:iron_ingot +2639" restarted off then on; MZB-940's 224 is their difference); `a_restart_replays_the_switch_and_freezes_industry_at_its_tick` ("the engine ran to the switch's tick and stopped there", left Tick(72339), right Tick(72338): main's engine froze at the hand-over's time, not the record's). Java, `BoundaryProtocolTest` on a shim of the new signature over main's behaviour (8 of 27 fail): "the generation and the mask ==> expected: <2> but was: <1>"; "the same mask journals nothing ==> expected: <2> but was: <1>"; "the transfer between the two sessions' JOURNAL_GENs, each session's switch after its own ==> expected: <5> but was: <3>"; "aSwitchIsJournaledBetweenTheRecordsAroundIt: expected: <[304, 306, 272, 306, 272]> but was: <[304, 272, 272]>" (kinds in the engine's received order); and the three trim and pin tests that count the session's switch among the journal's records (expected 3, was 2). |
| U-e11 | **MZB-940 is closed by this slice (goods lost by a restart whose config differs from the session that crashed).** Session 1 runs with industry on and decides station exchanges after the last pages; the world saves after them; the engine crashes; session 2 starts with industry off. Before U-e SESSION_START set `enabled` before the replay, so the replay ran off: `journaled` ignored every record but Java's answers (the train arrivals among them), none of the window's exchanges was decided again, the kept IND_CARRIAGE_RESULTs met no pending exchange, the carriage side of each was in the world and its station side, held in the engine's memory, was booked nowhere: with an odd number of exchanges "world totals off by: create:crushed_raw_iron -224". Now the replay starts from the pages' mask (on), decides them again, and session 2's own MODULE_SWITCH turns industry off after it (U-e7). **Test:** `a_window_of_exchanges_after_the_pages_makes_the_same_world_restarted_off_or_on` (`tests/industry.rs`, `ack_never_resends`): the pages before the window, an odd number of at least three exchanges decided since, the world saved after them; B restarts on, A restarts off and switches on in its second hand-over; the world against the engine's books closes in both and the pages, clocks and op ids are equal. **Merged:** `feat/mowzie-m10` (36634dd2) is merged into this branch; the M10 test `a_world_saved_after_three_later_exchanges_closes_its_books_with_industry_off` (session 1 on, the world saved after three later exchanges, session 2 off; `twice` in `tests/industry.rs`) is un-ignored and passes 3 of 3 alone and in the serial `ack_never_resends` group, and MZB-940 in docs/choices-mowzie-build.md says U-e closes it. | The same property as the M10 test, written on U-e's own helpers so that the two branches do not collide (M10 lands its commits on main; duplicated ones would conflict). |
| U-e12 | **What this leaves.** (1) The viewer modules' switch-on reset ends at the hand-over's time live and at the replay clock in a replay (`shared::hand_over_target`), so a dragon or herd group resumed by a replayed switch-on is placed at the replay clock; the two differ by the ticks between the switch and its hand-over (none when the engine keeps up). (2) The phase 1 world is not run to the switch's tick, because running it is split-invariant and nothing reads its clock at a switch. (3) The kept lists, the viewers' catch-up and the industry clock of a switch *inside* a world whose pages are lost (a start with no mask and a journal that holds switches) begin from SESSION_START's mask: the pages' loss is the larger loss. (4) The fixtures: `crates/rtsim-view/tests/data/core` and `create` hold the engine's meta page at version 2, and the only test that starts an engine on them, `gametest_worlds_open_in_the_host_with_their_saved_state` (`crates/rtsim-host/tests/host.rs`), fails on each ("the engine's meta page: invalid: meta page version 2, this build reads 3"). They are not regenerated here; the viewer's own tests read module pages only and pass. | (1) is the only divergence between a live switch-on and its replay that the A/B test cannot hide, because its modules decide nothing at a switch-on that reads the placement. The fixtures are listed in the report. |
| U-e13 | **A replayed switch-on from an earlier session runs that switch's engine session start where it was journaled, and leaves this session's pending start alone** (Graphide review of U-e, PR 16; real). `set_enabled_at` replaced `session_start_at` for every switch-on, whichever session journaled it. Live, a switch-on moves the pending start to its tick and the engine's session start runs at the next record or step (`begin_session`). In a replay the next records of an earlier session never begin the session (`this_session` guards it: the SESSION_START start must wait for the replay end), so the T2 of a crashed session's off-then-on window replaced the start SESSION_START stored for the restart: one engine session start, at the old switch's tick, none at the session's present, and the one the live run did at T2 (districts unloaded, learners discarded, sent-this-session cleared) never happened in the replay. Now `set_enabled_at` takes `this_session`: a switch-on of this session replaces the pending start (as before); one of an earlier session runs `begin_session` at its own tick right after the requeue of the kept commands (so `hand_on_again` sees them, as the live run's next record did) and puts the pending start back. **Test:** `a_restart_replays_an_earlier_sessions_switch_on_with_its_session_start` (`tests/industry.rs`, `ack_never_resends`): session 0 switches industry off and on in a window after the pages and crashes, session 1 restarts on (the journal's last mask, so its own switch changes nothing); the engine's session starts are two (the replayed switch-on's, then the session's own at its present). New counters `IndustryStats::session_starts` and `session_start_tick`. **Failing first at cdd6aa4c:** "the replayed switch-on's session start at its tick, then this session's at its present", left `(1, 72546)`, right `(2, 72947)`: one start, at the old switch-on's tick, none at the restart's present. | The A/B shape (a restart against a run that never crashed) cannot compare these two, because a restart's own session start is the difference, so the test counts the starts and their ticks instead; the live run's count is asserted first (one start at the switch-on). |

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
  jumps the backlog. *Closed by slice U-e (above): the switch is a record at its place, and a replay
  applies it there.*

## RP-291: a tribute stack's answer heard with the dragons off (slice U-c, part 1)

- *Chosen.* The dragons take a tribute stack's COMMAND_RESULT whatever the switch, at once
  (`dragons::after_core`, `answered_again`).
  - APPLIED or DUPLICATE_OP: the stack leaves `placing` and its coin counts as placed
    (`tribute_placed`).
  - FAILED: the coin goes back to its lair's unplaced coin (`tribute_failed`). The chest is
    forgotten if it is the stack's own (MZB-762). Whether the coin goes again at once, as a new stack
    to a chest the lair reported since, depends on the switch (MZB-767): the dragons running, it does;
    off, no stack is made and no op id is drawn.
  - Off, the dragons' pager is marked `save_due`, so the pages hold the change at the next save.
  - MSC55 in docs/choices-money-sink.md is amended.
- *Why.* The core takes the stack out of the outbox at its answer whatever the switch, and Java forgets
  the op at the engine's acknowledgement (the journal's op index and the chest's chunk carry). Ignored
  while off (MSC55's first form), the stack stayed `placing`. `write_tribute` wrote it again at the
  switch-on under the same op, and Java, which no longer knew the op, put the coin in the chest a
  second time. MSC55's premise, that Java would answer DUPLICATE_OP, no longer held.
  - Applied at once, not kept for the switch-on (RP-292): neither `tribute_placed` nor
    `tribute_failed` reads the clock (a failed stack's next op comes from the lair's own counter,
    MSC47), and no decision reads a lair's `placed`. Since MZB-762 a FAILED that keeps a chest also
    places the failed coin again, which is a decision (it draws op ids and makes stacks), so it runs
    only while the dragons run (MZB-767): "no decision" holds for the answer taken off, and the
    switch on, which forgets the chests (`begin_replay`), would otherwise find stacks made for a
    chest it no longer holds.
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
    at a save while there are any and the list is dirty (see *Cost*), whether or not the save writes
    the module (a switched-off module's other pages stay as they were, U10). It is read at load
    whatever the switch. A module switched off before its first save may hold that page alone
    (`holds_pages_besides_kept`).
  - **Applied at the switch-on,** in the resets of every module that shares the phase 1 world, in
    one function (`shared::resets`; U-c's review, P2). MODULES reaches the four in two passes
    (`shared::switch`): first every module's flag, with a switch-off's own work; then the resets due,
    in one order (dragons, ships, herds, conflict) and in three steps:
    1. each module's engine begins its replay (`begin_replay`);
    2. the records kept by every module that began are applied as one list, merged by the observation
       number each came under (the order `Replay` admitted them, and the order a restart's replay
       sends them; a copy's is its REASSERT's), each module's own in the order it came. No two share
       a number, so the merge meets no tie and module order never overrides the journal's:
       `dispatch` gives each admitted record to one module, and a REASSERT holds one record, whose
       copy comes under the REASSERT's own number (a `debug_assert` checks the numbers rise);
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
    - Phase 1's drain (slice U-d) goes into `shared::resets`, after the viewers (done, RP-300: the joint
      switch-on).
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
    it, so the ids the conflict derives from a tick are the same both ways: a slot's generation,
    from the camp, the slot and the tick its member was recruited or adopted (`derive_gen`,
    crates/rtsim-conflict/src/engine/camp.rs). A late booking at the world's present derived them
    from another tick. The dragons derive no id from a tick.
- *A grove's tribute answer* (RP-291's twin). A COMMAND_RESULT for one of the grove's stacks in flight,
  heard while the conflict is off, is kept like the conflict's records (`conflict::after_core` and
  `answered_again`; the same Kept push, so `shared::resets` merges it in observation order) and applied
  at the switch-on at its own tick (`tribute_tick` takes `shared::floor`). It cannot be taken at once as
  the dragons' are: `tribute_placed` picks the next stack and books the goods out of the earmark
  (MZB-930), and `tribute_failed` may pick the next stack too (MZB-762), both at a tick, so they are
  decisions. Nor can it wait to be heard again: the core's outbox completes
  the op on any answer, and U10's comment that the stack stays in the outbox was wrong. Ignored, the
  stack stayed `placing`, was written again at the switch-on under the same op, and Java, which had
  forgotten it, put the coin in the chest twice. Tests: rtsim-jni `tests/protocol.rs`, module `conflict`,
  `a_groves_tribute_stack_answered_with_the_conflict_off_*` and the `_not_loaded_` one.
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
  - The review's probes (63131797), each failing first by mutation:
    - Probe A, the merge across modules:
      `a_kept_wipe_goes_in_before_a_later_kept_dragon_record_of_another_module` (`conflict`, with
      ECOLOGY on in the camp). The conflict and ECOLOGY go off together; a mob wipes the band out
      before d0 (kept by the conflict), a dragon is hurt a day past d0 (kept by the dragons), and both
      come on two days past d0. It wants nothing expired, the demand withdrawn and the casualties
      booked. With the merge in module order (the selector `(i as u64 + 0 * seq_of(b), i)`), the
      hurt's catch-up runs the conflict past d0 first: (1, 0, false) against (0, 1, true).
    - Probe C, a failed save's kept page: `a_dragon_killed_while_off_stays_dead_after_a_failed_save_and_a_restart`
      (`dragons`). The first save after the death while off fails (a torn log), the next writes the
      kept page again, and after a clean restart the dragon stays dead. With `drain`'s three
      `save_failed()` calls removed: (alive, materialized again) is (1, 1) against (0, 0).
- *Left.*
  - No order test for the dragons or the herds. Neither has an event log a test can read (the herds'
    `log_enabled` is false and has no accessor). Their order rests on the same `shared::catch_up`
    before each kept record that the conflict's tests exercise.
  - The ships (`Module.SHIPS`) still ignore their records while off (U10).
  - No GameTest: no Java changed.
  - A restart still replays an off window as if the module had been on (RP-290, U-e). *Closed by slice U-e: the replay runs in the pages' mask and applies each journaled switch at its place.*

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

## RP-300: phase 1 switched off hears its journaled records (slice U-d, findings U10 and PR13)

**The rule.** The switch gates only new decisions; every record of what already happened is heard; a
record applied while off goes in at its own tick, never at the frozen clock.

**Chosen.**

- Every journaled phase 1 record goes through one path, `phase1::book` (`engine/phase1.rs`): on, the
  input is applied now; off, it is kept in `Link::kept`, a list in time order (equal ticks in the
  order heard) at the record's own world tick (`wt(GAME_TIME)`). The kinds: TRADE (a village
  resident's and a caravan's), CARGO_CHANGED (robbery, goods in), TAKEN and DIED, HURT, RESCUE,
  TALKED, BASE_MARKED and BASE_UNMARKED, VILLAGE_ADOPTED (its Join and village type), HOUSE_PLACED
  and a work's COMMAND_RESULT. The census (VILLAGE_CENSUS, latest state, sent again) is still
  ignored and counted in `ignored`, which now means only that.
- `drain_kept` applies the list in order, each input after `shared::run_rounds` has run the world
  and the modules sharing its clock to its tick (so the dragons, ships, herds and conflict meet it
  in order, S13). It runs at the switch-on (`phase1::drain_when_due`, a step of `shared::resets`:
  the joint switch-on, below), at the top of `simulate`, at the top of
  `shared::run_to` (a viewer module running the clock while phase 1 is off must not run it past a
  kept input) and in the dragons' and the ships' `ingest` before they run their engines
  (`drain_before_run`: a DRAGON_STATE or SHIP_STATE applies at the hand-over's time, and an engine's
  `run_until` ends with the world's, so the inputs before that time go in first, review P3). Each
  input is removed before it is applied, because applying runs the clock again, which comes back
  to the drain. Off with no viewer running nothing moves the clock, so nothing drains (the guard
  of `drain_kept`); the two `ingest`s have no guard, because a module that is on and has no lair,
  dock or oil yet is no viewer and its engine runs the world all the same. While the session's
  replay runs with a viewer the drain stops at the replay clock (`clock_target`).
- The list is saved in the meta page (`META_VERSION` 4 to 5, after the queue), in the same atomic
  commit as the pages of the save that acknowledged its records, so a crash while off brings back
  exactly the inputs whose records Java dropped. Before release a version 4 save is read as
  unreadable pages and dropped (S5): no migration.
- TAKEN and DIED book at `wt(GAME_TIME)` on and off. They booked at `world.now()`, the world's
  tick when the record was heard, which a replay hears at another tick (hazard J3).
- Guards resolve against what will be applied: a village is known if it is a node or a kept Join
  or MarkBase names it, so a HOUSE_PLACED or a work style heard while off for a village adopted
  in the same window is not refused; a repeat of a record already kept (a copy after a crash) is
  not kept twice.
- `check_capture` (the check at a caravan's despawn that its members' items equal the sim's row)
  returns early while a kept input moves that caravan's goods (Rob, CargoIn, CaravanTrade, Member,
  MemberHeld): the row has not heard them yet, so a difference is not a bug and was logged as a
  false mismatch. The check runs again once they are applied.
- HOUSE_PLACED and the work COMMAND_RESULT went onto the deferral for the order's sake (item 4 of
  the brief, see Tests): a record applied at once while an earlier one of its village waits would
  be applied before it, and the input log would not be in time order.
- WORK_CATALOG is left alone: it is latest state (the whole catalogue), sent once a session and
  not journaled, so a replay never brings it back at another tick. It is still taken while off, at
  the frozen clock.
- **The cap.** The list is bounded by count: `KEPT_CAP` is 24,576 inputs (`Link::kept_cap`, which
  a test lowers). Over it, `Link::keep` drops one report at a time until the list fits or none is
  left. Droppable are the inputs whose loss costs a sentiment or reputation nudge and nothing else,
  in three classes dropped in this order (`drop_rank`, review P3): **talks** first (`Talked`), then
  **hurts and rescues** (`CaravanReport`, and a `VillageReport` that is not a kill), then
  **residents killed** (`VillageReport` of kind 8), the dearest of the three (a killing moves a
  village's sentiment by -125 for 30 days, an assault by -25 and a rescue by +25 for 10:
  `rtsim-knowledge` tunables). Within a class the
  oldest goes first, whatever the age of the others: a kill older than a talk outlives it. A new
  report over a list with none of an earlier class is the oldest of its class and may go itself (a
  new talk over hurts and kills). Never dropped: trade, caravan trade, robbery and goods in, a
  taken or killed member (goods that left the sim's row), an adoption (a village for good, with its
  type), a base mark or unmark, a house placed, a work's answer (a resident or a work for good).
  `UnmarkBase` is in the kept class although it only removes a base: a base unmarked and never
  heard stays in the world for good. If loss records and structure alone pass the cap, all are kept
  over it. Each drop adds to `kept_dropped`, each insert that leaves the list over the cap adds to
  `kept_over` (both this session's, not saved), and a WARN line is logged at the first and then at
  each power of two of the counter (naming the class dropped), so a day of overflow is a dozen
  lines.
- **The switch-on says what it did.** `switch_on_drain` (`engine/phase1.rs`) wraps the switch-on's
  drain and logs one INFO line when anything was kept: how many of the kept inputs it applied, how
  many of them were base marks, and how long it took. Bases are never dropped, so a long window
  grows the drain, and the step that runs it, linearly with its marks: each is a node built into
  the world, about 25 ms apiece in the bench below. Inputs a replay's clock has not reached stay
  kept and the line says how many wait.

- **The joint switch-on** (U-d's review, P2). Phase 1's bit and ECOLOGY's (the dragons' and the
  herds'), or the conflict's, can come on in one MODULES record, and each of those modules kept
  records of its own from the same window. The drain ran in `set_enabled`, when phase 1 heard the
  record and before `shared::switch` set any flag, so the whole window of phase 1's inputs went in
  with the viewers off, the world ran to the last of them, and each record a viewer kept (a
  dragon killed in the window, a band wiped out) met a world already past its tick, with inputs of
  its future in it. Now `set_enabled` only marks the drain due (`Link::switch_on_due`, not saved),
  and `shared::resets` takes it as its third step, after `apply_kept` and before the replay ends
  (`phase1::drain_when_due`, which also logs the line above). Each viewer's kept record's catch-up
  (`shared::run_to`) already applies phase 1's inputs up to its own tick, so the record meets the
  world at its tick, with phase 1's earlier inputs in and its later ones not yet; the drain then
  takes what is left, and the replays end. A switch-on of phase 1 alone begins no reset, and the
  step runs all the same (`finish` is entered with nothing begun). A module's own reset in the
  middle of a session (`reset_these`, `end_resets`: a first herd, `dragons::reset`) does not take
  it: it runs inside the application of a record, often a kept one, and would take the drain
  before the records that follow. The first version of the move took the flag in `end_resets`
  itself and that is what the probe showed: the dragon's record leaves `reset_if_due` behind it,
  whose `end_resets` with nothing begun drained the window from inside `apply_kept`.
  The line counts the window as phase 1 found it when the record came: `Link::switch_on_due` is
  an `Option<SwitchOn>`, a snapshot (the inputs kept, the base marks among them, the instant) taken
  in `set_enabled`, none when nothing was kept. Counted at the drain it covered only what the
  viewers' catch-ups left (an "applied 1 of the 1" over a window of two), and nothing at all when
  a viewer's record was later than phase 1's last input. The duration runs from the record, so it
  includes the viewers' catch-ups too.

**Why.** The engine acknowledges every journaled record it admits, whatever a module's switch
(`Replay`), and Java drops what it saw acknowledged and does not send it again. Phase 1 used to
ignore its records while off (U10, PR13), so a caravan robbed or traded with between the switch-off
and its despawn kept in the sim's row the goods the player now has, and a dupe followed at the
switch-on. Booking at the frozen clock instead would be wrong too: tick-derived op ids and events
differ between the live run and a replay (J3).

**The cap's estimate** (synthetic, no real server's data). A busy day of switch-off, 40 players
online on average for 24 hours, 960 player-hours, with these rates per player-hour: resident trades
10, caravan trades 0.5, robbery stacks taken 1, members taken or killed 0.25, caravan or resident
hurts 3, rescues 0.25, talks 5; per day 150 villages adopted (an adoption is a Join and, for a typed
village, its style: 290 inputs), 160 houses placed, 160 work answers and 40 bases marked. That is
9,600 + 480 + 960 + 240 + 2,880 + 240 + 4,800 + 290 + 160 + 160 + 40 = 19,850 inputs, of which 11,930
are loss records or structure (never dropped) and 7,920 are reports. The cap, 24,576, holds that day
with 24 percent to spare; the reports are what the cap trims. Loss records and structure alone pass
the cap after about 2.06 days of switch-off at this load (24,576 / 11,930), or after about 25
hours at twice the players (80 on average, 23,860 of them in a day): **that is the residual**, the list then grows past the cap, one
WARN line at the first and at each power of two, with every loss record kept and applied at the
switch-on. At about 32 bytes an entry, ten days of this load would be about 119,000 entries (loss
records and structure only) and about 3.8 MB in the meta page, rewritten at every save.

**The cap against the page.** The page store has no limit on a page below 4 GiB (the length is a
`u32`), so the budget is mine: the meta page is rewritten at each save, goes through the store's log
(which folds at 4 MiB), and I held it under 1 MiB. Measured in the unit bench
`a_list_filled_to_the_cap_saves_loads_and_drains` with the mix above at the cap: 24,576 entries,
773,339 bytes (31.5 per entry), save and load 6 to 12 ms, and a whole drain at the switch-on 7 ms
for the inputs themselves (the 49 bases in the bench's mix cost about 25 ms each on top, 1.2 s, which
is the world adding a node as it would live). The worst case, every entry a robbery of ten goods, is
1.68 MB, which no mix reaches: a robbery is a stack, mostly one to four goods. So one meta page holds
the cap and the list is not paged. If a server's switch-off regularly passes the residual above,
paging the list (a page per few thousand entries) is the next step, not a higher cap.

The drop also means arm A and arm B of D1 are equal only under the cap. Over it, A (a restart with
phase 1 off) has dropped reports that B (a restart with it on, applying each record as it comes)
applied. A crash while off keeps the list as saved, so a drop already made is not made again.

**Cost.** One list in the meta page (empty on, a few bytes), a second path through the records
(`book` and `apply_booked`, which the on path takes too), a drain at four call sites, and one more
thing the capture check must know. While off and no viewer runs, the kept list grows with the
window's records, one input each; it is applied at the switch-on in one go, before the world runs
to now.

**Tests** (`crates/rtsim-jni/tests/protocol.rs`, `mod p1_off` and `mod p1_off_fork`; unit tests in
`engine/phase1.rs`). Failing first on the base (4e694452), what each printed:

- T1 robbery heard while off, then on again: the sim's row never booked it, `ROBBERIES 0 (want 1)`.
- T1b a despawn capture with a kept robbery: `capture_mismatches 1 (want 0)`.
- T2 trade: `CARAVAN_TRADES 0 (want 1)`. T3 member taken: `REPORTS 0 (want 1)`.
- D1 determinism, fork A/B: run to pages at S, switch phase 1 off, let the window's records in
  (a robbery, a trade, a taken member; nothing else), crash right after with the journal restored
  from its file, copy the page dir and clone the `WorldSave`, restart twice. Arm A restarts with
  phase 1 off and switches on later, arm B restarts with it on (the records replay at their own
  ticks); both run to the same world tick and compare `phase1_state().0` and the op ids decided
  after. An A/A control first (two B restarts are equal) and a sensitivity control (a record
  missing from one arm shows). On the base: A hash 16904006048110328712 with no ops and nothing
  booked, B hash 2294602932434821836 with nine gossip ops and the three bookings. On the fix A == B.
- *Amended by slice U-e (RP-290).* D1's window is journaled with its MODULE_SWITCH(off) ahead of the
  records (`switch_then`), so arm B (a restart on) no longer hears them as if the module had been on: its
  replay applies the switch off, keeps the records, and its own session's switch-on applies them. A and B
  are still equal. U-e's `a_restart_that_replays_an_off_window_makes_the_world_a_run_that_never_had_it_makes`
  (`module_switch`) states what is meant: the same journal with the switches patched to the mask the
  module was already in (never off) makes the same world.
- D3 a crash while off after a save that carries the kept inputs: they come back from the pages and
  are applied once at the switch-on. On the base the list does not exist (0 kept, want 2).
- Unit tests: the list across a save and load (`the_kept_inputs_come_back_from_a_save_in_order`),
  the drain order (`kept_inputs_drain_in_time_order_each_at_its_own_tick`: heard 3000, 1000, 2000,
  2000 applied 1000, 2000, 2000, 3000 and the world at 3000) and a `run_to` with a viewer running
  and phase 1 off (`a_run_to_with_a_viewer_running_drains_the_inputs_it_passes`: the input before
  the target in, the one after waiting).
- Item 3 (TAKEN/DIED tick): with only that booking set back to `world.now()` on the fixed tree D1
  diverges (A hash 7001903515526480207, B 2294602932434821836, the gossip ops after differ). At
  `wt(GAME_TIME)` it does not. It is the one on-path behaviour change.
- Item 4 (HOUSE_PLACED and the work COMMAND_RESULT) under the determinism test first: D2 (HOUSE_PLACED
  of a growth house, three Ribbit villages) and D2b (a work's answer arriving a day after the
  switch-off) both pass on the base, that is with both taken at the frozen clock: A == B, same
  hash, same ops. D2b is sensitive (without the answer the hash and the ops differ), but the answer's
  tick does not change what the world decides after it: `work_result` only wakes the village's next
  purchase at `max(at, now)`, and `house_placed` has no tick-derived state. So no divergence was
  found and neither is a J3 hazard in these setups; they stay on the deferral for the order's sake
  (above) and because one rule with no exceptions is easier to keep true. Reversible: take them at
  once as before.
- Item 5 (VILLAGE_ADOPTED, BASE, HURT, RESCUE, TALKED) all go through `book` with their guards
  resolved. HURT and RESCUE of a caravan: `REPORTS 0 (want 2)` on the base. TALKED:
  `talk_stats().talked 0 (want 1)` on the base, and the same op id again changes nothing. A base
  marked while off is kept and is a node afterwards (unit test); the adoption is the unit test
  `a_switched_off_phase1_keeps_its_records_and_catches_up`. HURT, RESCUE and a base of a village (as
  against a caravan) go through the same `known` guard, which the adoption and base tests exercise;
  no test sends a RESIDENT_HURT, a resident RESCUE or a base UNMARK while off. A quiet-window control
  (no records: nothing changes, the capture matches) keeps the comparison honest.

**Tests of the cap** (`engine/phase1.rs`; the first four failed on the uncapped list, which kept
every entry):

- `an_overflowing_kept_list_drops_the_oldest_report_and_never_a_loss_record`: with a cap of 5 and a
  robbery, a hurt, a trade, a talk and a join kept, a sixth input drops the talk at 40 (the hurt at
  20 is older, but talks go first; the first version of the cap, oldest report of any class, printed
  ticks [10, 30, 40, 50, 60] where [10, 20, 30, 50, 60] is wanted, and on the uncapped list [10,
  20, 30, 40, 50, 60]), a seventh the hurt; over a list of loss records one more is kept over the
  cap and a new report goes itself.
- `the_cap_counts_and_warns_on_the_first_drop_and_at_powers_of_two`: nine drops count 9 (printed
  on the uncapped list: 0) and warn at the 1st, 2nd, 4th and 8th; five loss records over the cap
  count 5 and warn at the 1st, 2nd and 4th.
- `loss_records_over_the_cap_are_all_kept_and_applied_at_the_switch_on`: a cap of 3, three villages
  and 40 trades heard with 40 hurts among them: 43 kept (83 on the uncapped list), 40 hurts dropped,
  and at the switch-on every village is adopted and all 40 trades are heard by the world.
- `a_capped_list_survives_a_save_and_load`: a list over its cap of 6 with 7 loss records (14 on the
  uncapped list) reads back equal, and after the load a further report is the one that goes.
- `a_list_filled_to_the_cap_saves_loads_and_drains`: the size bench above.

**Tests of the review's three P3s** (`engine/phase1.rs`). Each failed first on `89bcdc61`, what it
printed:

- Drop order, `the_cap_drops_talks_first_then_hurts_and_rescues_then_kills` (a cap of 6, the oldest
  entry a kill, then a hurt, a talk, a trade, a resident hurt, a talk): the first drop took the
  kill at 10 (kept ticks [20, 30, 40, 50, 60, 70]; wanted the talk at 30, [10, 20, 40, 50, 60,
  70]). Then each class in its order, a new talk over hurts and kills going itself, the kill last,
  and the per-class counts coming back right from a save (`soft_len`).
- Drop order at the real cap, `a_mix_past_the_real_cap_keeps_what_the_drop_order_keeps`: the list
  filled to exactly 24,576 (18,000 trades, 3,000 talks, 2,000 hurts or rescues, 1,576 kills, the
  oldest entry a kill) drops nothing; 3,500 more trades heard drop all 3,000 talks and the 500
  oldest hurts (talks, hurts, kills left on `89bcdc61`: 1,251, 833, 992; wanted 0, 1,500, 1,576);
  2,000 more drop the other 1,500 hurts and the 500 oldest kills (the head of the list among them);
  1,000 more leave 76 kills, the newest; then the 76 go for loss records, and the 77th over the
  cap is kept over it. Throughout the list is in time order and `kept_over` is 0 until no report
  is left.
- DRAGON_STATE, `a_dragon_state_applies_the_kept_inputs_before_it_runs_the_world_to_the_hand_over`
  (phase 1 off, a lair, a Join kept at 2,000, the hand-over at 9,000): the record ran the world to
  9,000 with the Join still kept, and it was booked at 9,000 once on (`Some(9000)`, wanted
  `Some(2000)`). The same hole for dragons that are on with nothing in the world yet
  (`a_dragon_state_of_dragons_with_nothing_yet_applies_the_kept_inputs_first`, which is also the
  first LAIR_FOUND of a session, the same `ingest`) and for ships on with no dock
  (`a_ship_state_of_ships_with_nothing_yet_applies_the_kept_inputs_first`, which the ships' own
  drain missed through the viewer guard); all three pass with `drain_before_run`. The herds'
  `ingest` runs only the legs (`w.legs`), not the world, so it needs no drain. The conflict's
  `ingest` runs the world only to its own events, and the wheel of a conflict with no faction is
  empty; the ChiefState arm ingests at the engine's own `now()`. Both read, not probed.
- The switch-on's line, `the_switch_on_says_how_many_inputs_it_applied_and_how_long_it_took`: 45
  inputs kept (3 adoptions, 2 base marks, 40 trades) give "applied 45 of the 45 inputs ... (base
  marks among them: 2) in ...", nothing kept gives nothing, and a replay clock at 5,000 with
  inputs at 2,000 and 9,000 gives "applied 1 of the 2 inputs" and "1 wait for the replay clock".
  The call from `drain_when_due` is seen in the output of `modules()` in the other tests (the line
  prints once per switch-on that had something kept), not asserted.

**Tests of the joint switch-on** (`engine/phase1.rs`, and one in `tests/protocol.rs`). The lib's
`State::kept_probe` (test builds only) records, for each record a viewer kept and `apply_kept`
applied: its tick, the phase 1 world's present, and the phase 1 inputs still kept, once it is in.
Failing first on `879c5367` (the drain in `set_enabled`), what each printed:

- `a_joint_switch_on_applies_the_viewers_record_between_phase_1s_own_inputs`: phase 1 and ECOLOGY off
  in one MODULES (`0b0_0111`), a lair for the dragons, a village heard at 1,000, a DRAGON_DIED
  kept at 2,000, a village heard at 3,000, then on in one record (`0b1_1111`). Printed
  `[(2000, 3000, 0)]` (the death met a world at 3,000 with nothing left waiting); wanted
  `[(2000, 2000, 1)]`. After the move both villages are in, at their own ticks (1,000 and 3,000).
- `a_joint_switch_on_applies_the_conflicts_record_between_phase_1s_own_inputs`: the same with the
  conflict's bit (`0b10_1111`) and a DIFFICULTY record kept at 2,000. Printed `[(2000, 3000, 0)]`,
  wanted `[(2000, 1000, 1)]`: with no camp the conflict does not run the world to its record, so
  the present is the village before it. A conflict whose record has a camp to run is the dragons'
  case with another module's clock, which the first test covers.
- Both pass on the move. The first run of the move failed the first test with
  `[(2000, 3000, 0)]` again, for the reason in the bullet above (the nested `end_resets`);
  `a_modules_own_reset_inside_a_kept_record_does_not_take_phase_1s_drain` keeps that: a
  `reset_these` and an `end_resets` with nothing begun leave the drain due.
- The line of the switch-on (review P3 of the joint switch-on), failing first on `ce97ce26`:
  `the_joint_switch_ons_line_covers_the_whole_window_when_a_viewer_record_is_the_last` (a village at
  1,000, a base mark at 1,500, a village at 2,000, a dragon's death at 3,000): the death's catch-up
  drew in all three and the drain found none, so the line fired 0 times (wanted once, "applied 3
  of the 3 inputs", "base marks among them: 1"). The dragons' test above also asserts the line
  (a record between two inputs): it said "applied 1 of the 1", wanted "applied 2 of the 2". The
  lib's `State::switch_on_lines` (test builds only) holds what `drain_when_due` logged.
- `a_switch_on_of_phase_1_alone_drains_in_the_resets_of_its_record` (nothing begun): phase 1
  hearing the record leaves both kept inputs kept and the flag due; `shared::switch` drains them
  in time order and takes the flag, once.
- The unit tests of the switch-on that called `modules(&mut st, 0b1111)` (the helper only made
  phase 1 hear the record) now call `shared::resets` after it, as the engine does; five of them
  failed first on the move for that alone (`a_base_marked_while_off_is_a_node_once_it_is_on_again`,
  `kept_inputs_drain_in_time_order_each_at_its_own_tick`,
  `a_switched_off_phase1_keeps_its_records_and_catches_up`,
  `loss_records_over_the_cap_are_all_kept_and_applied_at_the_switch_on`,
  `a_list_filled_to_the_cap_saves_loads_and_drains`).
- `a_joint_switch_on_of_phase_1_and_the_dragons_keeps_the_death_at_its_own_tick` (protocol): the real
  engine path, a dragon adopted and embodied, both off, a village, the player's kill, a village,
  three days, both on: the dragon stays dead, its lair is empty from the kill's tick and before the
  later village, both villages are in and nothing is kept. It passes on `879c5367` too (the dragons
  book the death at its own tick either way); the order is only seen by the probe.

**Left.**

- The joint switch-on's order is shown for the dragons and the conflict as viewers; the herds,
  which share ECOLOGY's bit, go through the same merged list (`apply_kept`) and the same catch-up,
  and have no test of their own.
- The census stays ignored while off (latest state; the next one after the switch-on lands).
- WORK_CATALOG is taken at the frozen clock while off (see Chosen), which is harmless because the
  engine's reply to a catalogue is not tick-derived.
- `docs/deploy-rehearsal-omegasrv.md` still describes phase 1 ignoring its records while off, as it
  was when the rehearsal ran.
- The residual of the cap: loss records and structure alone past 24,576 inputs (about two days of
  the synthetic busy load) are kept over it, with a WARN. A report dropped for the cap is a
  sentiment or reputation nudge lost for good; the engine acknowledged it and Java will not send it
  again.
- The switch-on's drain grows with the window's base marks (see Chosen), which the cap never
  drops: a long switch-off with many bases is a long step at the switch-on, logged. Paging the
  list, and applying it across steps, is the answer if a log line ever shows it.

## Slice U-f

Branch `fix/slice-u-f` (from `fix/slice-u-b`), 2026-10-25 to 2026-10-27. RP-215's fix, the coordinator's second decision: **each
half of a vanilla double chest whose halves are in different chunks is its own inventory of 27 slots.** A command
at a half changes that half, in the chunk it names, and nothing else, so the chunk's own epoch and carried result
already say whether it happened; the other chunk is not read, noted or carried. A pair inside one chunk stays one
inventory of 54 slots at its lower half: one chunk's write covers it. The first decision (carry and note the
reconcile in the other half's chunk, U-f1 to U-f15 of c423fa24 to c985bac5) was dropped after three review rounds
found it inexact for a change split across the halves, for a crash that saves one chunk and not the other, and for a
command held across the other chunk's acknowledgement; the per-half design has none of those states. The tests are
slice U-b's `SavedAheadGameTests` (batch `mic_rtsim_saved_ahead`), `ChestPairLoadGameTests` and create's
`IndustryGameTests`, and need the native engine as every core suite does.

| Id | Choice | Why, and what is left |
|---|---|---|
| U-f1 | **A vanilla chest or trapped chest (`Blocks.CHEST`, `Blocks.TRAPPED_CHEST`, exactly) whose type is not SINGLE and whose other half is in another chunk is read and written through `InvWrapper(its own ChestBlockEntity)`: 27 slots.** `StockAccess.splitPair(state, pos)` is the test (a reference comparison for any other block, and no block entity read; D1), `splitHalf` the handler (null for a position whose chunk is not loaded: it reads through `getChunkNow` and loads nothing), `StockAccess.items` returns it first, so every reader of an inventory (the sweeps, the reconcile, the transfer, the glass probes) gets it. A mod's `ChestBlock` subclass is not matched, and stays on its capability (Lootr has no item handler on purpose, GroveChests). | NeoForge's chest handler is `InvWrapper(ChestBlock.getContainer(...))`, which reads the other half through `Level.getBlockState` (loading its chunk, or waiting for it) and writes into either half. That is the whole of RP-215: a command naming one chunk changed two. The state's type is the whole filter: a LEFT half with nothing east of it (a world edit) gets the same own-slots handler the capability would have given it. |
| U-f2 | **A pair inside one chunk is unchanged:** one inventory of 54 slots, named by its lower half (`Districts.canonicalInventory`), the capability's `CompoundContainer`. | Both halves are written in the one chunk the command names, so its epoch and carried result cover the pair. Create's `IndustryGameTests.aDoubleChestIsOneInventory` stays as it was (its pair is inside one chunk). |
| U-f3 | **A split half's reads need nothing of the other chunk.** `StockAccess.partsLoaded` does not ask the other half's chunk for a vanilla chest, `Districts.hasCapability` and `GlassCompiler.hasCapability` answer true for a split half before any capability query (the chest's provider would read the other chunk), and `GlassCompiler.swept` probes a STORE node through `splitHalf` first. | A split half whose other chunk is still loading now reads whole (it reads none of it): `ChestPairLoadGameTests.aDoubleChestIsClassifiedAndDescribedWithoutWaitingForAChunkStillLoading` asks the classifier, the description and the handler at once and none makes the other chunk FULL. |
| U-f4 | **The two chunks are one district all the same.** `Districts.canonicalInventory` returns the half's own position for a split half, and `inventoryJoin` joins the half's chunk with its partner's (`join`, the existing helper), so a half in a chunk with no other industry is neither left out nor in another district. The routes that name an inventory all go through `canonicalInventory` (tagged stock and plugin inventories in `scanChunk`, `PlayerPorts.stockAt`, `Counting.moved`, `GraphBuilder.inventoryAt`, `TrainWatch`), so each gets the half's own position. | Without the join, each chunk would form a district of its own holding half of what the chest holds. Test: `ChestPairLoadGameTests.aSplitDoubleChestIsTwoInventoriesInOneDistrict` (core) and create's `aDoubleChestAcrossAChunkBorderIsTwoInventories`: two inventories of 27 slots, two chunks, one district. |
| U-f5 | **A reconcile or a transfer at a half goes through `StockAccess.items`, so it writes that half only.** `CommandDispatcher.transferToContainer` resolves the handler as every other reader does (it called `getCapability` before). No note and no carry in the other chunk: the reconciled chunk's epoch and result, and the transfer's carried COMMAND_RESULT, are in the chunk of the half that changed, as for any other container. | The result's `applied` says what the half took: a +100 into a half with room for 24 is applied 24, and the engine owes the other 76 (it books an unfit remainder as owed, as for any full chest). A -20 whose goods sit in the other half is applied 0, a shortfall the engine books as it books any. A transfer into a full half answers FAILED, though the other half has room. That costs fidelity, never goods: the engine's pair of stores was one, now it is two. |
| U-f6 | **A comparator by the other half is told.** After a write through a split half, `StockAccess.splitWritten` calls `level.updateNeighbourForOutputSignal` at the other half when its chunk is loaded. It marks nothing unsaved (the other half did not change). | A vanilla pair's `setChanged` updates the comparators beside both halves; the half's own `setChanged` reaches only its own. Test: `aComparatorByTheFarHalfFollowsAWriteThroughTheNearHalf`. |
| U-f7 | **The engine's acknowledgement is durable, so nothing is held for it.** `ackedObsSeq` is `durable_obs_seq()` (`crates/rtsim-jni/src/engine.rs`, set after `store.commit`, `Durability::Fsync` by default in `crates/rtsim-kernel/src/page.rs`; docs/boundary.md: "`ackedObsSeq` names only pages on disk"), so a chunk dropping its carry at the acknowledgement cannot leave the engine with the op pending. | The reviewer's P1-a (a command held across another chunk's acknowledgement) needed a hook under the carry design; with one chunk carrying one op there is no other chunk to wait for. `aReconcileHeldForAnUnloadedHalfAppliesOnceAfterTheOtherChunkIsAcked` runs the sequence. |
| U-f8 | **Create vaults and other multi-block storages are not fixed (residual 12 of docs/replay-gate.md), and a mod's double chest or any other `CompoundContainer` stays as it was (residual 9).** | The same mechanism, with no way to recognise the parts from the block state alone; found by reading Create 6.0.10's `ItemVaultBlockEntity`. Kept out as asked. |
| U-f9 | **A sim break of a pair inside one chunk drops only the broken half's goods, from either half; nothing is duplicated and no goods are lost.** No code change: the question was whether the engine's one doomed inventory of 54 slots, booked dropped whole, leaves goods duplicated or lost when the world keeps one half (traced and measured; docs/replay-gate.md residual 14). | `SavedAheadGameTests.aSimBreakOfAPairsFirstHalfDropsOnlyThatHalf` (P holds 100 diamonds, Q 30 emeralds, P breaks): 100 diamonds and one chest on the ground, no emeralds, Q a single chest holding its 30, and Q describes as an inventory of 27 slots at its own position. `aSimBreakOfAPairsSecondHalfDropsOnlyThatHalf` (Q breaks): 30 emeralds and one chest on the ground, no diamonds, P a single chest holding its 100. The engine books `inv.effective()` of the pair as dropped when P breaks, 130 where 100 dropped, and the district's stock lacks Q's 30 until a detection declares Q: a ledger over-count that nothing reads (`books.dropped` is read only by the fake engine's sum), not goods. Both tests pass on the carry code and on the per-half code, so neither has a failing-first step: they characterise a case the per-half change leaves as it was. |
| U-f10 | **A counted move at a half of a split pair is credited to neither half and counted apart** (review finding on `Counting.moved`). `Counting.moved` resolves the target through `Districts.canonicalInventory`, which names the half itself; when that store is a half of a split vanilla pair (`StockAccess.splitPair`), the move is discarded like one it cannot attribute and `Counting.splitMovesDiscarded` rises (beside `movesCounted`, for the tests and the debug output). Residual 13 of docs/replay-gate.md: split pairs fed by Create movers get no counted flows, so the glass compile learns no rates for them. | Create's funnels, arms, chutes and packagers move goods through NeoForge's chest handler, which is the whole pair's (the `CompoundContainer`), so a mover facing half Q can fill half P in the other chunk; crediting Q would make the counted flows disagree with the stocks. Changing how Create fills a chest would change gameplay, so the count is withheld instead. Test: create's `IndustryGameTests.aMoveIntoAHalfOfASplitChestIsDiscardedNotCounted` (a chute over the east half of a split pair puts 4 ingots into the pair; a chute over a single chest is the control). A player's session at a half (review rounds 2 and 3) is not discarded, since a discard at open left a menu held across a learning window unbooked and booked its throws as tosses: `PlayerPorts.rightClicked` marks a click on a split half, `opened` keeps a Session for it (counted in `splitSessions`), and `closed` books the player's change as a PLAYER_SESSION port to the district with `NO_INVENTORY`, as a toss is; test `aSessionAtAHalfOfASplitChestIsBookedToTheDistrictAlone` (a menu held open across sweeps, goods taken from the other half, a throw out of the menu, a single-chest control). The audit of the other paths that book goods to a store (hoppers and droppers book nothing; comparators, sweeps, reconciles, probes and the break resolve to the half) is in docs/replay-gate.md residual 13. |

**Tests.** Core, slice U-b's `SavedAheadGameTests` (batch `mic_rtsim_saved_ahead`), `ChestPairLoadGameTests`; create's
`IndustryGameTests`. The `SavedAheadGameTests` force a chunk pair A and N and build the chest across their border, P the
LEFT half (facing north) at A's east edge, Q the RIGHT half at N's west edge. A crash is emulated on live state: what it
keeps stays, what it rolls back (A's epoch, A's carried records, the journal's applied index) is undone by hand, and the
engine's op is sent again under the same id. The tests were written first (10c3b504) and run on the carry code, which is
what the "before" column shows (core and create on 483ee13f's code, `/tmp/uf5-ff2-core.log` and
`/tmp/uf5-ff2-create.log`, the tests of d4d3c5a4 but for the seed of the district test). "After" is the per-half code:
core 146 of 146 required tests passed, create 247 of 247 (at 7ce0fd16, merged with main's 589d5f82). On the head
merged with main's 19ef7889 (slice U-g; 02490183, code as of 7ce0fd16 and U-g): core 151 of 151, create 248 of 248.

| Test | Before (carry code) | After (per half) |
|---|---|---|
| `aReconcileIntoASplitHalfWritesOnlyThatChunk` (+100 into P, room for 24) | requested 100, applied 100: P 64, Q 76, N carries the op | requested 100, applied 24: P 64, Q 0, N not unsaved, carries nothing, no epoch |
| `aSplitHalfRolledBackWithItsEpochTakesTheResendOnce` (+20, crash rolls A back) | results [APPLIED, DUPLICATE_OP], P 40: a loss | [APPLIED, APPLIED], P 60, Q 0: exact |
| `aSplitHalfKeptByTheCrashRefusesTheResendByItsEpoch` (+20, crash keeps A) | [APPLIED, DUPLICATE_OP], P 60: exact | the same: exact |
| `aRemovalWhoseGoodsSitInTheOtherHalfIsAShortfall` (-20 at P, goods in Q) | applied -20 from Q, in N, which carries the op | applied 0, P 0, Q 40, N untouched: a shortfall the engine books |
| `aReconcileHeldForAnUnloadedHalfAppliesOnceAfterTheOtherChunkIsAcked` | the first apply went into Q (P 0, Q 20), the case the carry design could not close | P 20, Q 0 after the first apply, and after the held one [APPLIED, APPLIED], P 20: once |
| `aTransferIntoAFullSplitHalfFailsWhileTheOtherHalfHasRoom` | APPLIED, the stack in Q (P 0, Q 20) | FAILED, nothing placed |
| `aTransferIntoASplitHalfIsCarriedByThatChunkOnly` | P 0, Q 20, A and N both carry the result | P 20, Q 0, A carries it, N does not |
| `aComparatorByTheFarHalfFollowsAWriteThroughTheNearHalf` | output 1 (the pair's own `setChanged`) | output 1 (`StockAccess.splitWritten`) |
| `aChestThatOnlyLooksLikeAPairIsNotCarriedInTheOtherChunk` | failed: N carried an op that did nothing to N | passes: each chest holds its delta whole, N carries nothing |
| `aSimBreakOfAPairsFirstHalfDropsOnlyThatHalf`, `aSimBreakOfAPairsSecondHalfDropsOnlyThatHalf` (U-f9) | first half: passes (the second-half test did not exist) | pass |
| `ChestPairLoadGameTests.aDoubleChestIsClassifiedAndDescribedWithoutWaitingForAChunkStillLoading` | failed at "each half is its own inventory" | passes: 27 slots whole or loading, nothing waits for N |
| `ChestPairLoadGameTests.aSplitDoubleChestIsTwoInventoriesInOneDistrict` | failed at "the west half's chunk is in a district": the test placed no seed (a chest starts no district, fixed in d4d3c5a4), so it was not red on its own assertion | passes: two chunks, two inventories of 27 slots, one district |
| create `IndustryGameTests.aDoubleChestAcrossAChunkBorderIsTwoInventories` | failed at "an inventory for each half, at its own position" | passes |
| create `IndustryGameTests.aDoubleChestIsOneInventory` (a pair in one chunk) | passes | passes: 54 slots at the lower half, unchanged |

On the carry code 9 core tests and 1 create test failed. Of the 9 core ones, 8 failed on the assertion they were
written for and the ninth, the district test, failed for want of a seed, so 9 tests were red on their own assertion
before the change (8 in core, 1 in create), and every per-half test that can be red was. Three tests pass on both codes
by design: the kept-by-the-crash resend, the comparator and the same-chunk break. The unit tests (`:core:test`) pass on
the per-half code: 287 tests at 7ce0fd16, 298 on the merged head.

Not covered: a mod's `ChestBlock` subclass and a `CompoundContainer` another mod builds (residual 9); a Create vault
and other multi-block storages (residual 12); Create's glass compile reads a store's goods through the pair's
capability, so at a split half it still counts the pair's 54 slots (residual 13, read and not run).
