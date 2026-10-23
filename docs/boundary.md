# The Java/engine boundary

**Status:** the contract between the core bridge `mic_rtsim` (Java) and the engine (Rust,
`crates/rtsim-jni`, a mic_native plugin). Written 2026-10-20 with the Java side and its pure-Java
stand-in, `StubEngine`; the Rust side followed the same night (§5). Both sides mirror the layout:
`neoforge/core/src/main/java/com/minecraftindustrialcomplex/mic_rtsim/boundary/Layout.java`,
`Obs.java` and `Cmd.java` in Java, `crates/rtsim-jni/src/wire.rs` in Rust. This page and those four
files change together.

**Rules it follows** (PLAN.md §4.5, §4.9):
- Four buffers in all, two per direction, allocated by the engine and reused forever.
- One hand-over per tick, at most. One JNI call per tick submits the step.
- The bridge never waits. Commands are at least one tick late.
- Fixed binary records, no format version before release. A change is made on both sides at once.

## 1. The buffers

| Buffer | Writer | Reader | Holds |
|---|---|---|---|
| O0, O1 | Java | engine | observations |
| C0, C1 | engine | Java | commands |

- **Memory.** Each buffer is one engine allocation: a 64-byte header, then the record area. The base
  address is 64-byte aligned. Java sees each one as a direct `ByteBuffer` (JNI `NewDirectByteBuffer`)
  and reads and writes it with absolute gets and puts only.
- **Byte order** is little-endian everywhere. Both engine targets (x86-64, AArch64) are little-endian,
  so neither side swaps.
- **Sizing** is the engine's: capacity goes in the header (`capacity`), and the engine sizes it from
  the per-player puppet caps and per-tick quotas so a normal tick fits. It must be at least 65,656
  bytes (`Layout.MIN_CAPACITY`: the largest record plus SESSION_START and TICK), or a largest
  journaled record could never be sent. The stub uses 256 KiB per buffer, a PLACEHOLDER.

### 1.1 Header (64 bytes, every buffer)

| Offset | Type | Name | Written by | Meaning |
|---|---|---|---|---|
| 0 | u64 | `published` | producer, **release** | the hand-over number whose records the buffer now holds; 0 = never |
| 8 | u64 | `consumed` | consumer, **release** | the last hand-over number the consumer has finished with |
| 16 | u32 | `capacity` | allocator, once | bytes in the record area |
| 20 | u32 | `used` | producer | bytes of records, counted from offset 64 |
| 24 | u32 | `count` | producer | number of records |
| 28 | u32 | `flags` | producer | below |
| 32 | u64 | `wordA` | producer | O: `lastAppliedCmdSeq`. C: `ackedObsSeq` |
| 40 | i64 | `wordB` | producer | O: game time at the hand-over. C: game time the step reached |
| 48 | u64 | `wordC` | producer | O: highest obsSeq in the buffer (0 if none). C: highest cmdSeq |
| 56 | u64 | reserved | | zero |

Only `published` and `consumed` are atomic. Java uses
`MethodHandles.byteBufferViewVarHandle(long[].class, LITTLE_ENDIAN)` with `getAcquire` and
`setRelease`; Rust uses `AtomicU64` at the same offsets with `Ordering::Acquire` and
`Ordering::Release`. A release store of `published` makes the records and the rest of the header,
written before it with plain stores, visible to whoever reads `published` with acquire and sees the
new value. The same holds for `consumed` in the other direction.

**Observation flags:** 1 `SESSION_START` (the first hand-over of a session), 2 `REPLAY` (holds
journal entries from before this session), 4 `DROPPED` (latest-wins records were dropped for space).
**Command flags:** 1 `MORE` (the engine holds more commands in its overflow ring for later steps),
2 `FAILED` (the engine failed, a panic or a protocol error, and simulates nothing more this session:
its buffers come back empty from now on, so the bridge is never left waiting. `BoundarySession`
turns the flag into the bridge's session failure, B30: puppets captured, the journal kept for the
next session).

### 1.2 The hand-over

Hand-overs are numbered h = 1, 2, 3 ... per session. Hand-over h puts its observations in
**O[h & 1]**, and the step it starts writes its commands into **C[h & 1]**.

Java, at the end of every server tick:

1. **Apply commands.** For k = the oldest hand-over not yet applied, while k has been submitted:
   if `C[k&1].published != k` (acquire), stop: step k is still running. Otherwise apply its records
   from where the last tick stopped, until a quota defers one. When every record is done, store
   `C[k&1].consumed = k` (release) and go on with k + 1. The first time Java opens C[k&1] it reads
   `ackedObsSeq` and trims the journal.
2. **Write observations** into the open buffer, O[(last submitted + 1) & 1], after anything already
   there. While a step is running, observations of several ticks pile up in it.
3. **Submit**, only when both hold:
   - step h-1 has finished: `C[(h-1)&1].published == h-1` (or h = 1);
   - Java has finished applying step h-2's commands, so C[h&1] is free to overwrite.

   Then write O[h&1]'s header, store `O[h&1].published = h` (release), and make the one JNI call,
   `submit(h)`. It returns at once.

The engine, for step h:

1. Check `O[h&1].published == h` (acquire). Read every record.
2. Store `O[h&1].consumed = h` (release). Java checks this before reusing the buffer.
3. Simulate. If a save was requested, snapshot the pages at this step boundary.
4. Check `C[h&1].consumed == h-2` (or h <= 2). Write commands; whatever does not fit stays in the
   overflow ring and sets `MORE`. Write the header (`ackedObsSeq` = the highest observation sequence
   that pages already **on disk** include).
5. Store `C[h&1].published = h` (release).

At most one step is ever in flight. If the engine falls behind, Java keeps collecting observations
in the open buffer and applies nothing new; it never blocks. If Java falls behind on commands (a
quota), it does not submit the step that would overwrite them; the sim slows rather than the tick.

### 1.3 Calls (JNI, rtsim-jni mirrors `boundary/Engine.java`)

| Call | When | Blocks? |
|---|---|---|
| `observationBuffer(i)`, `commandBuffer(i)` | once per session | no |
| `start(lastAppliedCmdSeq) -> (pagesObsSeq, nextCmdSeq)` | once, at server start, before hand-over 1 | may (startup) |
| `submit(h)` | at most once per tick | **never** |
| `requestSave(throughObsSeq)` | Minecraft's overworld save event | never (a signal) |
| `close()` | server stopped | may, briefly |

`start` loads the pages and zeroes the four buffers' `published` and `consumed` words (Java refuses
buffers that are not fresh: hand-overs restart at 1 every session). `pagesObsSeq` is the highest
observation sequence the pages include; Java moves its next observation sequence past it, so a world
save older than the pages never reuses a sequence. `nextCmdSeq` must be above `lastAppliedCmdSeq`.

`requestSave(throughObsSeq)`: the world's journal holds every observation up to `throughObsSeq`. The
engine snapshots its pages after applying exactly those (before the first journaled record past it),
never further: pages ahead of the world save are P29's case. The stub does this.

**Acknowledgements** (`ackedObsSeq`) are trusted only from a well-formed header, are clamped to what
Java sent (or the pages held at start), and trim the journal only after that buffer's commands are
applied, so a command in the same buffer that re-issues an acknowledged op is still caught.

## 2. Records

Every record starts on an 8-byte boundary, and its size is a multiple of 8 (at most 65,528). Padding
and reserved fields are zero.

### 2.1 Record header (8 bytes)

| Offset | Type | Name |
|---|---|---|
| 0 | u16 | `kind`: observations below 0x8000, commands 0x8000 and up |
| 2 | u16 | `size` in bytes, header included |
| 4 | u32 | `aux`: a small kind-specific field |

A reader that meets an unknown kind skips `size` bytes. A malformed size ends the buffer.

### 2.2 Ids

- **Agent ids** are u64, the engine's (64-bit ids exist only at the boundary, PLAN.md §5.3).
  **Epochs** are u32, new for every materialization.
- **Dictionaries.** Dimensions, items and body kinds travel as u32 ids. Each direction has its own
  dictionary, defined by its writer with `DICT` records: kind 1 dimension, 2 item, 3 body kind (group
  kinds and member bodies share it; residents' entity types too, e.g. `minecraft:villager`), and for
  phase 1 (§4): 4 structure (`minecraft:village_plains`), 5 culture (`human`, `ribbit`: rtsim-settle's
  `Culture::name`), 6 profession (`minecraft:farmer`, `ribbits:merchant`), 7 structure template
  (`ribbits:houses/small_house_red_1`), 8 processor list (`ribbits:main`), 9 work kind
  (`mic_rtsim:plains_lamp`, SINK.md §4.2). Ids start at 1.
  - Java's dictionary is per **world**: it is saved in the journal, because a journaled observation
    replayed in a later session must mean the same thing. Java sends the whole of it again at every
    session start. A `DICT` always comes before, in the same or an earlier hand-over, any record that
    uses its id.
  - The engine's dictionary is per **session**: Java forgets it at every start, so the engine sends
    it again.
- **Operation ids** (u64) mark every cross-store transfer, as in rtsim-kernel's `journal::OpId`
  (decision log O10). Java's have the top bit set (`JAVA_OP_BIT`, `OpId::bridge`) over 63 random
  bits. They are not the record's obsSeq: a crash can roll the journal back while a chunk saved on
  unload keeps a record from the lost stretch (§3.1), and the lost sequences are then handed out
  again. The engine's op ids have the top bit clear (`OpId::engine`). When the engine re-derives a
  transfer after a crash it must give it the same op id, or the bridge cannot tell it is a duplicate;
  and a *different* transfer must never get an id an earlier one had, or the bridge, still holding
  the first one's COMMAND_RESULT, answers DUPLICATE and drops it. So an engine op id is derived from
  the transfer's own identity (what the sim decided, and when), never taken from a counter restored
  from the pages: such a counter hands the ids it gave out since the last save to new transfers
  after a crash. Transfers that come from outside the sim (test and debug orders) take random ids.
  Bits 56 to 62 name the engine allocator (0 for the core engine; the industry engine uses 0x49).

### 2.3 Observations (Java to the engine)

**Latest-wins kinds** (below 0x0100) are state. When a buffer is full they are dropped and counted,
and sent again at the next pass. **Journaled kinds** (0x0100 to 0x7FFF) are events. They all start
with this prefix, are kept in the journal until the engine acknowledges them, and are never dropped:

| Offset | Type | Name |
|---|---|---|
| 8 | u64 | `obsSeq`: per world, increasing, never reused |
| 16 | i64 | `gameTime` when it happened (it may be sent ticks or sessions later) |

The engine skips a journaled record whose `obsSeq` its pages or memory already include, and applies
an operation id at most once.

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0001 | SESSION_START | 0 | 8 i64 gameTime, 16 u64 lastAppliedCmdSeq, 24 u64 journalFirstObsSeq (0 = empty), 32 u64 worldNextObsSeq, 40 u32 moduleMask, 44 u32 reserved | 48 |
| 0x0002 | DICT | dict kind | 8 u32 id, 12 u32 nameLen, 16 UTF-8 name, zero-padded | 16 + name, aligned |
| 0x0003 | TICK | 0 | 8 i64 gameTime. Starts one tick's latest-wins records | 16 |
| 0x0004 | MODULES | 0 | 8 u32 mask, 12 u32 reserved. At start and on change | 16 |
| 0x0005 | WORLD_SAVED | 0 | 8 u64 durableObsSeq: a world save holding every observation up to it is on disk. Once per save | 16 |
| 0x0010 | PLAYER | dimension | 8 u64 uuidMost, 16 u64 uuidLeast, 24 f64 x, 32 f64 y, 40 f64 z. Every 20 ticks per player | 48 |
| 0x0011 | PUPPET_STATE | member index | 8 u64 agent, 16 u32 epoch, 20 u32 dimension, 24 f32 health, 28 u32 flags, 32 f64 x, 40 f64 y, 48 f64 z | 56 |
| 0x0100 | MATERIALIZED | member count | 24 u64 agent, 32 u32 epoch, 36 u32 dimension | 40 |
| 0x0101 | SPAWN_REJECTED | reason | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 cmdSeq | 48 |
| 0x0102 | DEMATERIALIZED | reason | 24 u64 agent, 32 u32 epoch, 36 u32 members captured | 40 |
| 0x0103 | MEMBER_STATE | member index | see below | 96 + 16 per item |
| 0x0104 | TAKEN | how | 24 u64 agent, 32 u32 epoch, 36 u32 member, 40 u64 playerMost, 48 u64 playerLeast, 56 u64 opId | 64 |
| 0x0105 | DIED | member index | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 killerMost, 48 u64 killerLeast, 56 u64 opId | 64 |
| 0x0110 | TRANSFER | transfer kind | 24 u64 opId, 32 u64 from, 40 u64 to, 48 u32 item, 52 reserved, 56 i64 amount | 64 |
| 0x0111 | COMMAND_RESULT | status | 24 u64 cmdSeq, 32 u64 opId | 40 |
| 0x0020 to 0x0022, 0x0120 to 0x0126 | phase 1 | | §4.1 | |

**MEMBER_STATE** (what a body adapter captured):

| Offset | Type | Name |
|---|---|---|
| 24 | u64 | agent |
| 32 | u32 | epoch |
| 36 | u32 | bodyKind (dictionary) |
| 40 | u32 | dimension (dictionary) |
| 44 | u32 | flags: 1 leashed to the leader, 2 has a chest, 4 tamed, 8 leashed elsewhere, 16 ticking |
| 48 | f32 | health |
| 52 | f32 | maxHealth |
| 56, 64, 72 | f64 | x, y, z |
| 80 | i32 | bodyA (adapter-defined; trader llama: variant id) |
| 84 | i32 | bodyB (adapter-defined; trader llama: strength) |
| 88 | u32 | itemCount |
| 92 | u32 | reserved |
| 96 | | itemCount items of 16 bytes: u32 slot, u32 item (dictionary), i32 count, u32 reserved |

A MEMBER_STATE always comes right before the event that captured it: DEMATERIALIZED (one per
captured member before it), TAKEN or DIED.

**Codes:**
- SPAWN_REJECTED reasons: 1 not ticking, 2 unknown group kind, 3 unknown dimension, 4 already embodied,
  5 spawn failed, 6 bad record, 7 the server would discard the body (`spawn-animals` or `spawn-npcs`
  off), 8 bad cargo (an item id with no item, a stack bigger than one stack, more stacks than the
  chest holds; cargo is never dropped silently).
- DEMATERIALIZED reasons: 1 command, 2 a member stopped entity-ticking, 3 a member left tracking or
  unloaded, 4 session end, 5 no members left (all taken or dead), 6 not embodied: the answer to a
  DESPAWN_GROUP for an agent or epoch with no puppet (0 members), so the engine never waits.
- TAKEN how: 1 lead, 2 taming, 3 pickup (Carry On, later plugin), 4 removed by vanilla or another mod,
  5 a plugin.
- TRANSFER kinds: 1 generic (the only one so far).
- COMMAND_RESULT status: 1 applied, 2 duplicate op id (already applied, skipped), 3 failed; phase 1
  adds 4 not free (PLACE_HOUSE found no free plot) and 5 vanilla knows (WRITE_GOSSIP refused: vanilla
  already records that act first-hand).

**SESSION_START and the kernel's `Replay::reconcile`.** `journalFirstObsSeq` and `worldNextObsSeq`
are the two numbers `reconcile(journal_first, world_next)` takes. `worldNextObsSeq` is the world
save's own next sequence, read before Java moves past `pagesObsSeq`, so a world save older than the
pages shows up as `EngineAhead` (P29). Java's sequences in this session then start at
`max(worldNextObsSeq, pagesObsSeq + 1)`, which keeps `Replay::admit` contiguous.

**WORLD_SAVED and `Replay::forget_ops_through`.** The bridge records `nextObsSeq - 1` at the
overworld's save event and chains a marker onto NeoForge's SavedData write chain
(`IOUtilities.withIOWorker`), so the marker runs after the journal's write task. It reports the
smaller of that value and the last sequence the journal's file on disk actually holds
(`JournalSavedData.writtenThrough`): a write the chunk barrier held, or one that failed, does not
count (docs/choices-journal-barrier.md JB1). The next hand-over carries the value; the engine may then
forget op ids applied at or before it.

**Idempotent events.** TAKEN, DIED and COMMAND_RESULT may arrive more than once (§3.1). The engine
applies each at most once: by op id while it remembers it, and by state after that (a member that
is already gone, an op that is already done).

**Deaths are journaled at the end of the tick** they happen in, once they are certain: another mod
can still cancel a death after the bridge saw LivingDeathEvent.

**What a session start means:** puppets never outlive a session. A replayed MATERIALIZED (a record
from before this session, flagged `REPLAY`) never makes an agent embodied again. At SESSION_START every agent the
engine thought embodied is abstract again, with the last state it knew. The DEMATERIALIZED records
of a clean shutdown come later, as journal replay.

### 2.4 Commands (the engine to Java)

Every command has a `cmdSeq` at offset 8: per world, increasing. A DICT's `cmdSeq` means nothing,
and the native engine writes 0: Java never skips a DICT, but it does record a DICT's sequence as
applied, so a DICT numbered above a command that follows it (a transfer resent after a restart, with
its original sequence) would make Java skip that command. Java skips any command except DICT
at or below the journal's `lastAppliedCmdSeq`, which it also sends back in every observation header
as `wordA`. **The engine does not trim its outbox by `wordA`**: Java's live value can be ahead of
the world save the next page snapshot pairs with (a transfer applied after an autosave, in the same
tick), and a snapshot trimmed by it would lose that transfer on a crash. A transfer leaves the outbox
when the engine admits its COMMAND_RESULT (applied, duplicate or failed) through `Replay`, so the
outbox in a snapshot taken at observation N holds exactly the transfers the world save at N lacks
(`Outbox::complete`). At `start` the engine trims by the `lastAppliedCmdSeq` it is given, which is
the world save's own (`Outbox::ack_through`). After a restart the engine resends its outbox
(`Outbox::after`) with the commands' original sequences; Java applies those above `lastAppliedCmdSeq` and skips any
whose op id it already applied. New commands must get sequences above `lastAppliedCmdSeq` too, so
the engine starts its counter at `max(next_seq, lastAppliedCmdSeq + 1)` (`start` returns it). Bodies
start at 16.

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8001 | DICT | dict kind | 16 u32 id, 20 u32 nameLen, 24 UTF-8 name | 24 + name, aligned | |
| 0x8002 | SPAWN_GROUP | member count | 16 u64 agent, 24 u32 epoch, 28 u32 groupKind, 32 u32 dimension, 36 u32 flags (1 SURFACE: each member stands on the highest motion-blocking, non-leaf block at its own x, z, found once its position is entity-ticking), 40 f64 x, 48 f64 y, 56 f64 z, 64 member blocks | 64 + blocks | MATERIALIZED or SPAWN_REJECTED |
| 0x8003 | DESPAWN_GROUP | 0 | 16 u64 agent, 24 u32 epoch, 28 u32 reason | 32 | DEMATERIALIZED |
| 0x8004 | STEER_GROUP | 0 | 16 u64 agent, 24 u32 epoch, 28 f32 speed, 32 f64 x, 40 f64 y, 48 f64 z | 56 | |
| 0x8005 | TRANSFER_TO_CONTAINER | 0 | 16 u64 opId, 24 u32 dimension, 28 u32 item, 32 i32 x, 36 i32 y, 40 i32 z, 44 i32 count | 48 | COMMAND_RESULT |
| 0x8010 to 0x8015 | phase 1 | | §4.2 | | |

**SPAWN_GROUP member block** (24 bytes, then its items): 0 u32 member index, 4 u32 itemCount, 8 f32
health (0 = full), 12 i32 bodyA (-1 = let the body roll it), 16 i32 bodyB (-1 = roll), 20 reserved,
24 itemCount items of 16 bytes as in MEMBER_STATE (item ids from the engine's dictionary).

**How Java applies them:**
- **Spawns and despawns** count bodies against the per-tick quota (PLACEHOLDER 8), a whole group at a
  time. A group that does not fit in what is left of this tick's quota waits, with every command
  after it, for the next tick; a group bigger than the whole quota goes alone at the start of a tick.
  All members of a group spawn in the same tick. A spawn is rejected unless every member's position
  is entity-ticking.
- **Steering** points the group's leader at the target; members follow it.
- **TRANSFER_TO_CONTAINER** is one stack (count up to the item's stack size; split bigger ones). It
  is all or nothing, through the block's NeoForge item handler. An op id whose COMMAND_RESULT(applied)
  is still in Java's journal is answered DUPLICATE and not applied again.

## 3. The journal (A9, PLAN.md §4.9)

`mic_rtsim:journal` is a SavedData in the overworld's data storage, `data/mic_rtsim_journal.dat` on
disk. It holds:
- `lastAppliedCmdSeq`;
- every journaled observation the engine has not acknowledged, as the exact record bytes;
- `nextObsSeq`;
- Java's dictionary.

**Replay is the normal path.** Journaled observations always reach the engine from the journal: at
each hand-over Java copies the entries it has not sent yet this session. At a session start it has
sent none, so everything unacknowledged goes out again, flagged `REPLAY`. The engine acknowledges
(`ackedObsSeq`) only what pages on disk include, and Java then trims.

**Op ids, both ways:**
- Java to the engine: the engine skips by `obsSeq` first, then by op id. A replayed TRANSFER the
  pages include is skipped; the same op id under a new sequence is a duplicate.
- The engine to Java: Java skips by `cmdSeq`, then by op id. When Java applies an op it journals
  COMMAND_RESULT(applied, opId), and the op id stays in the journal's index until the engine
  acknowledges that record. An engine that a crash rolled back re-derives the op under a new
  `cmdSeq`; Java answers DUPLICATE. The replayed COMMAND_RESULT tells the engine the op is done.

### 3.1 Chunks saved ahead of the journal

Minecraft saves a chunk, its blocks or its entities, whenever it unloads, but the journal only at an
autosave. A chunk on disk can therefore be newer than the journal: a chest that received an engine
transfer, a llama a player took, the items a dying caravan llama dropped. After a crash the journal
would not know them, and the engine (as old as the journal, or older) would resend the transfer or
bring the llama and its cargo back.

So the world side carries its own records, in the `mic_rtsim:pending` attachment, until the engine
acknowledges them **in the same session**:
- a chunk carries the COMMAND_RESULT of every engine transfer applied into it. Before applying a
  TRANSFER_TO_CONTAINER, Java looks there: an op id found in the chunk is answered DUPLICATE;
- a taken creature carries its MEMBER_STATE and TAKEN;
- each item a dying member dropped carries its MEMBER_STATE and DIED.

When the holder loads in a later session (a chunk load, an entity read from disk), Java journals the
records again: the same bytes and op ids, new sequences. It drops them from the holder once the engine
acknowledges a sequence at or past them in this session; within one session sequences are never
reused, so that comparison is exact. A known gap: an item picked up before the acknowledgement takes
its record into a player's inventory, where nothing carries it.

**The other way round** (the journal ahead of a chunk): vanilla writes the journal in `saveLevelData`,
before it serializes any chunk, and skips a chunk saved in the last 10 s, so a journal could say applied
while the chunk on disk did not have it, and the engine would never send the command again. Every chunk
a journaled record speaks for (the holders above, and a reconcile's chunk) is noted when the record is
committed; at the journal's write those still unsaved are saved, the levels' chunk storage writes those
chunks ahead of its queue, and the journal's write waits on the IO chain for them. If they are not on disk
in time (or one is still being unloaded), the file is held to the next world save, never written ahead of
them (`ChunkWriteBarrier`, docs/choices-journal-barrier.md JB1 to JB5; first built as docs/choices-repair.md
R21 to R24). Entities that carry records are not covered.

**Still open:** a crash that leaves the engine's pages ahead of the world save (Proposal P29). A held or
failed journal write widens that window to the next world save; WORLD_SAVED no longer reports it, so P29's
`confirm_through` will keep the older page generation (docs/choices-journal-barrier.md JB1, JB6).

## 4. Phase 1: caravans and markets (PLAN.md §8)

Written 2026-10-20 with the Java side (`Obs.java` and `Cmd.java` hold the offsets; choices G1 to G30
in `docs/overnight-choices-phase1mc.md`). Units are rtsim-trade's: stocks and treasuries in
thousandths of an item or coin, prices in millionths of a coin per whole item; players and the bridge
only ever move whole items, so a whole item is 1,000 at the boundary. Culture, gossip type and report
kind numbers are rtsim-settle's and rtsim-knowledge's.

**Ids.** A village is its structure start: `villageId = dim (14 bits) << 48 | startChunkX (24 bits) << 24
| startChunkZ (24 bits)`, two's complement, the top two bits clear (rtsim-settle's "packed chunk position
of its structure start"). A player base is `1 << 62 | dim (14 bits) << 48 | cellX << 24 | cellZ` over
128-block cells. Both are Java's and stable across sessions (the dimension id is Java's per-world
dictionary id).

### 4.1 Observations

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0020 | VILLAGE_CENSUS (latest-wins) | 0 | 8 u64 village, 16 u32 dimension, 20 u32 flags (1 the meeting point is a meeting POI), 24 i32 meetX, 28 meetY, 32 meetZ, 36 u32 residents, 40 u32 beds, 44 u32 workstations, 48 u32 golems, 52 u32 n, 56 n × (u32 profession, u32 count) | 56 + 8n |
| 0x0021 | VILLAGE_GOSSIP (latest-wins) | 0 | 8 u64 village, 16 u32 n, 20 u32 residents, 24 n × (u64 playerMost, u64 playerLeast, i32 major_negative, i32 minor_negative, i32 minor_positive, i32 major_positive, i32 trading, u32 reserved): vanilla's own gossip about each player, summed over the census's villagers | 24 + 40n |
| 0x0022 | WORK_CATALOG (latest-wins) | 0 | 8 u32 kinds n, 12 reserved, 16 n × kind: 0 u32 id (dictionary 9), 4 u32 group (0 lamp, 1 decor, 2 golem), 8 u32 culture, 12 u32 style (structure; 0 every village of the culture), 16 u32 cap (1 one per capN residents, 2 capN a village), 20 u32 capN, 24 u32 variants v, 28 reserved, then v × variant: 0 u32 place (1 template, 2 body), 4 u32 what (template or body), 8 u32 processors (0 none), 12 u32 blocks, 16 u32 items m, 20 reserved, then m × (u32 item, u32 count) | 16 + Σ(32 + Σ(24 + 8m)) |
| 0x0120 | VILLAGE_ADOPTED | flags (1 LATE) | 24 u64 village, 32 u32 dimension, 36 u32 structure, 40 u32 culture, 44 u32 pieces, 48 i32 minX, 52 minY, 56 minZ, 60 maxX, 64 maxY, 68 maxZ, 72 i32 meetX, 76 meetY, 80 meetZ, 84 reserved | 88 |
| 0x0121 | TRADE | side | 24 u64 opId, 32 u64 node, 40 u64 agent, 48 u32 epoch, 52 u32 member, 56 u64 playerMost, 64 u64 playerLeast, 72 u64 merchantMost, 80 u64 merchantLeast, 88 u32 costA item, 92 i32 costA count, 96 u32 costB item (0 none), 100 i32 costB count, 104 u32 result item, 108 i32 result count, 112 u32 merchant body, 116 u32 profession (0 none) | 120 |
| 0x0122 | HURT | what | 24 u64 agent, 32 u32 epoch, 36 u32 member, 40 u64 victimMost, 48 u64 victimLeast, 56 u64 attackerMost, 64 u64 attackerLeast, 72 u32 attacker kind, 76 f32 amount, 80 u64 node, 88 u32 victim body, 92 u32 flags, 96 u32 hits, 100 u32 profession | 104 |
| 0x0123 | RESCUE | what (1 puppet, 2 resident) | 24 u64 agent, 32 u32 epoch, 36 u32 member, 40 u64 rescuerMost, 48 u64 rescuerLeast, 56 u64 victimMost, 64 u64 victimLeast, 72 u32 threat (the mob's entity type, body dictionary), 76 reserved, 80 u64 node | 88 |
| 0x0124 | CARGO_CHANGED | member | 24 u64 opId, 32 u64 agent, 40 u32 epoch, 44 u32 flags (1 the player is the nearest one), 48 u64 playerMost, 56 u64 playerLeast, 64 u32 n, 68 reserved, 72 n items of 16 bytes as in MEMBER_STATE (slot 0xFFFFFFFF, count = signed delta, negative = taken out) | 72 + 16n |
| 0x0125 | BASE | 1 MARKED, 2 UNMARKED | 24 u64 base, 32 u32 dimension, 36 reserved, 40 f64 x, 48 f64 y, 56 f64 z (where the player stayed, a weighted mean), 64 u64 playerMost, 72 u64 playerLeast, 80 f32 dwell, 84 reserved | 88 |
| 0x0126 | HOUSE_PLACED | 0 | 24 u64 opId (the PLACE_HOUSE's), 32 u64 village, 40 u32 dimension, 44 u32 template, 48 i32 minX, 52 minY, 56 minZ, 60 maxX, 64 maxY, 68 maxZ, 72 i32 slotX, 76 slotY, 80 slotZ, 84 u8 rotation, 85 u8 mirror, 86 reserved | 88 |

All of 0x0120 to 0x0126 are journaled (the 24-byte prefix of §2.3).

**Codes:**
- TRADE side: 1 the merchant sold goods for its currency (the player bought), 2 the merchant bought
  goods, 3 any other shape (two costs, enchanted or other component-bearing results). For a resident the
  side is decided against its village market's currency; for a puppet it is the engine's own offer flag
  (MERCHANT_OFFERS).
- HURT what: 1 a puppet member hurt, 2 a resident hurt, 3 a resident killed (a puppet's death is DIED).
  Attacker kind: 0 none, 1 player, 2 mob, 3 other entity. Flags: 1 VANILLA_GOSSIP (a villager hurt or
  killed by a player: vanilla already wrote its gossip, the engine must not write it again), 2 LETHAL.

**What each one means:**
- **VILLAGE_ADOPTED** (C22): a structure start of a village structure generated (a mixin on
  `StructureManager.setStartForStructure`, which `ChunkGenerator.tryGenerateStructure` calls during the
  chunk's STRUCTURE_STARTS step), or, flag LATE, a chunk loaded holding a village start the registry did
  not know (a world made before the mod). After a crash a start can be seen again: the engine treats a
  village id it already has as a no-op. The meeting point is the start piece's centre until a census
  finds a meeting POI (the bell), which VILLAGE_CENSUS then reports.
- **TRADE** (C14, C21, C23): one per trade, at `MerchantResultSlot.onTake` right after
  `Merchant.notifyTrade`, for an adopted village's resident (agent 0, node = its village) or a puppet
  merchant (node = the village or player base it stands in, else 0). The counts are what was really
  paid and received, reputation included. The op id is the trade's transfer: the engine applies it to the
  village market's stock and treasury, or to the puppet's cargo, exactly once. The bridge has already
  moved its own cached market (§4.3) and the puppet's real cargo.
- **HURT**: hits on one victim by one attacker are summed over a window (100 ticks, PLACEHOLDER) into one
  record. A resident's death is RESIDENT_KILLED at once, journaled at the end of its tick.
- **RESCUE** (C24): a player killed a mob whose target was a puppet member or a resident; journaled at the
  end of the tick once the mob is really dead.
- **CARGO_CHANGED** (robbery): a puppet's own cargo (a llama's chest, a ribbit merchant's pack) changed
  other than by the bridge (spawn, trades). Signed deltas; the op id is the transfer out of (or into)
  the cargo. In vanilla an untamed trader llama's chest cannot be opened, so this is mostly other mods.
- **BASE** (C26, P30's recommendation): a player's decaying dwell counter for a 128-block cell crossed the
  mark (MARKED) or fell below a quarter of it (UNMARKED). One record per (base, player).
- **HOUSE_PLACED**: a PLACE_HOUSE finished; the slot is where a resident of the house stands (above the
  template's resident-pool jigsaw, else the floor's middle). Its COMMAND_RESULT(APPLIED) follows.
- **VILLAGE_CENSUS / VILLAGE_GOSSIP**: one loaded village every 100 ticks (PLACEHOLDER), round robin, from
  loaded chunks only; puppets are never residents. The census's golems are what the engine counts as
  the village's defenders (SINK.md MS-8), latest wins.
- **WORK_CATALOG** (SINK.md §4.2, the money sink): what a village's works purse buys, from the data at
  `data/<ns>/mic_rtsim/work/<id>.json` (`village/WorkCatalog.java`). A template's blocks and items are
  read from its palette (air and structure voids not counted, a jigsaw as its final state, a door or a
  bed once); a body's items are the data's. Sent as a preamble record once a session and again after a
  data reload; the engine takes it while phase 1 is switched off too. A work the engine buys comes
  back as a PLACE_HOUSE (a template, SEARCH and Y_FROM_VILLAGE around the meeting point) or a
  SPAWN_RESIDENT with AT_VILLAGE (a body); its COMMAND_RESULT tells the engine whether it stands:
  APPLIED or DUPLICATE placed it, NOT_FREE or FAILED mark the kind full for that village, with no
  refund (MS-11). HOUSE_PLACED for a work changes nothing in the engine.

### 4.2 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8010 | MARKET | goods n | 16 u64 village, 24 u64 asOfObsSeq, 32 u32 currency (item), 36 u32 reserved, 40 i64 treasury, 48 i64 treasuryTarget (Mbar; 0 never refuses), 56 u8 levels L (1 to 8), 57 u8 depth D (1 to 32), 58 reserved, 64 8 × i32 thresholds (level k applies at standing ≤ it; level 0's is 127; unused zero), 96 n × good | 96 + n × (16 + 16LD) | |
| 0x8011 | MERCHANT_OFFERS | offers n | 16 u64 agent, 24 u32 epoch, 28 u32 member, 32 n × offer | 32 + 40n | |
| 0x8012 | WRITE_GOSSIP | 0 | 16 u64 opId, 24 u64 village, 32 u64 playerMost, 40 u64 playerLeast, 48 u32 gossip type, 52 i32 amount, 56 u32 report kind, 60 u32 flags (1 FIRST_HAND) | 64 | COMMAND_RESULT |
| 0x8013 | SENTIMENT | entries n | 16 u64 holder (village), 24 i32 dislikeAtOrBelow, 28 i32 friendAtOrAbove, 32 u32 flags (1 REPLACE), 36 reserved, 40 n × (u64 playerMost, u64 playerLeast, i32 sentiment, u32 reserved) | 40 + 24n | |
| 0x8014 | PLACE_HOUSE | 0 | 16 u64 opId, 24 u64 village, 32 u32 dimension, 36 u32 template, 40 u32 processors (0 none), 44 i32 x, 48 i32 y, 52 i32 z, 56 u8 rotation, 57 u8 mirror, 58 u16 flags (1 SEARCH, 2 Y_FROM_VILLAGE), 60 u32 radius, 64 u64 seed, 72 i32 yOffset, 76 reserved | 80 | HOUSE_PLACED, then COMMAND_RESULT |
| 0x8015 | SPAWN_RESIDENT | 0 | 16 u64 opId, 24 u64 village, 32 u32 dimension, 36 u32 body (entity type), 40 u32 profession (0 the body's default), 44 u32 flags (1 AT_HOUSE, 2 AT_VILLAGE), 48 f64 x, 56 f64 y, 64 f64 z, 72 u64 house (the PLACE_HOUSE op id) | 80 | COMMAND_RESULT |

**MARKET good** (16 + 16LD bytes): 0 u32 item, 4 u32 reserved, 8 i64 stock (where its ladders start),
16 two ladders, the market's sales (the player buys) then its purchases, each L levels × D steps of
(i32 items, i32 coins); step j of side s (0 sells, 1 buys) at level k is at 16 + ((s × L + k) × D + j) × 8.
A (0, 0) step ends a ladder. A ladder is what a run of trades on that side meets, one bundle after
another, each priced at the market the trades before it left (rtsim-trade `market::ladder`): it ends
where the market refuses (sold out, a glut, cash refused) or the treasury cannot pay the next bundle.
Level 0 is the fair price; level k is `markup_for(threshold k, worst)`, never better than fair. Since
Ben's morning review ("Prices": one source of truth; G31: reputation inside the sim's quote) these
bundles are the price: the bridge computes none (choices PR1, PR2 in
docs/overnight-choices-p1rework.md). The engine's levels are -25, -50, -75 and -100 and its depth 8
(PLACEHOLDERS, `phase1::PRICE_LEVELS`, `LADDER_DEPTH`); MARKET is about 4.7 KiB for the catalogue's
seven goods.

**MERCHANT_OFFERS offer** (40 bytes): 0 u32 costA item, 4 i32 costA count, 8 u32 costB item (0 none),
12 i32 costB count, 16 u32 result item, 20 i32 result count, 24 i32 maxUses (0 = 64, PLACEHOLDER),
28 i32 xp, 32 f32 priceMultiplier, 36 u32 flags (1 the merchant sells, 2 the merchant buys: TRADE's
side). Counts must fit one stack; a bad offer is skipped.

Gossip type: 1 major_negative, 2 minor_negative, 3 minor_positive, 4 major_positive, 5 trading
(rtsim-knowledge `GossipType`). Report kind: rtsim-knowledge `ReportKind` (1 assault ... 8 resident
killing).

### 4.3 How Java applies them

- **MARKET** (C13, C14, C23; latest wins). The bridge keeps the latest per village and shows its
  bundles (PLAN.md §8.5): before a player starts trading (the interaction event), after every trade, and
  uses only at a restock. A **market-shaped** offer (one cost and one result, one of them the currency,
  the other a plain good the market trades) is replaced in place, one for one, by the step of its
  ladder where the bridge's own trades since `asOfObsSeq` left that good's stock (`MarketBook`): the
  sim never adds or removes a resident's offers. The ladder is the player's **price level**: the
  deepest level whose threshold the player's standing is at or below, the standing being vanilla's own
  reputation with that villager (`Villager.getPlayerReputation`, its gossip, which the sim also writes,
  C20) or, for ribbits, the village's SENTIMENT. Vanilla's `updateSpecialPrices` discount (reputation,
  Hero of the Village) is taken off again on these offers (`VillagerReputationMixin`): the reputation is
  inside the engine's quote, so one formula sets the price (G31). Uses: the rest of the ladder for what
  the village sells (none while it refuses cash), the steps the treasury pays for what it buys. A stock
  the ladder does not reach (a sale and a purchase of one good mixed since the MARKET) shows the offer
  crossed out until the next MARKET, which the engine sends right after a trade. Vanilla's demand is
  held at 0 on these offers. A bundle that does not fit one stack is clamped only where that is worse
  for the player, else the offer shows crossed out.
  **asOfObsSeq**: every observation at or below it is in the MARKET. The bridge keeps each of its own
  trades under its TRADE's sequence and replays those above it on top of a new MARKET, so one the
  engine computed before a trade never gives the money back.
  The bridge's copy of the price math (`PriceCurve`, `PriceOracle`) is only a test oracle: unit tests
  check it against rtsim-trade's vectors, and the stub engine prices a test's scripted curves with it.
- **MERCHANT_OFFERS** (C21; latest wins). Replaces a puppet merchant's offers (vanilla's random wandering
  trader trades and Ribbits' rolled merchant trades are dropped at spawn). Every result comes out of the
  member's trade cargo (a wandering trader's: its group's llama chests; a ribbit merchant's: its pack)
  and every payment goes into it; uses = min(maxUses, cargo of the result / result count, room for each
  payment / its count). Ignored for an agent or epoch that is not embodied. The engine's bundles are the
  price (as MARKET's). A caravan standing at a border market its road touches (within 96 blocks,
  PLACEHOLDER) also exchanges emeralds and amethyst at that market's rate (P21): an offer selling the
  foreign coin for the market's own and one buying it back, each rounded against the player; the engine
  sends the offers again as the caravan walks into or out of that reach.
- **WRITE_GOSSIP** (C20). Written with `GossipContainer.add` into every loaded resident villager of the
  village, as the player's entry. **Each villager takes an op id once**: it carries the op ids it was
  written with (`mic_rtsim:gossip_ops`, the last 64, saved with it), so a resend after a crash adds
  nothing to a villager that has it, whatever the journal remembers. Villagers not loaded get it at
  their village's next census, aged by vanilla's own daily decay for that type, until it would have
  decayed away. Answered APPLIED (also when no villager was loaded), DUPLICATE (the journal or the
  village's pending list has the op), FAILED (unknown village, a culture without vanilla gossip, a bad
  type or amount), or VANILLA_KNOWS (flag FIRST_HAND with report kind trade, resident assault or resident
  killing: vanilla writes those itself, PLAN.md §8.7). Vanilla drops an entry whose value is below 2
  (`GossipContainer.EntityGossips.makeSureValueIsntTooLowOrTooHigh`), so an amount of 1 for a player
  with no entry of that type changes nothing; send 2 or more.
- **SENTIMENT** (latest wins). The ribbit sorcerers leave out a player at or below DISLIKE; ribbit
  merchants' prices use the sentiment (above). No snapshot: Ribbits' own behaviour.
- **PLACE_HOUSE** (C7, P17's recommendation). Placed with vanilla's `StructureTemplate.placeInWorld` and
  the settings a jigsaw piece gets (structure blocks ignored, jigsaw blocks replaced by their final
  state, then the processor list), a few layers a tick under a block quota (512, PLACEHOLDER), on a free
  plot: no other building of the village within 2 blocks, no block a player placed, no block entity,
  only air, plants, leaves or water above the floor layer, ground under most of it. With SEARCH the
  bridge tries rings around (x, z) out to the radius; it waits while candidate chunks are not loaded.
  Answered HOUSE_PLACED + APPLIED, NOT_FREE, or FAILED (unknown village, dimension or template). An op the
  journal or the village's house list has is DUPLICATE; one still being built gets no second answer.
- **SPAWN_RESIDENT** (C7). Made by the factory a plugin registered for the body
  (`mic_rtsim_ribbits` for `ribbits:ribbit`), at the house's slot (AT_HOUSE, waiting while the house is
  being built), beside the village's meeting point (AT_VILLAGE: in the first of a few nearby columns,
  the height nearest the meeting point's, within 6 blocks, where the block below holds it up and the
  body fits out of liquid, else the meeting point itself; a bought golem, SINK.md) or at (x, y, z), once its chunk is
  entity-ticking, under the spawn quota. Residents are
  ordinary entities (C2): the spawn chunk and the resident carry the COMMAND_RESULT (§3.1) so a crash
  cannot spawn it twice.

**Known gaps:** a crash after a house's chunk was saved and before the next world save leaves the
village's house list without the house; a resent PLACE_HOUSE is answered DUPLICATE when its search
reaches the chunk that carries the first one's COMMAND_RESULT (a loaded chunk under a candidate plot),
but a free plot found first elsewhere would still get a second house. HOUSE_PLACED itself is not
carried by the chunk (only the COMMAND_RESULT is): after such a crash the engine hears APPLIED without
the house's box. A house stopped part way (a player's block or a block entity appeared in what was
still to be built, NOT_FREE) leaves the layers it placed. Gossip: a villager carries the last 64 op
ids and one village's high-water mark; a crash that rolls back the village registry's sequence but
not a villager saved later makes that villager skip the next entries up to its old mark, and a
villager that moves between villages and back takes the first village's kept entries again (beyond
its last 64 ops).

## 5. The engine side: rtsim-jni

`crates/rtsim-jni` is a mic_native plugin (a cdylib linking `mic-native-rt`) and the only crate that
uses `jni`; the bridge's `engine/NativeEngine.java` loads it through `MicNative.loadPlugin` and binds
its natives with `System.load`. Choices are in `docs/overnight-choices-jni.md` (J1 to J34).

1. **Buffers** (`buffers.rs`): four engine allocations, 64-aligned, `capacity` in the header, handed
   to Java with `NewDirectByteBuffer`. Rust touches `published` and `consumed` only as `AtomicU64`
   (acquire loads, release stores); every other byte belongs to one side at a time, handed over by
   those words.
2. **`start`** (`engine.rs`): opens the page store in `<world>/rtsim/`, loads `Replay`, the
   `Outbox` and the world, zeroes the four buffers' words and returns `(pagesObsSeq, nextCmdSeq)`,
   `nextCmdSeq = max(the outbox's next, lastAppliedCmdSeq + 1)`.
3. **`submit(h)`** queues step h on mic_native's background pool and returns. The step follows
   §1.2; its release store of `published` is its last act on the buffers.
4. **Journaled observations** go through `Replay::admit` (TRANSFER, TAKEN, DIED and
   COMMAND_RESULT applied or duplicate with their op ids); a gap is logged and skipped
   (`Replay::skip_to`); SESSION_START feeds `Replay::reconcile` and makes every agent abstract;
   WORLD_SAVED calls `forget_ops_through`. Repeats are no-ops by op id, then by state.
5. **Saves**: the pages are snapshotted at a step boundary (before the first journaled record past
   the requested `through`) and written on the IO lane; `ackedObsSeq` names only pages on disk.
6. **Commands**: transfers go through the `Outbox`, leave it when their COMMAND_RESULT is admitted,
   and are resent after a restart with their original sequences and op ids, before anything new;
   puppet commands are per session. DICT records come before their first use in the same buffer.
   What does not fit waits (`MORE`).
7. **Op ids**: Java's have the top bit set; the engine's leave it clear. Ordered transfers take 55
   random bits (§2.2); the sim's own commands derive theirs from their identity (industry, phase 1).
8. **Failure**: a panic or protocol error in a step makes every later command buffer empty and
   flagged `FAILED` (§1.1).
9. **Industry** (`engine/industry.rs`, docs/industry-boundary.md §7, choices W1 to W23): the industry
   engine runs inside the step. Its records (0x0040 to 0x005F, 0x0400 to 0x04FF, 0x8400 to 0x84FF)
   and dictionaries (0x10 to 0x12) are its own; its kept commands share the transfers' resend rules
   (their sequences from the same counter, resent merged in sequence order); its pages are tables
   0x4900 to 0x4903 and page 1:0x49.

10. **Phase 1** (`engine/phase1.rs`, choices V1 to V28 in docs/overnight-choices-phase1jni.md): the
   phase 1 world (`rtsim-world`: settle, trade, travel, knowledge) runs inside the step.
   - **Observations.** VILLAGE_ADOPTED joins a village (a no-op for one it has); TRADE (a resident's
     at its village's market, a puppet's at its caravan) and CARGO_CHANGED are booked as the counts
     the bridge reports, once by op id; HURT and RESCUE become reports of the caravan or the village
     (a resident's first-hand hurt is still a report: rtsim-knowledge never writes back what vanilla
     saw itself); TAKEN and DIED of a phase 1 puppet take or kill its member; BASE marks and unmarks
     player-base nodes; HOUSE_PLACED lets a grown house's residents move in; the first
     VILLAGE_CENSUS sets a village's population and later ones only raise it; VILLAGE_GOSSIP is read
     as latest state. A member TAKEN or DIED books exactly the items of the MEMBER_STATE right before
     it (what its chest or pack really held, the items the sim trades), out of the member's own
     cargo, then the caravan's purse, then the other members'; what it was planned to carry and did
     not hold stays with the caravan (Review P3, Ben: "Book what was really there"; choice PR3).
   - **Commands.** MARKET (`asOfObsSeq` the highest observation the step applied; its ladders from
     rtsim-trade's `market::ladder`) for villages near a player or just traded at, when they changed;
     MERCHANT_OFFERS when a caravan materializes, and again when it walks into or out of a border
     market's reach (P21);
     SENTIMENT for ribbit villages whose feelings may have changed; WRITE_GOSSIP from
     rtsim-knowledge's vanilla gossip (amounts below 2 raised to 2); PLACE_HOUSE then SPAWN_RESIDENT
     for growth. The last three carry op ids derived from what they are (§2.2) and are kept and
     resent like transfers. Caravans and touring bands a player can see are materialized as C25
     groups (SPAWN_GROUP with `FLAG_SURFACE`, cargo in the chests), steered toward where the sim
     walks them, and captured back when no player is near or the sim's caravan moved on.
   - **Pages** (choices S1 to S7 in docs/overnight-choices-persist.md, PF12 to PF21 in
     docs/choices-paging-followups.md). Table 0x5000: the link's meta page and the inputs since the
     snapshot in chunks of 1,024. A snapshot is the world's paged save (its meta page at 0x5001 id
     0, its row tables, the markets' among them, at 0x5010 and up: the pages changed since the last
     snapshot) and the link's own part (the village index and the censused villages) in 1 MiB
     pieces at 0x5001 id 1 and up. `start` opens the world on its pages (rows read as they are
     needed, under `Config.row_budget`) and replays only the tail, then in TEST mode checks the
     state hash against the one saved. A save writes the meta page and the tail chunks not yet on
     disk, or a snapshot when the tail reaches 1,024 inputs or a world day, the world changed
     outside the log (the dragons), or the world's rows are over their budget with every resident
     page changed. Pages this build cannot read are dropped with a loud log, deleted at start, and
     the phase 1 world starts afresh (`when_pages_unreadable`, S5; Ben chose "Drop and start
     fresh").
   - **The switch** (C25; U10, Ben: "Ignore everything"; choice PR13). Phase 1 runs only with all
     four of its modules on (knowledge, settlements, trade, travel). Switched off it captures every
     puppet back (one last write), drops what was queued for the session, ignores its records
     (journaled ones are admitted by `Replay` for their sequence and op id, then counted and
     dropped; censuses too), runs nothing and materializes nothing. Commands the bridge answers by
     op id decided before the switch and not yet written wait unsequenced until it is on again;
     resends of ones already written still go out with their sequences; COMMAND_RESULT and
     HOUSE_PLACED are still taken (the protocol finishing, not the module running). On again, the world runs to
     now at once. The dragons, ships, herds and conflict live in its world and run its clock while
     they run.
   - **News** travels separately from cargo (Ben: "News travels separately"; choice PR6): every road
     has messengers on their own schedule, so a news packet never waits for a caravan's dock; touring
     bands carry gossip on their circuits (P16). Nothing of this crosses the boundary.
   - **Tests.** Orders 16 to 21 script the phase 1 commands (the stub's `market`, `merchantOffers`
     and so on); order 22 marks a village as a test's own, which the engine then leaves alone; orders
     23 (`P1_SPAWN_NEAR`) and 24 (`P1_ADVANCE`) materialize a caravan off the flows at a chosen place
     and run the world ahead of game time (TEST mode only); orders 30 (`WORKS_GRANT`: u64 village,
     i64 thousandths of its own coin, ledgered as minted) and 31 (`WORKS_RAID`: u64 village, u32
     tribute) fill a village's works purse and stand in for a dragon's raid (SINK.md; TEST mode, phase
     1 switched on), and `nPhase1Works` reads a village's works back (bought, kinds full, purse,
     census golems, tribute paid, spent).

11. **Dragons** (`engine/dragons.rs`, §6, choices S8 to S14 in docs/overnight-choices-persist.md):
   rtsim-ecology's engine runs inside the step, in the phase 1 world (its knowledge holds the
   dragons' memories and grudges, its leg index their flights) on the phase 1 world's clock.
   - **Observations.** The journaled kinds 0x0500 to 0x05FF go through `Replay` (HOARD_ROBBED by its
     op id too) into `Dragons::ingest`; DRAGON_STATE is latest state. Players' positions reach the
     dragons through the shared interest areas (item 13). Java's dictionary names species (kind
     3), lairs (kind 4), dimensions and hoard items. The dragons run before the phase 1 world each
     step, interleaved with the ships on one clock (item 13), and a phase 1 input or a record runs
     them to its time first, so a dragon event never meets a world that ran past it.
   - **Commands.** DRAGON_MATERIALIZE, STEER, DEPART, ADOPT and DISCARD take sequences from the
     core's counter, after the core's, the industry's and phase 1's; all are per session (a lost
     adoption or discard is decided again when Java reports the body again). The species and the
     dimension go through the engine's dictionary.
   - **Tribute** (SINK.md §4.6, §6.4 above). LAIR_CHEST is latest state: the dragons keep the
     chest for the session and queue the lair's waiting coin as stacks. Each step writes those
     stacks first in the dragons' share of the command buffer (a stack that does not fit waits for
     the next step), through the core's outbox with `push_with_op` and the core's
     TRANSFER_TO_CONTAINER writer, the dimension and item names through the engine's dictionary. A
     stack whose dimension or item the engine cannot name is failed at once. The COMMAND_RESULT is
     journaled: APPLIED and DUPLICATE_OP place the stack, FAILED sends the coin back to wait; a
     replayed answer for an op the replay already admitted (its DuplicateOp verdict) still places
     the stack. While the module is off its COMMAND_RESULTs are not looked at: the stack is still in
     the outbox, goes again, and Java answers DUPLICATE_OP.
   - **Pages** (PF12 to PF21 in docs/choices-paging-followups.md). Table 0x4400: the meta page
     at id 0, and in TEST mode the state hash at id 1 (checked on load); the rows, the wheel and
     the indexes at 0x4410 and up, read as they are needed under `Config.row_budget`. A save writes
     the meta page and the pages changed since the last one. While there is a dragon or a lair the
     phase 1 link snapshots at every save, in the same commit. Dragon pages without the phase 1
     world they were saved with, or ones this build cannot read, are dropped loudly and deleted
     at start, and the dragons start afresh.
   - **The switch** (C25, `Module.ECOLOGY`, bit 4; choice U10 in
     docs/overnight-choices-integrate2.md): off, the dragons ignore their records, run and write
     nothing and keep the page they had; on again, their bodies of before are gone and they run on.
   - **Tests.** Order 25 (`DRAGON_COMMAND`, TEST mode only) queues a dragon command record with
     Java's species and dimension ids as the dragons' own: the Ice and Fire plugin's GameTests drive
     MATERIALIZE, STEER, DEPART, ADOPT and DISCARD with it on the native engine (choice Z2).
     Order 32 (`TRIBUTE_START`: u64 village, u64 lair, TEST mode only) makes a phase 1 village the
     lair's tributary as if the lair's dragon had raided it: it pays a season's tribute at once,
     at the later of game time's world tick and the dragons' and the world's clocks (ignored while
     the dragons are off). `nDragonsVillageRisk` reads a village's fear to the dragons, its golems'
     included (-1 for a village the world does not know), and `nDragonsTribute` the world-wide
     tribute: seasons paid, coins placed in chests, coins waiting for a chest, stacks sent and not
     yet answered. The three are on `NativeEngine` only, as `worksGrant` is.
   - **Not wired yet:** the dragons' raids reach the industry engine only while the industry module
     runs.

12. **Ships** (`engine/ships.rs`, §7, choices U1 to U18 in docs/overnight-choices-integrate2.md):
   rtsim-ships' engine runs inside the step, in the phase 1 world (lanes are its edges, voyages
   its legs, captains its notables) on the phase 1 world's clock, beside the dragons.
   - **Observations.** The journaled kinds 0x0600 to 0x06FF go through `Replay` (CARGO_LOOTED,
     SHIP_SUNK and SHIP_CAPTURED by their op ids too) into `Ships::ingest`, at their own time with
     the dragons and the world run to it first; those three book against the agent whatever the
     epoch (a replay can name a body the pages never saw, U14). SHIP_STATE is latest state. Item
     ids are Java's and map to the phase 1 catalogue's goods by name; dimensions are Java's ids
     (the ships' frames). PORT_FOUND names an adopted village (a port of a village the world
     lacks is ignored); HAVEN_FOUND joins its haven to the world as a haven node (U13). The
     network is built at the first simulate point where two docks share a medium and a dimension;
     later ports get shuttles (U12). The water mask is the world model's (item 16, §10:
     `rtsim_ships::water::HabitatWater`, known open water only, X2-17); in a dimension the world
     model knows nothing of, every cell is water, as U11 had it (X2-6).
   - **Commands.** SHIP_MATERIALIZE, TIMETABLE, STEER, DEPART, CARGO and DESPAWN take sequences
     from the core's counter after every other module's; all are per session (U4), so none is
     kept or resent. Body kinds, the dimension and cargo items go through the engine's dictionary.
     A merchant embodied across a docking gets its next voyage's hold (SHIP_CARGO, U18), its op id
     from the ship, epoch and voyage (allocator 0x53).
   - **Pages.** Table 0x5300 as the dragons' 0x4400 (the meta page at id 0, the TEST hash at id 1,
     the rows and indexes at 0x5310 and up). While there are ships, docks or havens the phase 1
     link snapshots at every save, in the same commit. Ship pages without the phase 1 world they
     were saved with, or ones this build cannot read, are dropped loudly and deleted at start, and
     the ships start afresh.
   - **The switch** (C25, `Module.SHIPS`, bit 6; U10): as the dragons'.
   - **Tests.** Order 26 (`SHIP_COMMAND`, TEST mode only) queues a ship command record with Java's
     body, item and dimension ids as the ships' own (U15), for a future plugin's GameTests. The
     Rust fake Java side (`tests/common`, `Java::ship_obs`, `ship_command`) plays the plugin in the
     protocol test.

13. **What the modules in the phase 1 world share** (`engine/shared.rs`, U2 to U8): **one interest
   area per player** and **one clock**. PLAYER records go in once per hand-over (the latest per
   player); at the simulate point each player's area is kept or booked anew in the phase 1 world's
   leg index (`World::observe_area`: kept while the player stays within a quarter of the radius
   and it covers the next window) and routed to the modules that run (`World::route_area`, the
   `rtsim_world::interest::Viewer` trait): each books what the area needs, the legs sharing its
   cells are looked up once, and each crossing goes to the module that owns the traveller (leg
   owner bits: 63 players, 62 dragons, 61 ships, 60 herds, 59 conflict parties). A player not
   reported for 200 game ticks leaves; a session start clears every area; with no such module
   running no area is booked. The dragons', the ships', the herds' and the conflict's events run
   interleaved in time (`shared::run_to`); a module that starts running in the middle of a session
   (the herds' first herd, the conflict's first camp) hears of the areas already booked
   (`shared::introduce_herds`, Y1; `shared::introduce_conflict`, C2-5).

14. **Herds** (`engine/herds.rs`, §8, choices Y1 to Y7 and Y22 in
   docs/overnight-choices-herdsmc.md): rtsim-ecology's herds run inside the step, in the phase 1
   world (their legs and the players' areas are its leg index) on its clock, beside the dragons and
   the ships.
   - **Observations.** The journaled kinds 0x0700 to 0x07FF go through `Replay` (HERD_KILLED and
     HERD_TAKEN by their op ids too) into `Herds::ingest`, at their own time with the other modules
     run to it first. HERD_GROUP_STATE is latest state, counted and not yet read. Dimensions are
     Java's ids (the herds' frames).
   - **Commands.** HERD_MATERIALIZE, STEER and DESPAWN take sequences from the core's counter after
     every other module's; all are per session, none kept or resent, none with an op id. The species
     and the dimension go through the engine's dictionary.
   - **Pages.** Table 0x4800 as the dragons' 0x4400 (the meta page at id 0, the TEST hash at id 1,
     the rows, the wheel and the claims at 0x4810 and up). While there are herds the phase 1 link
     snapshots at every save, in the same commit. Herd pages without the phase 1 world they were
     saved with, or ones this build cannot read, are dropped loudly and deleted at start.
   - **The switch** (C25, `Module.ECOLOGY`, bit 4, the dragons' bit; U10's rule).
   - **Prey.** With the dragons running too, the dragons run with the herds as prey
     (`Dragons::run_until_with_herds`, Y7).
   - **Climate and founding** (item 16, §10). The climate is the world model's
     (`rtsim_ecology::herds::habitat::HabitatClimate`: forage, temperature and terrain from the
     habitat cells; water and unknown ground closed), and the world model founds one herd per
     suitable region on its first sample (X2-11). In a dimension the world model knows nothing of,
     the climate is still Y3's uniform grassland (X2-6), and order 28 (`HERD_FOUND`, TEST mode)
     founds herds there as before (Y4).
   - **Tests.** Order 27 (`HERD_COMMAND`, TEST mode) queues a herd command record with Java's
     species and dimension ids as the herds' own (Y5); `nHerdInfo` reads one herd (Y22). The Rust
     fake Java side (`tests/common`, `Java::herd_obs`, `herd_command`) plays the adapter with
     rtsim-ecology's `FakeHerds` in the protocol test.

15. **Conflict** (`engine/conflict.rs`, §9, choices C2-1 to C2-17 in
   docs/overnight-choices-conflictjni.md): rtsim-conflict's factions, feuds and war bands run
   inside the step, in the phase 1 world (havens are its nodes, marches legs in its leg index,
   chiefs notables in its knowledge) on its clock, beside the dragons, the ships and the herds.
   - **Observations.** The journaled kinds 0x0800 to 0x08FF go through `Replay` (CHIEF_KILLED,
     TRIBUTE_PAID, GIFT_GIVEN and BAND_LOOTED by their op ids too) into `Conflict::ingest`, at
     their own time with the other modules run to it first; Java's items, bodies, structures and
     pollution types are translated to the conflict's own first (C2-4). BAND_STATE is latest
     state, counted and not yet read. Dimensions are Java's ids (the conflict's frames).
   - **Signals inside the engine.** Each industry district's learned pollution (deposit goods
     named for a Destroy pollution type, `d/0/destroy:smog/<hash>`, I14) is the settlers' source
     while the district runs, loaded or replaying (C2-7); what an unloaded district drains of any
     other deposit is the settlers' mining (C2-8). Raids on districts go through the industry
     engine's threat interface with the band's own loads (`Threat::loads`, C2-1); raids on villages
     and havens move stock in the phase 1 world (`World::stock_to_held`, C2-10).
   - **Commands.** TERRITORY, FACTION_NOTICE, ENVOY_MATERIALIZE, BAND_MATERIALIZE, BAND_STEER,
     BAND_RECALL and DEMAND_SETTLED take sequences from the core's counter after every other
     module's; all are per session (K2-27), none kept, resent or with an op id. TERRITORY goes
     again for every faction at a session start. Body kinds, the dimension and the tribute item go
     through the engine's dictionary; deadlines and arrivals are game time.
   - **Pages.** Table 0x4300 as the dragons' 0x4400 (the meta page at id 0, the TEST hash at id 1,
     the rows, the wheel and the indexes at 0x4310 and up). While the conflict is active the phase
     1 link snapshots at every save, in the same commit. Pages without the phase 1 world they were
     saved with, or ones this build cannot read, are dropped loudly and deleted at start.
   - **The switch** (C25, `Module.CONFLICT`, bit 5; U10's rule).
   - **Tests.** Order 29 (`CONFLICT_COMMAND`, TEST mode) queues a conflict command record with
     Java's ids as the conflict's own (C2-15). The Rust fake Java side (`tests/common`,
     `Java::conflict_obs`, `conflict_command`) plays the plugin with rtsim-conflict's
     `FakeConflict` in the protocol test.

16. **The world model** (`engine/worldmodel.rs`, §10, choices X2-1 to X2-25 in
   docs/overnight-choices-worldmodel.md): rtsim-space's `SampledWorld`, fed by HABITAT, behind one
   `WorldHandle` the herds' climate and the ships' water mask read.
   - **Observations.** HABITAT (latest wins) is ingested at the observe point: names through Java's
     dictionary (biomes kind 0x30, bodies kind 3), latest wins per cell, chunk-generation counts only
     rise. A record that does not decode or names what Java never named is counted and skipped.
   - **Founding.** At the simulate point, before the herds run, regions with new cells are judged
     (at most 64 a step): one herd per suitable region on its first sample (X2-11), only while
     `Module.ECOLOGY` is on; a decided region is marked in its page and never judged again.
   - **Commands.** HABITAT_KNOWN at every session start, one per region the world model holds, then
     END; per session, sequences from the core's counter after every other module's, the dimension
     through the engine's dictionary.
   - **Pages.** Table 0x5700: the meta page (name tables, state hash, checked on load); table
     0x5701: one page per region. A save writes the meta page and the regions changed since the last
     save (every region after a failed save). Pages this build cannot read are deleted at start and
     the world model starts empty; Java resends every cell as its chunks load.
   - **Tests.** `nHabitatInfo` reads a cell, the counts and the region's founded herd; the Rust fake
     Java side (`tests/common`, `Java::habitat`, `habitat_command`) plays the sampler in the
     protocol test.

## 6. Phase 2: dragons (PLAN.md §9; the Ice and Fire plugin)

Written 2026-10-20 overnight with the engine side, `crates/rtsim-ecology` (`boundary.rs` is the
reference codec, with round-trip tests); the plugin, `mic_rtsim_iceandfire` (`neoforge/iceandfire`,
choices Z1 to Z21 in `docs/overnight-choices-iceandfire.md`), was written the same night. The
native engine routes these records since 2026-10-20 (§5 item 11); the Rust fake Java side in
`crates/rtsim-jni/tests/common` (`Java::dragon_obs`, `dragon_command`) plays the plugin for the
protocol test and is the plugin's executable reference: what it answers to each command is what
the plugin must answer. This
section and `boundary.rs` change together. Choices are numbered R# in
`docs/overnight-choices-dragons.md`. Ice and Fire facts below were checked in the CE 2.1-beta.1 jar
and the pack's `config/iceandfire/iaf-common.json` (2026-10-20); anything not checked says
UNVERIFIED.

### 6.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x0060 to 0x006F | Java to engine | dragon latest-wins observations |
| 0x0500 to 0x05FF | Java to engine | dragon journaled observations (the 24-byte prefix of §2.3) |
| 0x8500 to 0x85FF | engine to Java | dragon commands (`cmdSeq` at 8) |

The bridge and the industry ranges (`docs/industry-boundary.md` §1) stay as they are.

### 6.2 Ids and dictionaries

- **Agents.** A dragon's agent id is `0x44 << 56 | handle`, the engine row's 48-bit handle under
  namespace byte 0x44 (R13), so it never meets another module's agent ids if those keep their top
  byte clear or take their own. Epochs as §2.2: new for every materialization or adoption.
- **Lairs** are their structure start, packed as villages are (§4: `dim << 48 | startChunkX << 24 |
  startChunkZ`). Structures travel in dictionary kind 4: `iceandfire:fire_dragon_roost`,
  `fire_dragon_cave`, `ice_dragon_roost`, `ice_dragon_cave`, `lightning_dragon_roost`,
  `lightning_dragon_cave` (verified structure ids).
- **Species** travel as body kinds (dictionary kind 3): `iceandfire:fire_dragon`, `ice_dragon`,
  `lightning_dragon` (verified). So do prey animals in DRAGON_ATE (`minecraft:cow`, ...).
- **Hoard items** are items (dictionary kind 2).
- **Players** are UUIDs (most, least), as everywhere.

### 6.3 Observations

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0060 | DRAGON_STATE (latest-wins) | flags: 1 sleeping, 2 flying, 4 has an attack target, 8 ticking | 8 u64 agent, 16 u32 epoch, 20 u32 dimension, 24 f32 health, 28 f32 maxHealth, 32 i32 hunger, 36 i32 ageTicks, 40 f64 x, 48 f64 y, 56 f64 z. Every 100 ticks per embodied dragon (PLACEHOLDER) | 64 |
| 0x0061 | LAIR_CHEST (latest-wins) | 0 | 8 u64 lair, 16 u32 dimension, 20 i32 x, 24 i32 y, 28 i32 z: a chest in the lair's box the tribute can go into | 32 |
| 0x0500 | DRAGON_MATERIALIZED | 0 | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u64 entityMost, 48 u64 entityLeast | 56 |
| 0x0501 | DRAGON_SPAWN_REJECTED | reason (§2.3's codes, plus 9 unknown species) | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 cmdSeq | 48 |
| 0x0502 | DRAGON_DEMATERIALIZED | reason (§2.3's codes) | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 f32 health, 44 f32 maxHealth, 48 i32 hunger, 52 i32 ageTicks, 56 f64 x, 64 f64 y, 72 f64 z | 80 |
| 0x0503 | DRAGON_HURT | attacker kind: 0 none, 1 player, 2 mob, 3 other | 24 u64 agent, 32 u32 epoch, 36 u32 hits, 40 f32 amount, 44 f32 healthAfter, 48 u64 attackerMost, 56 u64 attackerLeast | 64 |
| 0x0504 | DRAGON_DIED | killer kind (as HURT) | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u64 killerMost, 48 u64 killerLeast, 56 f64 x, 64 f64 y, 72 f64 z | 80 |
| 0x0505 | LAIR_FOUND | flags: 1 LATE (in a chunk generated before the plugin) | 24 u64 lair, 32 u32 dimension, 36 u32 structure (dictionary 4), 40 i32 minX, 44 minY, 48 minZ, 52 maxX, 56 maxY, 60 maxZ, 64 i32 homeX, 68 homeY, 72 homeZ, 76 reserved | 80 |
| 0x0506 | HOARD_ROBBED | n | 24 u64 lair, 32 u64 opId, 40 u64 playerMost, 48 u64 playerLeast, 56 n × (u32 item, i32 count, u64 reserved) | 56 + 16n |
| 0x0507 | DRAGON_UNTAGGED | spawn: 1 structure, 2 natural, 3 spawner, 4 spawn egg, 5 command, 6 hatched, 7 other | 24 u64 entityMost, 32 u64 entityLeast, 40 u32 species (dictionary 3), 44 u32 dimension, 48 u32 flags (1 tamed, 2 has home, 4 sleeping, 8 aging disabled), 52 i32 ageTicks, 56 i32 hunger, 60 u32 gender (0 female, 1 male), 64 u32 variant (0 to 3 within the species), 68 i32 homeX, 72 homeY, 76 homeZ, 80 u64 lair (0: none), 88 f64 x, 96 f64 y, 104 f64 z | 112 |
| 0x0508 | DRAGON_ATE | count | 24 u64 agent, 32 u32 epoch, 36 u32 prey (dictionary 3), 40 u64 village (0: none), 48 f64 x, 56 f64 y, 64 f64 z | 72 |
| 0x0509 | OIL_CHUNKS | n | 24 u32 dimension, 28 u32 source (1 Deep Time, 2 CDG, 3 a deposit sample), 32 n × (i32 chunkX, i32 chunkZ, i32 amount mB, u32 flags: 1 bottomless). n at most 1024 per record | 32 + 16n |
| 0x050A | DRAGON_BREATHS | count | 24 u64 agent, 32 u32 epoch, 36 reserved | 40 |
| 0x050B | OIL_CONFIG | flags: 1 field flights, 2 place lairs, 4 place nests | (none) | 24 |
| 0x050C | NEST_FOUND | 1 placed, 2 failed (no room) | 24 u64 nest, 32 u32 dimension, 36 i32 heartX, 40 heartY, 44 heartZ (zero when failed) | 48 |
| 0x050D | NEST_CENSUS | flags: 1 the heart is gone | 24 u64 nest, 32 u32 worms (alive, an unloaded one counting as alive, plus those still owed), 36 reserved | 40 |
| 0x050E | NEST_LOSS | cause (`LossCause`: 0 player, 1 age, 2 starved, 3 faction, 4 predator, 5 other) | 24 u64 nest, 32 u32 count, 36 reserved, 40 u64 playerMost, 48 u64 playerLeast (zero for none) | 56 |
| 0x050F | NEST_HEART | 0 | 24 u64 nest, 32 u64 playerMost, 40 u64 playerLeast (zero for none) | 48 |

- **DRAGON_HURT** sums one attacker's hits on one dragon over a tick (PLACEHOLDER: over 20 ticks
  would also do). The engine turns a player's wound into an assault report the dragon and its lair
  witness (a grudge), and brings the dragon's departure forward (C24).
- **DRAGON_DIED** is journaled at the end of the tick, as DIED (§2.3). A player's kill is a killing
  report the dragon's kin and lair witness; the settlements it preyed on get a rescue report (C24:
  rescues count); its strongest memories and feelings pass to its heir.
- **DRAGON_ATE**: a sim dragon killed animals (Ice and Fire's own hunting picks them from its
  `dragon_targets` tag, verified). When the kill is inside a village's bounds, those animals were the
  village's herd and its market stock falls by the count (P28: watched consequences go through
  stock, prices and caravans; Ben added bounded damage too, see §6.7). Summed per (dragon, prey
  type, village) per tick.
- **HOARD_ROBBED**: a player broke hoard piles (`iceandfire:gold_pile` for fire, `silver_pile` for
  ice, `copper_pile` for lightning, verified) or took items from a container, inside a known lair's
  box. Summed per (player, lair) per tick. The engine takes what its abstract hoard holds of each
  item (the rest was the structure's own treasure, never in its ledger) and the lair and its
  dragons witness a robbery report. Hoard piles are blocks: a broken pile counts what its loot table
  drops for that player (verified: 2 nuggets a layer, layers 1 to 8; the plugin's hoard blocks are the
  tag `#mic_rtsim_iceandfire:hoard`, Z16).
- **DRAGON_UNTAGGED**: an Ice and Fire dragon joined the level without the plugin's tag (§6.5).
- **OIL_CHUNKS** (OILGATE.md §4.1, OG-2): chunks with base oil above zero. Idempotent (a chunk sent
  twice is one chunk); a chunk with amount 0 is never sent. The engine keeps only that a chunk has
  oil and whether it is bottomless; the amount is for logs. The first OIL_CHUNKS a session's engine
  ever holds opens the oil gate (§6.8).
- **DRAGON_BREATHS** (OG-4): the `DragonFireEvent`s of one embodied MAT_OIL dragon since its last
  report, summed (PLACEHOLDER: per 20 ticks, and before its DEMATERIALIZED). Each costs one point of
  the reserve.
- **OIL_CONFIG** (OG-3, OG-7, OG-9e): the plugin's three switches, sent once per session before the
  first PLAYER. Without it the engine keeps its defaults (field flights on, lair placement on, nests on;
  OGB-6, OGC70).
- **NEST_FOUND** (OG-9e, §6.8): answers PLACE_NEST, placed (with the heart's block) or failed (the
  chunk has no natural ground for the chamber). Sent again, with the same heart, for a PLACE_NEST of a
  nest already placed. Not ledgered: the engine keeps the nest, the plugin keeps the chamber.
- **NEST_CENSUS**: a nest counted when its heart's chunk loads and after its worms spawn. The engine
  reconciles without pressure: a count above K is K, a shortfall is a loss of cause other (OGC78).
  Flag 1 (the heart's block is not the spawner) is a heart broken with no player named.
- **NEST_LOSS**: worms of one nest died. A worm drops boss loot, so a death no wild mob is credited
  with is a player's (EQUILIBRIUM.md §2.2); only cause 0 bumps the nest's pressure.
- **NEST_HEART**: a player (or something else, zero) broke the heart's spawner.
- **LAIR_CHEST** (SINK.md §4.6, the money sink's tribute): every 100 ticks (PLACEHOLDER,
  `Lairs.CHEST_SCAN_TICKS`) the plugin looks over the loaded chunks of each lair it reported this
  session for a `ChestBlockEntity` inside the box with an empty slot (a chest whose loot table is not
  rolled yet counts as having room: reading its slots would roll it), the nearest the lair's home
  (ties by block order), and reports it. One report per lair per session, and again when that chest is
  gone, full, or its chunk was unloaded since (a reloaded chunk is a new chunk object; an unloaded
  chunk is what fails a transfer, and the engine forgets a chest whose transfer failed), and after
  `Module.ECOLOGY` is switched back on. A lair with no chest sends nothing. Written after the lair's
  LAIR_FOUND, which is earlier in the buffer, so the engine always knows the lair. The engine keeps
  the chest for the session only (never saved): a new session hears of it again.

### 6.4 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8500 | DRAGON_MATERIALIZE | flags: 1 clamp above the surface, 2 has a home, 4 sleeping, 8 hostile, 16 oil (the body carries a reserve) | 16 u64 agent, 24 u32 epoch, 28 u32 species (dictionary 3), 32 u32 dimension, 36 i32 ageTicks, 40 f64 x, 48 f64 y, 56 f64 z, 64 i32 hunger, 68 u32 gender, 72 u32 variant, 76 f32 health (0 full), 80 i32 homeX, 84 homeY, 88 homeZ, 92 i32 oil (0 to the capacity; 0 without flag 16), 96 f64 targetX, 104 targetY, 112 targetZ, 120 u64 targetPlayerMost, 128 u64 targetPlayerLeast | 136 | DRAGON_MATERIALIZED or DRAGON_SPAWN_REJECTED |
| 0x8501 | DRAGON_STEER | flags: 1 settle at the target | 16 u64 agent, 24 u32 epoch, 28 reserved, 32 f64 x, 40 f64 y, 48 f64 z | 56 | |
| 0x8502 | DRAGON_DEPART | reason: 1 bored, 2 hungry, 3 wounded, 4 passing through, 5 recalled | 16 u64 agent, 24 u32 epoch, 28 u32 timeoutTicks, 32 f64 x, 40 f64 y, 48 f64 z | 56 | DRAGON_DEMATERIALIZED |
| 0x8503 | DRAGON_ADOPT | 0 | 16 u64 entityMost, 24 u64 entityLeast, 32 u64 agent, 40 u32 epoch, 44 u32 flags (1 has a home), 48 i32 homeX, 52 homeY, 56 homeZ, 60 reserved | 64 | DRAGON_MATERIALIZED or DRAGON_SPAWN_REJECTED |
| 0x8504 | DRAGON_DISCARD | reason: 1 the sim owns the population, 2 its lair has its dragon | 16 u64 entityMost, 24 u64 entityLeast | 32 | COMMAND_RESULT |
| 0x8505 | PLACE_LAIR | 0 | 16 u64 field (the engine's id, for logs), 24 u32 dimension, 28 u32 structure (dictionary 4), 32 i32 x, 36 i32 z, 40 u32 radius, 44 reserved | 48 | LAIR_FOUND (flag 1 LATE) when placed; nothing when no site is found |
| 0x8506 | OIL_SURVEY | 0 | 16 u32 dimension, 20 i32 x, 24 i32 z, 28 u32 radius | 32 | OIL_CHUNKS (source 2), as many as it takes; nothing when it finds none |
| 0x8507 | PLACE_NEST | 0 | 16 u64 nest, 24 u32 dimension, 28 i32 chunkX, 32 i32 chunkZ, 36 u32 depth (the chamber's floor this many blocks below the surface) | 40 | NEST_FOUND, at once if the chunk is loaded, else on its next load |
| 0x8508 | NEST_RENEW | count | 16 u64 nest, 24 u32 dimension, 28 u32 stage (1 to 3, the worms' size), 32 reserved | 40 | NEST_CENSUS once they spawn |
| 0x8509 | NEST_SORTIE | count | 16 u64 nest, 24 u32 dimension, 28 i32 x, 32 i32 z (a block of the raided district), 36 u32 timeoutTicks | 40 | |

**Tribute** (SINK.md §4.6) needs no command of its own: the coin a village pays a lair goes into the
lair's reported chest as the core's TRANSFER_TO_CONTAINER (§2.4, 0x8005), one stack of at most 64
per command, the village's currency item (emerald for humans), the chest's dimension and block.
These are engine ops in the core's outbox, so they are resent after a restart until answered, and
each stack's op id is drawn from the lair's key and a per-lair stack counter (a resent stack is the
same op). The answer is COMMAND_RESULT: APPLIED or DUPLICATE_OP count the stack in the chest;
FAILED (the chunk unloaded, the chest gone or full) puts the coin back to wait and makes the engine
forget the chest until LAIR_CHEST names one again, and the stack then goes under a new op id.
Nothing goes out while the lair has no chest: the coin waits in the lair's books.

### 6.5 How Java applies them

**Materialize.** Spawn the species' entity at (x, y, z); with flag 1, raise y to the motion-blocking
heightmap plus a margin (PLACEHOLDER 8) where the terrain is higher (the engine flies dragons at a
forced cruise altitude while unloaded, as Veloren's airships, so one never appears underground).
Write its NBT before it ticks (verified keys and types, `DragonBaseEntity.addAdditionalSaveData`):
- `AgeTicks` (int) = ageTicks, and `AgingDisabled` (bool) = true: the engine owns age (structure
  lair dragons spawn with aging disabled anyway, verified).
- `Hunger` (int, 0 to 100, higher is fuller) = hunger.
- `Gender` (bool, true is male) = gender == 1.
- `Variant` (string): the colour's plain name, `DragonColor.getName()` (`red`, not a namespaced id;
  verified in `addAdditionalSaveData`, `finalizeSpawn` and both lair pieces). The variant index is the
  position in the species' `DragonType.colors()` list, in `IafDragonColors`' construction order: fire
  red, green, bronze, gray; ice blue, white, sapphire, silver; lightning electric, amethyst, copper,
  black (verified), which is also `DRAGON_COLOR.byId(variant + offset)` with offsets 0, 4 and 8.
- With flag 2: `HomeAreaX/Y/Z` (int), `HomeDimension` (string), `HasHomePosition` (bool) = true.
  **Trap** (verified): on read the home is dropped if any coordinate is exactly 0; the plugin nudges a
  0 to 1.
- Health (when not 0) after attributes refresh: `refreshDirtyAttributes` sets max health from age
  (4% of `maxHealth` at day 0 up to all of it at day 125; the pack's `maxHealth` is 500, verified).
- Sleeping (flag 4): `setInSittingPose(true)` (the `Sleeping` NBT).
- Persistence required: the pack's `canDespawn` is true (verified), and a puppet must never be
  saved by vanilla (§6.1 of PLAN.md; P3).
- Tag it with the agent and epoch (the plugin's own persistent data), as the bridge tags puppets.
- First flight target: `dragon.flightManager.setFlightTarget(new Vec3(targetX, targetY,
  targetZ))` (verified: `public final IafDragonFlightManager flightManager`). Caveats (verified):
  Ice and Fire overwrites the target while the dragon has a live attack target, and clamps y to its
  `maxFlight` (256 in the pack).
- With flag 8 (hostile), make the target player its attack target. The pack's `neutralToPlayer` is
  false (verified), so a wild dragon attacks players on its own anyway.

**Steer** sets the flight target as above; flag 1 lets it land there.

**Depart.** Steer the dragon to the target and, when it is outside every player's entity-ticking
range (or `timeoutTicks` pass), capture it and despawn it: DRAGON_DEMATERIALIZED with its health,
hunger, age and position. Any other loss of the body (its chunk unloads, the session ends) is
captured the same way with §2.3's reasons.

**Adopt** tags an untagged entity with the agent and epoch, makes it persistent, and sets its home
when flag 1 is set; from then on it is an embodied puppet like a materialized one, and leaves view
through DRAGON_DEMATERIALIZED. Answered by DRAGON_MATERIALIZED with the entity's UUID. An entity the
engine adopted before and that comes back untagged (the adoption never reached a world save) is
adopted again as the same agent under a new epoch, never as a second dragon.

**Discard** removes the entity (`discard()`), with no drops. Answered by COMMAND_RESULT (status
applied, its `cmdSeq`, op id 0: the record has none, and a discard repeated after a crash is
harmless).

### 6.6 Owning the population (P13's recommendation: the sim owns every dragon)

What the bridge must do:
1. **Turn off In Control's fire dragon.** The pack has an In Control spawner entry for
   `iceandfire:fire_dragon` in `config/incontrol/spawner.json` (`persecond` 0.1, `attempts` 20,
   overworld, 25 to 100 blocks from a player, `maxthis` 200) and two `onjoin` rules in
   `config/incontrol/spawn.json` whose mob list holds `minecraft:blaze` and `iceandfire:fire_dragon`
   (allowed in `c:is_desert`, denied elsewhere when In Control caused the join), all verified in the
   pack config. Remove the spawner entry and take `iceandfire:fire_dragon` out of both rules' mob
   lists (the blaze rules stay). This is a pack change; Ben chose it (DECISIONS.md OGB-6). It is built
   on the pack repo's branch `oilgate-f8` and merges in the same pack commit that adds mic-rtsim to the
   pack's `mods/` (OGC46); until then, the engine discards the fire dragons In Control spawns while
   field flights run (item 3, OGC66). `neoforge/iceandfire/pack-proposal/config/incontrol/` holds the
   same files for OG-10's harness (Z21).
2. **Report every untagged dragon** that joins a level (`EntityJoinLevelEvent`) as DRAGON_UNTAGGED,
   with its spawn kind from `FinalizeSpawnEvent.getSpawnType()` where one fired first (STRUCTURE,
   NATURAL, SPAWNER, SPAWN_EGG, COMMAND, BREEDING mapped to the codes above), its NBT fields, and the
   lair whose box holds it. Ice and Fire's lair pieces do **not** spawn their dragon through
   `finalizeSpawn` (verified: `DragonRoostPiece.spawnDragon` and `DragonCavePiece.createDragon` call
   `EntityType.create` and `WorldGenLevel.addFreshEntity`), so no STRUCTURE spawn type is ever seen for
   them; the lair's box decides: a dragon with no spawn type that joins inside a lair's box is a
   structure spawn (choice Z10). In Control's spawner spawns as NATURAL (verified).
3. **Apply the engine's answer:** the engine adopts a lair's own dragon from world generation as its
   occupant, adopts spawn-egg and command dragons as wanderers, leaves natural and spawner dragons alone
   (R18 as amended by OG-1 and P13: they are In Control's; a fire one is discarded while field flights
   run, since they replace it, OGB-6 and OGC66, and every one when the engine's `adopt_natural_dragons`
   is set) and discards a second structure dragon for a lair that has one,
   and leaves **tamed** dragons alone (A10: a creature a player takes leaves the sim). Dragons
   hatched from eggs are tamed by Ice and Fire (verified, `DragonEggEntity.updateEggCondition`), so
   they are the player's.
4. **No respawn of its own.** Population comes back only through the engine: pairs breed in their
   lair (M1's recommendation, sim-only), young dragons leave to claim empty roosts, and a roost that
   stands empty for the respawn delay draws a young dragon from afar (TUNABLE, PLACEHOLDER 20 days,
   `DragonTunables::respawn_delay_days`), which arrives as a flight a player can watch. Ice and
   Fire's own breeding needs a player to feed mature dragons (verified), so wild dragons in game do
   not breed by themselves.

### 6.7 What the engine side does with them (rtsim-ecology)

- Player positions (PLAYER, §2.3) feed a decaying home counter per player (P26's recommendation)
  and book each player's interest area into the shared leg index (on the native engine, the one
  area per player every module shares, §5 item 13); a dragon's leg that crosses one
  books exactly one meeting, which materializes it at the edge of the area if both legs are still
  as booked when it fires (A15).
- Every dragon is a knowledge notable; its death of age is one booked event (C24), its heir is its
  youngest offspring, else its mate, else its lair, and a dragon claiming an empty lair takes over
  the lair's strongest feelings.
- A raid on a district goes through the industry engine's threat interface (`docs/industry-
  boundary.md`): unwatched districts resolve abstractly, never better for the defender than the
  best comparable observed fight, with capped damage applied on load (C18). A watched district
  resolves the same way, at once, and gets a real fight too: the dragon materializes there (P28 as
  Ben decided it, "bounded damage too"; docs/choices-oilgate.md OGC56, OGC57). Ice and Fire
  griefing stays off, so the bounded damage is the raid's. Districts are found by any of their
  chunks, not their centre (OGC50, OGC51).
- **Tribute** (SINK.md §4.6, MS-5; choices MSC40 and up in docs/choices-money-sink.md). A village
  with a works purse that a lair's dragon raided becomes that lair's tributary and pays at once, then
  once a season at the calendar's season change: a share of its purse (PLACEHOLDER a quarter,
  `tribute_share_milli`) in whole coins of its currency, booked as `tribute` in the village's works
  books and in the dragons' ledger (a column of its own, apart from what dragons took). The coin joins the lair's hoard and waits there, unplaced, until
  the plugin reports a chest (LAIR_CHEST); then it goes in stacks (§6.4). While a tribute is paid
  within the season, the lair's dragons leave that village's livestock alone. It lapses when the
  purse is dry at a payment or the lair is vacant.

### 6.8 The oil gate (OILGATE.md; choices OGC# in `docs/choices-oilgate.md`)

Fire dragons need oil, and the danger In Control's desert rule puts on deserts follows oil fields
instead. The engine side is `crates/rtsim-ecology/src/oil.rs`; the synthetic plugin in `synth.rs`
(`FakeIaf`) answers the oil records as the plugin must.

**What the plugin does:**
- **Reports oil chunks** (OIL_CHUNKS): on a Deep Time level, every petroleum trap's chunks at world
  load (source 1, Deep Time's `DeepTimeSites` and CDG bridge); elsewhere CDG's
  `OilChunksSavedData.getBaseOilAmount` for each chunk as it first generates (source 2), under a
  per-tick budget. CDG's infinite amount (`Integer.MAX_VALUE`) is flag 1.
- **Answers OIL_SURVEY** by sampling CDG's amount for the chunks within `radius` of (x, z) under the
  same budget, and sending what it finds as OIL_CHUNKS (source 2): the chunks in memory at once, each
  of the others the first time it loads this session (a chunk not in memory reads the biome source,
  79 ms on a real overworld; docs/choices-oilgate.md OGC43).
- **Answers PLACE_LAIR** by placing the structure through `/place structure`'s code path (every biome
  allowed) on the first load of a free site within `radius` of (x, z), at most once per `field`, and
  sending its LAIR_FOUND (flag 1). The roost's own dragon, if Ice and Fire spawns one, is reported
  untagged as any lair's dragon is (§6.6), and the engine adopts it.
- **Counts breaths:** a body materialized with flag 16 starts with `oil` points. Each
  `DragonFireEvent` it posts costs one point and is summed into DRAGON_BREATHS; at zero the event is
  cancelled, so the dragon fights with bite and claws only (OG-13). A body without flag 16 breathes
  freely and is not counted.
- **Sends OIL_CONFIG** from its config at session start: `fieldFlights` (default true since OGB-6:
  the flights replace In Control's fire dragon, OGC46) and `placeLairs` (default true).
- **Sends the player's y** in PLAYER (already there, §2.3): field flights roll only for players at or
  above y 15 (In Control's `minheight`).
- **Keeps the oil nests** (OG-9e, OILGATE.md §4.6, OGC84 to OGC102): a PLACE_NEST waits in the saved
  state until its chunk is loaded, then carves a 9 by 9 chamber whose sand floor lies `depth` blocks
  under the chunk's ocean-floor heightmap, only where the chamber and its shell are natural ground with
  no fluid and no block entity (up to 8 seeded spots), sets an inert Death Worm spawner as its heart and
  answers NEST_FOUND (placed or failed). A PLACE_NEST for a nest already placed answers NEST_FOUND
  placed again, carves nothing, and re-forms a broken heart. NEST_RENEW owes worms (Ice and Fire's
  Death Worms, sized by the stage), placed on the chamber floor when the heart's chunk ticks entities
  with no player within 128 blocks; NEST_CENSUS follows, and goes out on each load of the heart's
  chunk. A nest worm breaks no blocks. A worm's death is NEST_LOSS (the boss rule), the heart's breaking
  NEST_HEART. NEST_SORTIE glides the nest's loaded worms through the ground to the raided column and up
  it; a worm stops under a block tagged `#mic_rtsim:armoured_floor` or `#mic_rtsim:burrow_spared` and
  glides home, else it surfaces, fights until `timeoutTicks` and glides home.

**What the engine does with them:**
- **The gate opens** on the first oil chunk it knows (OGC12): before that, no dragon is held to oil
  (a world with no oil known plays as before). When it opens, every fire dragon starts on a full
  reserve.
- **The reserve** falls one point per `tick_rate` ticks and one per breath (embodied) or raid pass
  (abstract); a dragon low on oil refuels by resting on a field or by drinking from a district's oil
  stock (crude, diesel, gasoline goods) through the industry engine's threat interface, as theft.
- **Field flights** (when OIL_CONFIG flag 1 is set): In Control's arrival process keyed to fields,
  drawing fire dragons as flights that materialize with flags 8 and 16 at the edge of view.
- **Lairs:** a field with no fire lair within reach gets one PLACE_LAIR (when flag 2 is set); every
  new fire lair asks one OIL_SURVEY around itself.
- **Sensing:** a district with a chunk near a field that forms, drills, holds oil stock or pollutes
  becomes an Encroachment report its drinking dragons witness, against the district, the players
  homed near it and the players who built there in the last `grudge_window_days` (OGC50, OGC55); a
  grudge sends a raid. Once a game day the field watches its own ground: a fire dragon of the
  nearest fire lair in reach senses every district with a chunk near a field, with no flight
  (OGC52, OGC53). The industry engine keeps each district's last builders from IND_EDIT, saved with
  it (OGC54).
- **Oil nests** (when OIL_CONFIG flag 4 is set, OGC70 to OGC83): an oil chunk new to the index founds
  a nest in its cell of 8 by 8 chunks and sends PLACE_NEST. A nest is asleep until a pumpjack's
  deposit sample of its district's latest sweep lies within reach of the heart; awake, it hatches K
  worms, renews lost ones by EQUILIBRIUM.md §2.2's rule while drilling lasts, and raids the districts
  in reach as threat class `Burrowing` (docs/industry-boundary.md). The nest's id packs the frame
  (top 16 bits) and the cell's x and z (24 bits each, signed).

## 7. Ships (PLAN.md §10.1; a future ships plugin)

Written 2026-10-20 overnight with the engine side, `crates/rtsim-ships` (`boundary.rs` is the
reference codec, with round-trip tests). **No plugin exists**, and none is chosen: which mod gives
the bodies (Small Ships, Create Aeronautics Structures, the Seven Seas add-on) and how ports and
havens are provided are P8 (Ben). So every record names **abstract body kinds**, and the plugin
maps them to a mod's entities (P15's body-kind data). The executable reference for what a plugin
answers is the fake in `crates/rtsim-ships/src/synth.rs` (`FakeShips`), which the tests and
`rtsim-cli sim ships` drive through this codec. The native engine routes these records since
2026-10-20 (§5 item 12, choices U# in `docs/overnight-choices-integrate2.md`); the Rust fake Java
side in `crates/rtsim-jni/tests/common` (`Java::ship_obs`, `ship_command`) plays a plugin for the
protocol test. Choices are numbered T# in `docs/overnight-choices-ships.md`. Nothing below about a
body mod is verified; it is the contract a plugin would have to meet.

### 7.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x0070 to 0x007F | Java to engine | ship latest-wins observations |
| 0x0600 to 0x06FF | Java to engine | ship journaled observations (the 24-byte prefix of §2.3) |
| 0x8600 to 0x86FF | engine to Java | ship commands (`cmdSeq` at 8) |

The bridge's, phase 1's, the industry's (`docs/industry-boundary.md` §1) and the dragons' (§6.1)
ranges stay as they are; these three are next to the dragons'.

### 7.2 Ids and dictionaries

- **Agents.** A ship's agent id is `0x53 << 56 | handle` (namespace byte 0x53, "S"), the engine
  row's 48-bit handle, as dragons use 0x44 (R13). The ship and its crew are one group (C25): one
  agent, one epoch, members listed in the materialize record. Epochs as §2.2: new for every
  materialization.
- **Body kinds** (dictionary kind 3), abstract until P8: `mic_rtsim:ship/sea_merchant`,
  `ship/sea_pirate`, `ship/sea_navy`, `ship/sky_merchant`, `ship/sky_pirate`, `ship/sky_navy`, and
  crews `mic_rtsim:crew/sailor`, `crew/pirate`, `crew/marine`, `crew/aeronaut`, `crew/sky_pirate`
  (`ShipTunables`). D7 rules out player-like crews: the plugin's body data picks mob-like bodies.
- **Items** (dictionary kind 2) for cargo. **Structures** (kind 4) name the structure a port or
  haven was found in, 0 for none.
- **Ports and havens** are keyed by their village's (or haven's) key, packed as §4's villages are.
- **Op ids**: the plugin's (top bit set, §2.2) on CARGO_LOOTED, SHIP_SUNK and SHIP_CAPTURED; the
  engine's SHIP_CARGO uses allocator 0x53 in bits 56 to 62 over 56 bits derived from the ship, its
  epoch and the voyage (U18).

### 7.3 Observations

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0070 | SHIP_STATE (latest-wins) | flags: 1 ticking, 2 a player aboard, 4 in combat | 8 u64 agent, 16 u32 epoch, 20 u32 dimension, 24 u32 hull (thousandths), 28 u32 crew, 32 f64 x, 40 f64 y, 48 f64 z. Every 100 ticks per embodied ship (PLACEHOLDER) | 56 |
| 0x0600 | SHIP_MATERIALIZED | 0 | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u64 entityMost, 48 u64 entityLeast (the ship entity) | 56 |
| 0x0601 | SHIP_SPAWN_REJECTED | reason (§2.3's codes, plus 9 unknown body kind) | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 cmdSeq | 48 |
| 0x0602 | SHIP_DEMATERIALIZED | reason (§2.3's codes) | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u32 hull, 44 u32 crew, 48 f64 x, 56 f64 y, 64 f64 z | 72 |
| 0x0603 | SHIP_DAMAGED | by: 0 none, 1 player, 2 ship, 3 other | 24 u64 agent, 32 u32 epoch, 36 u32 hull after, 40 u32 crew lost, 44 reserved, 48 u64 playerMost, 56 u64 playerLeast, 64 u64 attacker agent (by ship) | 72 |
| 0x0604 | SHIP_SUNK | by (as DAMAGED) | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u64 playerMost, 48 u64 playerLeast, 56 u64 killer agent, 64 u64 opId, 72 f64 x, 80 f64 y, 88 f64 z | 96 |
| 0x0605 | SHIP_BOARDED | by: 1 player, 2 ship | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 playerMost, 48 u64 playerLeast, 56 u64 boarder agent | 64 |
| 0x0606 | SHIP_CAPTURED | by: 1 player, 2 ship | 24 u64 agent, 32 u32 epoch, 36 u32 crew taken (M2's crew conversion; 0 for a player), 40 u64 playerMost, 48 u64 playerLeast, 56 u64 captor agent, 64 u64 opId | 72 |
| 0x0607 | CARGO_LOOTED | n | 24 u64 agent (whose hold), 32 u32 epoch, 36 u32 by (1 player, 2 ship), 40 u64 playerMost, 48 u64 playerLeast, 56 u64 looter agent, 64 u64 opId, 72 n × (u32 item, i32 count, u64 reserved) | 72 + 16n |
| 0x0608 | SHIP_FIGHT | outcome: 1 the attacker won, 2 the defender won, 3 broke off | 24 u64 attacker agent, 32 u64 defender agent, 40 u32 attacker crew before, 44 after, 48 u32 defender crew before, 52 after, 56 u32 attacker hull after, 60 defender hull after | 64 |
| 0x0609 | PORT_FOUND | medium: 1 sea, 2 sky | 24 u64 village, 32 u32 dimension, 36 u32 structure (dictionary 4, 0 none), 40 i32 dockX, 44 dockY, 48 dockZ, 52 u32 berths | 56 |
| 0x060A | HAVEN_FOUND | media: 1 sea pirates, 2 sky pirates (a mask) | 24 u64 haven, 32 u32 dimension, 36 u32 structure, 40 i32 dockX, 44 dockY, 48 dockZ, 52 reserved | 56 |

All of 0x0600 to 0x060A are journaled. CARGO_LOOTED, SHIP_SUNK and SHIP_CAPTURED carry the
plugin's op id: the engine applies each once (§2.3's idempotent events).

### 7.4 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8600 | SHIP_MATERIALIZE | members m (the ship first, then its crew) | 16 u64 agent, 24 u32 epoch, 28 u32 body (dictionary 3), 32 u32 dimension, 36 u32 flags (1 clamp above the surface, 2 hostile, 4 docked), 40 f64 x, 48 f64 y, 56 f64 z, 64 u8 medium (1 sea, 2 sky), 65 u8 role (1 merchant, 2 pirate, 3 navy), 66 u8 phase (0 dock, 1 depart, 2 cruise, 3 approach, 4 hunting, 5 sailing), 67 u8 platform, 68 i32 cruiseY, 72 u64 target agent (a pirate's prey, 0 none), 80 u32 crew, 84 u32 hull, 88 u32 waypoints w, 92 u32 cargo c, 96 i64 departTick, 104 i64 cruiseTick, 112 i64 approachTick, 120 i64 arriveTick, 128 u32 harbourBlocks, 132 reserved, 136 w × (i32 x, i32 z), then c × (u32 item, i32 count, u64 reserved), then m × (u32 body, u32 flags: 1 captain) | 136 + 8w + 16c + 8m | SHIP_MATERIALIZED or SHIP_SPAWN_REJECTED |
| 0x8601 | SHIP_TIMETABLE | flags: 1 dock at the end | 16 u64 agent, 24 u32 epoch, 28 u32 waypoints w, 32 i64 departTick, 40 cruiseTick, 48 approachTick, 56 arriveTick, 64 i32 cruiseY, 68 i32 dockY, 72 u8 platform, 73 reserved, 76 u32 harbourBlocks, 80 w × (i32 x, i32 z) | 80 + 8w | |
| 0x8602 | SHIP_STEER | flags: 1 attack the target | 16 u64 agent, 24 u32 epoch, 28 f32 speed (blocks a tick), 32 f64 x, 40 f64 y, 48 f64 z, 56 u64 target agent | 64 | |
| 0x8603 | SHIP_DEPART | reason: 1 bored, 2 hold full, 3 damaged, 4 passing through, 5 recalled | 16 u64 agent, 24 u32 epoch, 28 u32 timeoutTicks, 32 f64 x, 40 f64 y, 48 f64 z | 56 | SHIP_DEMATERIALIZED |
| 0x8604 | SHIP_CARGO | n | 16 u64 agent, 24 u32 epoch, 28 reserved, 32 u64 opId, 40 n × (u32 item, i32 count, u64 reserved): the hold, exactly | 40 + 16n | COMMAND_RESULT |
| 0x8605 | SHIP_DESPAWN | reason: 1 lost, 2 recalled | 16 u64 agent, 24 u32 epoch | 32 | SHIP_DEMATERIALIZED |

### 7.5 How Java would apply them

**Materialize** spawns the role's ship body and its crew as one group at (x, y, z), raised above
the surface with flag 1 (the engine flies sky ships at a forced cruise height while unloaded, as
Veloren's airships, so one never appears underground; sea ships come at sea level, 63). It tags
every member with the agent and epoch, as the bridge tags puppets, and suppresses vanilla saving
(P3). The hold gets the cargo. A merchant or navy ship then **follows its timetable**: docked at
waypoint 0 until `departTick`; departing over the first `harbourBlocks` of the path until
`cruiseTick` (a sky ship climbing from its dock's height to `cruiseY`); cruising until
`approachTick`; approaching over the last `harbourBlocks` until `arriveTick` (descending to the
next dock's height), at the platform given. The engine's positions are these same pieces of the
path at constant speed each, so the body and the engine agree without physics; a body that falls
behind (blocked) catches up or reports where it is, and the engine's timetable stays as it is. Each
time it reaches a dock while embodied, a SHIP_TIMETABLE gives the next voyage and, for a merchant,
a SHIP_CARGO its hold: the plugin replaces the hold exactly (what it carried was landed by the
flow) and answers COMMAND_RESULT, applied, or duplicate for an op id it already applied (U18). A hostile pirate
(flag 2) attacks its target agent; a pirate without a target holds its position (hunting) or sails
where SHIP_STEER says.

**Fights** are the plugin's to play with real entities while a player watches. When one ends it
reports SHIP_FIGHT (the real outcome the engine's abstract fights are floored by: never better for
the defender than what was seen), and what changed hands: CARGO_LOOTED (by a ship or a player),
SHIP_CAPTURED, SHIP_SUNK, SHIP_DAMAGED.

**Leaving.** A ship that stops entity-ticking for every player is captured and despawned with
SHIP_DEMATERIALIZED (hull and crew: wounds persist as numbers; holes in hulls are not kept, PLAN.md
§10.1's UNVERIFIED sub-level templates). SHIP_DEPART steers a pirate away and despawns it once out of
view or after the timeout.

**Ports and havens** are reported once, when found (PORT_FOUND, HAVEN_FOUND). How a plugin finds
them is P8: a structure (a harbour piece, an airship tower), a marker block, or a village feature.

### 7.6 What the engine side does with them (rtsim-ships)

- **Lanes are world edges** (`World::add_lane`): merchant ships carry the ports' flows exactly as
  caravans carry roads' (A13), capped by their holds; sea lanes follow a water mask's paths, sky
  lanes are straight. Routes are closed loops after Veloren's (the lane graph made Eulerian and
  split into loops); each loop is sailed both ways.
- **Timetables**: a ship's position is a closed form of the clock and its saved clock offset, so a
  restart moves no ship. Unobserved ships cost nothing: no event, no booking.
- **Enters view** (A15): a player's interest area (on the native engine, the player's one shared
  area, §5 item 13) books the voyages it could see into the shared leg index and asks it for every
  ship leg it shares cells with; each voyage enters a player's view once (a crossing) and
  materializes the ship and its crew as one group.
- **Pirates** at havens hunt lanes as a hazard (a share of each consignment sails to the haven's
  market, C24; traders price it as an expected loss and send less on a hunted lane, U9), get bored on a scheduled event brought forward by damage or a full hold, and raid
  a merchant a player sees with the lane's hazard as the chance; a raid nobody watches is
  Lanchester's square law floored by SHIP_FIGHT outcomes. A hunt may start with a raid on a coastal
  player district or a booked player leg through the industry threat interface (C16 to C18).
- **Navies** (M2's recommendation) sail patrol services whose presence lowers the lanes' hazard;
  players' escorts add to it (`Ships::set_escort`, for a future plugin).
- **Losses come back**: a lost merchant or navy ship relaunches at its home dock in its own slot; a
  haven refits a pirate ship's slot after `min(96, 4 x 2^P)` days once its crew pool can man it;
  hires and captured crews fill the pool up to 18 (EQUILIBRIUM.md §2.4.6, slice S8). A pirate hunts
  only a lane worth it, and with none rests in port.

## 8. Herds (PLAN.md §10.2; the core's vanilla herd adapter)

Written 2026-10-20 overnight with the engine side, `crates/rtsim-ecology/src/herds`
(`herds/boundary.rs` is the reference codec, with round-trip tests). Bodies are vanilla animals for
now (D4: nothing in the core depends on a pack mod), so the adapter is built into the core: package
`herd` of `mic_rtsim` (`HerdBridge`; `HerdRecords` mirrors this section; choices Y8 to Y21 in
`docs/overnight-choices-herdsmc.md`). Naturalist is a later body candidate (P8). The native engine
routes these records (§5 item 14). The fake in `crates/rtsim-ecology/src/herds/synth.rs`
(`FakeHerds`) plays the adapter in the crate's tests, `rtsim-cli sim herds` and rtsim-jni's
protocol test. Choices are numbered H2-# in `docs/overnight-choices-herds.md`. The vanilla facts
below were checked in the 1.21.1 sources (NeoForm's decompiled `EntityType`, `Sheep`, `Horse`,
`AbstractHorse`, `Goat`, `AgeableMob`, `EatBlockGoal`, `DyeColor`); anything else is the contract
the adapter meets.

### 8.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x0080 to 0x008F | Java to engine | herd latest-wins observations |
| 0x0700 to 0x07FF | Java to engine | herd journaled observations (the 24-byte prefix of §2.3) |
| 0x8700 to 0x87FF | engine to Java | herd commands (`cmdSeq` at 8) |

The bridge's, phase 1's, the industry's (`docs/industry-boundary.md` §1), the dragons' (§6.1) and
the ships' (§7.1) ranges stay as they are; these three are next to the ships'.

### 8.2 Ids and dictionaries

- **Agents.** A herd's materialized group has the agent id `0x48 << 56 | handle` (namespace byte
  0x48, "H"), the herd row's 48-bit handle, as dragons use 0x44 and ships 0x53. A herd has at most
  one group at a time; the group is one puppet unit (C25): one agent, one epoch, every member
  spawned, steered and captured together. Epochs as §2.2: new for every materialization.
- **Members** are not agents. A member is its **member index** (u64) in its herd: the identity
  the engine derives everything from (PLAN.md §4.4). Indices are never reused (births take a
  monotonic next index), so a member index names one animal forever.
- **Identity seed** (u64, per member): a pure function of (herd seed, member index). The plugin
  seeds the body's own random with it wherever vanilla rolls something per animal (a horse's
  health, speed and jump strength), so the same horse comes back with the same stats.
- **Species** (dictionary kind 3, body kinds): `minecraft:sheep`, `minecraft:cow`, `minecraft:pig`,
  `minecraft:horse`, `minecraft:goat` (verified entity type ids).
- **Players** are UUIDs (most, least), as everywhere. **Op ids** are the plugin's (top bit set,
  §2.2) on HERD_KILLED and HERD_TAKEN; the engine applies each once.

### 8.3 Observations

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0080 | HERD_GROUP_STATE (latest-wins) | flags: 1 ticking | 8 u64 agent, 16 u32 epoch, 20 u32 dimension, 24 u32 members present, 28 reserved, 32 f64 x, 40 f64 y, 48 f64 z (the group's centroid). Every 100 ticks per embodied group (PLACEHOLDER) | 56 |
| 0x0700 | HERD_MATERIALIZED | members spawned | 24 u64 agent, 32 u32 epoch, 36 u32 dimension | 40 |
| 0x0701 | HERD_SPAWN_REJECTED | reason (§2.3's codes, plus 9 unknown species) | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 cmdSeq | 48 |
| 0x0702 | HERD_DEMATERIALIZED | reason (§2.3's codes) | 24 u64 agent, 32 u32 epoch, 36 u32 n, 40 n × capture (24 bytes: 0 u64 member, 8 u32 flags (1 sheared), 12 f32 health, 16 i32 x, 20 i32 z) | 40 + 24n |
| 0x0703 | HERD_KILLED | by: 0 none, 1 player, 2 mob, 3 other | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 member, 48 u64 killerMost, 56 u64 killerLeast, 64 u64 opId | 72 |
| 0x0704 | HERD_SHEARED | 0 | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 member, 48 u64 playerMost, 56 u64 playerLeast | 64 |
| 0x0705 | HERD_TAKEN | how: 1 lead, 2 taming, 3 pickup, 4 another mod, 5 a plugin (§2.3's TAKEN codes) | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 member, 48 u64 playerMost, 56 u64 playerLeast, 64 u64 opId | 72 |
| 0x0706 | HERD_OBSTACLE | members that failed to pass | 24 u64 agent, 32 u32 epoch, 36 u32 members present, 40 i32 stuckX, 44 i32 stuckZ, 48 i32 targetX, 52 i32 targetZ | 56 |

All of 0x0700 to 0x0706 are journaled.

- **HERD_DEMATERIALIZED** captures the group: one entry per member still present. A member
  missing from it without a HERD_KILLED or HERD_TAKEN before is treated as still alive (the plugin
  lost track of it), never as dead.
- **HERD_KILLED** is journaled at the end of the tick, as DIED (§2.3). A player's kill is booked
  as killed; any other death (a wolf, a fall, a dragon's real attack while embodied) as lost. The
  member goes to the herd's dead-index set: it never materializes again.
- **HERD_TAKEN** (A10): a lead, taming (horses), a Carry On pickup or another mod's capture. The
  plugin strips the tag and the animal becomes an ordinary entity that vanilla saves; the engine
  removes the member from its year-class and books it as taken. It never materializes again.
- **HERD_SHEARED** makes the member notable (its sheared state is kept) until its wool has
  regrown, after which it folds back into its cohort (it stays known as seen).
- **HERD_OBSTACLE**: the group's members keep failing to pass a place on the way to the steer
  target (the adapter's rule, Y15 in docs/overnight-choices-herdsmc.md: at least half the members
  present lagging behind the herd and stalled for 30 seconds, PLACEHOLDER). The engine raises the cost of entering the cell a cell ahead of `stuck` toward
  `target` (H2-14) and replans the herd from `stuck` at once; the cost decays with a half-life of
  two sim years (PLACEHOLDER).

### 8.4 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8700 | HERD_MATERIALIZE | members m | 16 u64 agent, 24 u32 epoch, 28 u32 species (dictionary 3), 32 u32 dimension, 36 u32 flags (1 SURFACE: each member stands on the surface at its own x, z), 40 i32 centreX, 44 i32 centreZ, 48 i32 targetX, 52 i32 targetZ, 56 f32 speed (blocks a tick), 60 u32 footprint radius (blocks), 64 m × member (40 bytes: 0 u64 member, 8 u64 identity seed, 16 u32 variant, 20 i32 vanillaAge, 24 u32 ageDays, 28 u8 cohort (0 young, 1 adult, 2 old), 29 u8 flags (1 sheared), 30 u16 reserved, 32 i32 x, 36 i32 z) | 64 + 40m | HERD_MATERIALIZED or HERD_SPAWN_REJECTED |
| 0x8701 | HERD_STEER | 0 | 16 u64 agent, 24 u32 epoch, 28 f32 speed (blocks a tick), 32 i32 targetX, 36 i32 targetZ, 40 i64 arriveTick | 48 | |
| 0x8702 | HERD_DESPAWN | reason: 1 left view, 2 the player left, 3 recalled | 16 u64 agent, 24 u32 epoch, 28 reserved | 32 | HERD_DEMATERIALIZED |

The member **variant** is, per species (vanilla NBT, verified):
- sheep: the `Color` byte's dye id (white 0, pink 6, gray 7, light gray 8, brown 12, black 15),
  rolled with vanilla's own odds (`Sheep.getRandomSheepColor`: 5% black, 5% gray, 5% light gray, 3%
  brown, the rest white but one in 500 of those pink);
- horse: the `Variant` int, `colour | markings << 8` (7 colours, 5 markings, uniform as
  `Horse.finalizeSpawn` rolls them);
- goat: 1 for `IsScreamingGoat` (2%, as `Goat.finalizeSpawn`), else 0; horns as spawned;
- cow and pig: 0 (no variants in 1.21.1).

**vanillaAge** is `AgeableMob`'s `Age`: negative for a young member (a baby, at most −24,000 at
birth and closer to 0 as it nears the sim's maturity age), 0 for adults and old members. The sim's
cohorts run on its own clock (sim years); vanilla's 24,000-tick growth only decides how the body
looks.

### 8.5 How Java applies them (the core's `herd` package)

**Materialize** spawns every member at its (x, z) as one group in one tick (§2.4's quota counts the
whole group, Y13), all inside the entity-ticking area of the player the group was made for (the
engine places them in the part of the herd's footprint the player's area covers, at its inner
edge); a member whose column is not entity-ticking rejects the whole group (reason 1). The record
has no y: each member stands on the highest motion-blocking, non-leaf block of its column (Y11).
Each body is made as vanilla makes the animal (`finalizeSpawn`, called directly with spawn type
EVENT, so no `FinalizeSpawnEvent`), then gets: the agent and epoch tag, persistence required while
embodied, vanilla saving suppressed (P3), and through its own NBT read its variant (`Color`,
`Variant`, `IsScreamingGoat`), `Age` = vanillaAge and `Sheared` for flag 1 (Y10). What vanilla rolls
per animal comes from the identity seed: a horse's max health, speed and jump
(`AbstractHorse.randomizeAttributes` fed a random seeded with it; vanilla feeds the level's
random), a goat's lost horn (Y12). HERD_MATERIALIZED answers with the members spawned; a species
the adapter does not have is rejected with reason 9.

**Steering** (Y14): the group's anchor walks from the herd's centre toward `target` at `speed`; each
member keeps its place around it (its offset from the centre at spawn). A member more than 8 blocks
from its place (PLACEHOLDER) walks back to it (a goal below panic, breeding and tempting; goats
through their brain's walk target); otherwise vanilla's own AI runs. **Steer** sets a new target:
the herd's next waypoint. `arriveTick` is when the engine's closed form reaches it, so a body that
dawdles can catch up: the anchor arrives then.

**Despawn** captures the group (HERD_DEMATERIALIZED, reason 1, one capture per member present) and
removes the bodies; for a group not embodied it answers reason 6 with no capture. Any other loss is
captured the same way with §2.3's reasons: a member not entity-ticking (2), unloaded or changing
dimension (3), the session ending or the ECOLOGY switch off (4, Y9); a group whose last member died
or was taken is answered reason 5 with no capture.

**Takings and deaths** are reported as they happen: HERD_KILLED at the end of the tick of a certain
death, with the killer (Y17), its drops carrying it (§3.1); HERD_TAKEN for a lead (any holder), a
tamed horse, Carry On's pickup (`RtsimApi.takeFromSim`, through `SessionPlugin.take`) or a body
another mod removed (Y16), the animal untagged and carrying its record; HERD_SHEARED when a member
turns sheared, naming the player who used shears on it that tick (Y18). A taken animal leaves the
group (C25: taking one member splits it from its group) and becomes ordinary.

**Obstacles** (Y15): a member more than 14 blocks behind the anchor's progress toward the waypoint
that has gained less than a block for 600 ticks (PLACEHOLDER) has failed to pass; when at least half
the members present have, HERD_OBSTACLE goes once per waypoint, with where they stand on average.

**HERD_GROUP_STATE** goes every 100 ticks per group (Y19).

**Vanilla's own animals.** Vanilla keeps spawning its own passive animals at chunk generation; the
sim's herds are extra bodies over them. The adapter neither suppresses nor adopts them; whether it
should (as the dragons adopt lair dragons, §6.6) is open (H2-28; the recommendation is Y20).

### 8.6 What the engine side does with them (rtsim-ecology's herds)

- **Herds are group rows** (SCALE.md §2): counts per year-class (young, adult and old are sums by
  age), body condition per cohort, a route that is a closed form of time, and the few members a
  player has seen. An animal nobody has seen costs zero bytes.
- **Each sub-season** (staggered by a hash of the herd) a herd is charged for its grazing, gives
  birth if it is its breeding sub-season (one binomial draw on the adults), and picks its next
  target from a forage flow field within its comfort band. Deaths are dyadic counts per
  year-class (O5's recommendation), read in O(log) whenever anything asks: catch-up is flat in
  elapsed time.
- **Legs are booked lazily** (A15): the herds index their routes by region themselves; only when a
  player's interest area is near (the world's one area per player, the herds being one of its
  viewers, owner bit 60) does the part of a route inside the area's window go into the shared leg
  index, with one meeting per approach; the group materializes when the footprint
  reaches into the area: count × the share of the footprint in view at closest approach, capped
  per herd (PLACEHOLDER 32) and per player (PLACEHOLDER 48).
- **Identity**: seen members come back first (most recently seen first), then fresh members by
  cohort share; anonymous deaths never fall on a seen member while unseen ones remain; killed and
  taken members are in the dead-index set and never come back.
- **Predators**: dragons weigh herds near their lair (and farther for a herd they follow) and take
  latent animals by cohort weight (`Herds::predate`); an embodied group's animals are the
  plugin's, lost only in a real fight (HERD_KILLED, by 2).
## 9. Conflict (PLAN.md §10.3; a future conflict plugin)

Written 2026-10-20 overnight with the engine side, `crates/rtsim-conflict` (`boundary.rs` is the
reference codec, with round-trip tests). **No plugin exists**, and none is chosen: which mods give
the bodies (Goblin's Tyranny for goblins, Illager Invasion for raiders, `minecraft:pillager` as the
fallback) and which structures are camps and havens are P8 (Ben). So every record names **abstract
body kinds**, and the plugin maps them to a mod's entities (P15's body-kind data); D7: mob-like
bodies only. The executable reference for what a plugin answers is the fake in
`crates/rtsim-conflict/src/synth.rs` (`FakeConflict`), which the tests and `rtsim-cli sim conflict`
drive through this codec. The native engine routes them (§5 item 15, choices C2-# in
`docs/overnight-choices-conflictjni.md`). Choices are numbered K2-# in
`docs/overnight-choices-conflict.md`. Nothing below about a
body mod is verified: it is the contract a plugin would have to meet.

**Mowzie's Mobs** (2026-10-22: MOWZIE.md §2, §4.12 and ledger 14; slice M1, choices MZB-1 to MZB-49 in
`docs/choices-mowzie-build.md`) adds 17 kinds and DEMAND_SETTLED outcome 5. Its Java half (the
conflict module in core, slice M3, and the plugin `neoforge/mowziesmobs`, slice M5) is the first
plugin. The engine half of most of the new kinds comes with a later slice (§9.6); until then the
engine decodes them, counts them as deferred and drops them, and sends none of the new commands.

### 9.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x0090 to 0x009F | Java to engine | conflict latest-wins observations |
| 0x0800 to 0x08FF | Java to engine | conflict journaled observations (the 24-byte prefix of §2.3) |
| 0x8800 to 0x88FF | engine to Java | conflict commands (`cmdSeq` at 8) |

The bridge's, phase 1's, the industry's (`docs/industry-boundary.md` §1), the dragons' (§6.1), the
ships' (§7.1) and the herds' (§8.1: 0x0080 to 0x008F, 0x0700 to 0x07FF, 0x8700 to 0x87FF) ranges
stay as they are; these three are next to the herds'. Mowzie's kinds take 0x0091 and 0x0092
(latest-wins; 0x0093 is retired, DIFFICULTY's before it was journaled), 0x080C to 0x0815 (journaled)
and 0x8807 to 0x880B (commands). **Dictionary kind 0x20**
(each direction, as §2.2) names Destroy pollution types (`destroy:smog`); the industry's 0x10 to 0x12
are untouched.
**Pages**: table 0x4300 ('C'), the meta page at id 0 and the rows at 0x4310 and up, as the ships'
0x5300 (§5 item 15).

### 9.2 Ids and dictionaries

- **Agents.** A party's agent id is `0x43 << 56 | handle` (namespace byte 0x43, "C"), the engine
  row's 48-bit handle, as dragons use 0x44, ships 0x53 and herds 0x48. A party (an envoy and its
  guards, scouts, a war band, plunderers) is one group (C25): one agent, one epoch per
  materialization, its members listed by body kind and count. Waves of one watched raid share the
  agent and the epoch (`wave` counts them).
- **Factions** are keyed by their camp's key (u64), packed as §4's villages are: the plugin reports
  the camp (FACTION_FOUND) and the engine joins it to the world as a haven node (C24).
- **Body kinds** (dictionary kind 3), abstract until P8, from the culture data
  (`crates/rtsim-conflict/data/cultures/*.json`): `mic_rtsim:conflict/goblin/warrior`, `.../archer`,
  `.../brute`, `.../emissary`; `mic_rtsim:conflict/bandit/thug`, `.../crossbowman`,
  `.../messenger`; `mic_rtsim:conflict/raider/vindicator`, `.../pillager`, `.../ravager`,
  `.../herald`.
- **Structures** (kind 4): a culture's camps, from its data (`goblinstyranny:goblin_camp`,
  `minecraft:pillager_outpost`, `illagerinvasion:illager_fort`, `mic_rtsim:conflict/bandit_camp`;
  the mod ids are UNVERIFIED).
- **Items** (kind 2): tribute, gifts and loot, mapped to the phase 1 catalogue's goods by name.
- **Cultures** (kind 5, as §4's, engine to Java only: BAND_MARCH) are named
  `mic_rtsim:conflict/<culture>` after the culture data's `name` (`mic_rtsim:conflict/goblin`), so
  they never meet phase 1's village cultures (`human`) in the one dictionary (MZB-12).
- **Players** are UUIDs (most, least); **(0, 0) means the settlers** (P14's recommendation: one
  shared actor for unowned signals and acts). **Op ids** are the plugin's (top bit set, §2.2) on
  CHIEF_KILLED, TRIBUTE_PAID, GIFT_GIVEN, BAND_LOOTED, CAMP_DEATH, TRADED and CHIEF_PAID; the engine
  applies each once (rtsim-jni hands them to `Replay`, which spends a deferred kind's too). The
  engine's commands move no items, so they carry no op ids, but for CAMP_RECONCILE: its op id is the
  engine's, allocator 0x43 in bits 56 to 62 (§2.2) over 56 bits derived from the faction, the epoch
  and the decision, and CAMP_BODIES carries it back so that an answer is booked once (MOWZIE.md §4.9).
- **Camps** (MOWZIE.md §4.9). A camp's **epoch** is a Java op id (top bit set) that the plugin takes
  each time the camp comes in (CAMP_CENSUS) and ends with CAMP_OUT; CAMP_BODIES, CAMP_RECONCILE and
  a CHIEF_BODY at home carry it. A camp body is known by its **slot and generation**; one found with
  neither (slot 0, generation 0) is a **stray**, known by its UUID.
- **Trains** are SplitMix64 of Create's train UUID, as the industry's (`docs/industry-boundary.md`
  H19).
- **Difficulty** is Minecraft's `Difficulty` id: 0 Peaceful, 1 Easy, 2 Normal, 3 Hard.

### 9.3 Observations

| Kind | Name | aux | Fields (offset type name) | Size |
|---|---|---|---|---|
| 0x0090 | BAND_STATE (latest-wins) | flags: 1 ticking, 2 fighting | 8 u64 agent, 16 u32 epoch, 20 u32 dimension, 24 u32 members present, 28 reserved, 32 f64 x, 40 f64 y, 48 f64 z. Every 100 ticks per embodied party (PLACEHOLDER) | 56 |
| 0x0091 | CHIEF_STATE (latest-wins) | flags: 1 crystal held | 8 u64 faction, 16 f32 health, 20 reserved | 24 |
| 0x0092 | HAVEN_CHEST (latest-wins) | 0 | 8 u64 faction, 16 u32 dimension, 20 i32 x, 24 y, 28 z, 32 u32 free slots (as LAIR_CHEST), 36 reserved | 40 |
| 0x0093 | retired (DIFFICULTY's until MZB-17 journaled it as 0x0815; never reuse) | | | |
| 0x0800 | FACTION_FOUND | flags: 1 LATE | 24 u64 faction (the camp's key), 32 u32 dimension, 36 u32 structure (dictionary 4), 40 i32 x, 44 y, 48 z, 52 u16 home bodies (the camp's bodies found at discovery), 54 u8 census (1 chief present, 2 crystal held), 55 reserved | 56 |
| 0x0801 | TERRITORY_SIGNAL | signal: 0 smog, 1 radiation, 2 logging, 3 mining, 4 killing, 5 building | 24 u64 faction (0: by position), 32 u32 dimension, 36 u32 count, 40 u64 playerMost, 48 u64 playerLeast ((0, 0): unowned, the settlers), 56 i32 x, 60 y, 64 z, 68 reserved | 72 |
| 0x0802 | POLLUTION | 0 | 24 u64 source (the district's id, or 0 and the engine keys the chunk), 32 u32 dimension, 36 u32 type (dictionary 0x20), 40 i32 x, 44 i32 z, 48 i64 amount over the window (Destroy's units; 0: the source stopped), 56 u32 windowTicks, 60 reserved | 64 |
| 0x0803 | BAND_MATERIALIZED | members spawned | 24 u64 agent, 32 u32 epoch, 36 u32 dimension, 40 u32 wave, 44 reserved | 48 |
| 0x0804 | BAND_SPAWN_REJECTED | reason (§2.3's codes, plus 9 unknown body kind) | 24 u64 agent, 32 u32 epoch, 36 u32 wave, 40 u64 cmdSeq | 48 |
| 0x0805 | BAND_DEMATERIALIZED | reason (§2.3's codes) | 24 u64 agent, 32 u32 epoch, 36 u32 n, 40 n × (u32 body, u32 survivors) | 40 + 8n |
| 0x0806 | BAND_CASUALTIES | killer kind: 0 none, 1 player, 2 mob, 3 other (4 is CAMP_DEATH's) | 24 u64 agent, 32 u32 epoch, 36 u32 body, 40 u32 count, 44 reserved, 48 u64 killerMost, 56 u64 killerLeast | 64 |
| 0x0807 | CHIEF_KILLED | killer kind (as CASUALTIES) | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 killerMost, 48 u64 killerLeast, 56 u64 opId | 64 |
| 0x0808 | ENVOY_ANSWER | 1 accepted, 2 refused, 3 attacked | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 playerMost, 48 u64 playerLeast | 56 |
| 0x0809 | TRIBUTE_PAID | n | 24 u64 faction, 32 u64 agent (the envoy; 0 at the camp), 40 u32 epoch, 44 reserved, 48 u64 playerMost, 56 u64 playerLeast, 64 u64 opId, 72 n × (u32 item, i32 count, u64 reserved) | 72 + 16n |
| 0x080A | GIFT_GIVEN | n | 24 u64 faction, 32 u64 playerMost, 40 u64 playerLeast, 48 u64 opId, 56 n × (u32 item, i32 count, u64 reserved) | 56 + 16n |
| 0x080B | BAND_LOOTED | n | 24 u64 agent, 32 u32 epoch, 36 reserved, 40 u64 opId, 48 n × (u32 item, i32 count, u64 reserved) | 48 + 16n |
| 0x080C | RAIL_PASS | 0 | 24 u64 faction, 32 u64 train, 40 u64 ownerMost, 48 u64 ownerLeast ((0, 0): none), 56 u32 dimension, 60 reserved, 64 f64 x, 72 y, 80 z (the sampled point nearest the camp), 88 i64 entryTick, 96 i64 exitTick | 104 |
| 0x080D | TRAIN_PASSED | flags: 1 watched | 24 u64 agent, 32 u32 epoch, 36 u32 carriage (the nearest the track point, from 0), 40 u64 train | 48 |
| 0x080E | CAMP_DEATH | killer kind: 0 none, 1 player, 2 mob, 3 other, 4 taken (carried off, Carry On) | 24 u64 faction, 32 u32 body (dictionary 3), 36 u32 flags (1 the chief, 2 a stray), 40 u32 slot, 44 u32 generation, 48 u64 killerMost, 56 u64 killerLeast, 64 u64 opId | 72 |
| 0x080F | CAMP_CENSUS | bodies n | 24 u64 faction, 32 u64 epoch (new), 40 u32 flags (1 chief present, 2 crystal held, 4 the throne spot is given), 44 f32 the chief's health, 48 f64 throneX, 56 throneY, 64 throneZ, 72 f32 direction, 76 u32 traded t, 80 u32 traded slot, 84 u32 traded generation (the body whose trade memory follows), 88 n × (u32 body, u32 flags: 1 stray, 2 loaded elsewhere, 4 the chief; u32 slot, u32 generation, u64 uuidMost, u64 uuidLeast), then t × (u64 playerMost, u64 playerLeast) | 88 + 32n + 16t |
| 0x0810 | TRADED | 0 | 24 u64 faction, 32 u64 playerMost, 40 u64 playerLeast, 48 u32 gold nuggets that changed hands, 52 reserved, 56 u64 opId | 64 |
| 0x0811 | NOTICE_SHOWN | the notice's serial (FACTION_NOTICE's 60) | 24 u64 faction, 32 u64 playerMost, 40 u64 playerLeast (the notice's; (0, 0) the settlers) | 48 |
| 0x0812 | CAMP_BODIES | lines n | 24 u64 faction, 32 u64 epoch, 40 u64 opId (the CAMP_RECONCILE's), 48 n × (u32 line (its index in the reconcile, from 0), u32 slot, u32 generation, u32 outcome: 1 done, 2 nothing to do, 3 stale) | 48 + 16n |
| 0x0813 | CAMP_OUT | 0 | 24 u64 faction, 32 u64 epoch (the one that ended) | 40 |
| 0x0814 | CHIEF_PAID | items n | 24 u64 faction, 32 u32 slot, 36 u32 generation, 40 u32 flags (1 a stray), 44 reserved, 48 u64 playerMost, 56 u64 playerLeast, 64 u64 opId, 72 n × (u32 item, i32 count, u64 reserved) | 72 + 16n |
| 0x0815 | DIFFICULTY | 0 | 24 u32 difficulty (§9.2), 28 reserved | 32 |

All of 0x0800 to 0x0815 are journaled; 0x0090 to 0x0092 are latest-wins (§2.3), and 0x0093 is
retired. Of the Mowzie kinds, the engine acts on DIFFICULTY and NOTICE_SHOWN today; the rest are
deferred (§9.6).

**List caps.** Every list of a variable-length record (observations and commands alike: members,
items, waypoints, unit kinds, census bodies, traded players, reconcile lines) carries at most 1,024
entries (`boundary::MAX_ENTRIES`). At the cap every kind still fits the header's u16 size: the
largest, CAMP_CENSUS with both its lists full, is 88 + 32 × 1,024 + 16 × 1,024 = 49,240 bytes. An
encoder writes the first 1,024 entries of a longer list, and the count it writes is theirs; a decoder
refuses a count above the cap (`CodecError::TooMany`) and a record cut short of its lists
(`CodecError::Short`). Framing a record over 65,535 bytes is a bug the encoder asserts against
(MZB-35).

- **TERRITORY_SIGNAL** is what the plugin counts inside a territory the engine told it about
  (TERRITORY): logs broken (2), ore broken (3), blocks placed on the faction's plots (5), members
  killed outside a party's group (4), each by the acting player, or (0, 0) for an act with no player
  (a Create drill, a deployer: P14 gives it to the settlers). It may batch a player's acts of one
  kind over a few seconds into one record. Ore drained by an unloaded district reaches the engine
  from the industry engine's deposits, not from here.
- **POLLUTION** is the Destroy plugin's attribution by position (I14) aggregated per source and
  window: the engine sets the source's rate to amount ÷ window (a continuous signal, so the
  culture's tolerated rate applies). While a district is unloaded, the composition sets the same
  source's rate from the industry engine's learned rate of the pollution good (choice K2-9).
- **BAND_DEMATERIALIZED** after the engine's BAND_RECALL can only lower the counts: those that died
  on the way out go; a despawn the plugin decided (chunks unloaded) sets them. It names its band by
  agent and epoch, and the engine keeps the bodies of every epoch it recalled until that epoch's
  despawn comes, so one that arrives while the band is in view again at a newer epoch still counts
  its dead (MZB-131). Across a restart the bodies the pages held in Minecraft are kept the same way
  until the journal's replay has caught up, so a BAND_CASUALTIES or a BAND_DEMATERIALIZED journaled
  after the last page save is counted as the session that wrote it counted it; what the replay
  did not count is dropped then, its members alive as before (MZB-138). The replay has caught up
  when the engine's `Replay` has applied every record before the session's first live one, however
  many hand-overs Java drained its journal over (MZB-141); a BAND_CASUALTIES for a recalled band
  counts off the bodies kept for its epoch live and replayed alike (MZB-142).
- **BAND_CASUALTIES** by a player is also a direct act against the faction: a killing signal from
  that player (P14). CHIEF_KILLED routs the band (§9.5) when the band still has a chief: the chief
  flag (BAND_CHIEF, and MEMBER_CHIEF on its first stack) goes when the chief dies of age while out,
  the heir taking over at home, so the band carries on without a chief body, a chief body already in
  Minecraft stays there until the band despawns, and a CHIEF_KILLED for it is dropped (MZB-139,
  MZB-146); the plugin still journals it when that body dies (nothing else comes of it on the Java
  side: the death and its drops are the mob's own, and the in-world news is a no-op until it lands:
  MZB-147).
  Casualties that leave an envoy party with
  no member, none of the party's casualties or its chief's death having named a player, withdraw its
  demand (DEMAND_SETTLED 5, §9.4).
- **DIFFICULTY** is sent at every session start and whenever the world's difficulty changes. It is
  journaled (MZB-17), so the engine applies it at its game time and its replay after a crash applies
  it again in the same order as the rest; one with the value the engine already has changes nothing.
  Like every journaled record it is ignored while the module is off (U10); the session start's
  sends it again (MZB-30).
- **NOTICE_SHOWN** says the plugin has played a FACTION_NOTICE in the world: its serial in aux and
  its faction and player as the notice's. One with a serial other than the kept notice's (an older
  notice) changes nothing (MZB-10, MZB-28).
- **CAMP_CENSUS**, **CAMP_OUT**, **CAMP_BODIES** and **CAMP_DEATH** are the camp reconciler's
  (MOWZIE.md §4.9): a census when every chunk of a camp's footprint is FULL, under a new epoch, with
  every body of the camp's kinds in the footprint (by slot and generation, or a stray by UUID), the
  camp's tagged bodies loaded elsewhere, and for an Umvuthi its throne spot and trade memory; CAMP_OUT
  when a footprint chunk drops below FULL; CAMP_BODIES as the answer to a CAMP_RECONCILE, line by
  line; CAMP_DEATH once per body UUID, at home or left behind. CAMP_BODIES and CAMP_DEATH are carried
  on the bodies and markers they changed until acknowledged, and journaled again at load.
- **CHIEF_STATE** is a chief's health and crystal at home (the Frostmaw's steal, MOWZIE.md §4.5);
  **CHIEF_PAID** a player's first payment to an Umvuthi body (its `rememberTrade`), with that body's
  slot and generation; **TRADED** a masked trade with the grove's minions; **HAVEN_CHEST** a grove
  chest tribute can go into.
- **RAIL_PASS** is one Create train's pass through a camp's range, at the sampled point nearest the
  camp (MOWZIE.md §4.3); **TRAIN_PASSED** a train passing a lurking party's AMBUSH_SPOT (§4.4).

### 9.4 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8800 | TERRITORY | flags: 1 removed | 16 u64 faction, 24 u32 dimension, 28 u32 signal mask (bit = signal number), 32 i32 x, 36 i32 z, 40 u32 radius, 44 reserved | 48 | |
| 0x8801 | FACTION_NOTICE | rung (1 notice) | 16 u64 faction, 24 u64 playerMost, 32 u64 playerLeast ((0, 0): the settlers, anyone near), 40 u32 dimension, 44 u32 signal (the strongest), 48 i32 x, 52 i32 z, 56 f32 grievance, 60 u32 serial (the engine's, from 1, one per notice; NOTICE_SHOWN echoes it) | 64 | |
| 0x8802 | ENVOY_MATERIALIZE | members m | 16 u64 agent, 24 u32 epoch, 28 u32 dimension, 32 u64 faction, 40 f64 x, 48 y, 56 z, 64 u64 playerMost, 72 u64 playerLeast (whom; (0, 0) any player), 80 u32 demand flags (1 tribute, 2 stop the signal), 84 u32 signal, 88 u32 tribute item (dictionary 2), 92 i32 tribute count, 96 i64 deadlineTick, 104 m × (u32 body, u32 count) | 104 + 8m | BAND_MATERIALIZED or BAND_SPAWN_REJECTED |
| 0x8803 | BAND_MATERIALIZE | member stacks m | 16 u64 agent, 24 u32 epoch, 28 u32 dimension, 32 u64 faction, 40 u8 purpose (1 envoy, 2 scouts, 3 raid, 4 invasion, 5 siege, 6 plunder), 41 u8 flags (1 hostile, 2 the war chief leads, 4 SURFACE), 42 u16 wave, 44 f32 bearing (degrees from +x toward +z), 48 f64 arcX, 56 f64 arcZ, 64 u32 arcRadius (0: spawn at the arc's centre), 68 f32 half width (degrees), 72 f64 targetX, 80 targetY, 88 targetZ, 96 u64 targetPlayerMost, 104 u64 targetPlayerLeast, 112 f32 speed (blocks a tick), 116 reserved, 120 m × (u32 body, u32 count, u32 flags: 1 the chief is in this stack, u32 reserved) | 120 + 16m | BAND_MATERIALIZED or BAND_SPAWN_REJECTED |
| 0x8804 | BAND_STEER | flags: 1 attack at the target | 16 u64 agent, 24 u32 epoch, 28 f32 speed, 32 f64 x, 40 f64 y, 48 f64 z, 56 i64 arriveTick | 64 | |
| 0x8805 | BAND_RECALL | reason: 1 bored, 2 routed, 3 done, 4 passing out of view, 5 the player left, 6 settled (the demand was settled, or the world became Peaceful) | 16 u64 agent, 24 u32 epoch, 28 u32 timeoutTicks, 32 f64 towardX, 40 f64 towardZ | 48 | BAND_DEMATERIALIZED |
| 0x8806 | DEMAND_SETTLED | 1 paid, 2 signal cut, 3 expired, 4 refused, 5 withdrawn | 16 u64 faction, 24 u64 agent (the envoy, 0 none), 32 u64 playerMost, 40 u64 playerLeast, 48 u32 signal, 52 reserved | 56 | |
| 0x8807 | AMBUSH_SPOT | flags: 1 removed (the lurk ended) | 16 u64 agent, 24 u32 epoch, 28 u32 dimension, 32 u64 faction, 40 f64 trackX, 48 trackY, 56 trackZ (the sampled track point), 64 f64 spotX, 72 spotY, 80 spotZ (where the body stands), 88 f32 reach (blocks from the track point), 92 reserved | 96 | TRAIN_PASSED |
| 0x8808 | CAMP_RECONCILE | lines n | 16 u64 faction, 24 u64 epoch, 32 u64 opId (the engine's, §9.2), 40 f64 throneX, 48 throneY, 56 throneZ, 64 f32 direction, 68 u32 flags (1 the throne spot is given), 72 n × (u32 slot, u32 generation, u32 body (dictionary 3; 0 none), u8 action: 1 spawn, 2 adopt a stray, 3 take, 4 set out, 5 hand back, 6 remove, 7 seat the heir; u8 flags: 1 the chief, 2 crystal held; u16 reserved, u64 a, u64 b: a stray's UUID (adopt, remove a stray), a party's agent and epoch (set out, hand back), else 0) | 72 + 32n | CAMP_BODIES |
| 0x8809 | TRAIN_ROBBED | 0 | 16 u64 faction, 24 u64 train | 32 | |
| 0x880A | CHIEF_BODY | traded t | 16 u64 faction, 24 u64 agent (0 at home), 32 u64 epoch (the party's, or the camp's at home), 40 f32 health, 44 u32 flags (1 crystal held), 48 t × (u64 playerMost, u64 playerLeast) | 48 + 16t | |
| 0x880B | BAND_MARCH | waypoints w | 16 u64 agent, 24 u32 revision, 28 u32 dimension, 32 u64 faction, 40 u32 culture (dictionary 5; 0 none), 44 u8 kind (BAND_MATERIALIZE's purposes, 1 envoy to 6 plunder), 45 u8 phase (1 mustering, 2 out, 3 waiting, 4 fighting, 5 home), 46 u8 flags (1 the chief leads, 2 ended, 4 joint), 47 reserved, 48 u64 partner (the joint attack's other contingent), 56 i64 musterEndTick, 64 u32 unit kinds k, 68 reserved, 72 k × (u32 body, u32 count), then w × (i32 x, i32 z, i64 tick) | 72 + 8k + 16w | |

Every conflict command is per session: puppets never outlive one (the dragons' S11 and the ships'
U4), so none is kept or resent, with three exceptions. TERRITORY is sent again for every faction at a
session start (`Conflict::session_start`, C2-9), and every FACTION_NOTICE that no NOTICE_SHOWN has
answered goes again after the journal's replay, so that a NOTICE_SHOWN the pages missed ends its
notice first (MOWZIE.md ledger 14, MZB-20, MZB-31). BAND_MARCH goes again for
every party at a session start (MOWZIE.md §4.16, slice M10). A CAMP_RECONCILE not answered by a
session's end is void; the camp's next census is reconciled afresh (MOWZIE.md §4.9). DEMAND_SETTLED 4
(refused) answers an ENVOY_ANSWER refusal or an attack: a refused demand is never a cut signal
(C2-12). DEMAND_SETTLED 5 (withdrawn) follows the last member of an envoy party falling when no
player killed any of it (BAND_CASUALTIES or CHIEF_KILLED with killer kind 1): no relief, no refusal
and no climb. The feud holds on Envoy until the withdrawn demand's deadline, then sends a fresh envoy
with a new demand (MOWZIE.md ledger 14, MZB-22). No BAND_RECALL follows when nobody is left in
Minecraft; one for members still spawned has reason 6 (MZB-32). The plugin drops the demand it held
and plays nothing for it.

**BAND_MARCH** replaces the agent's earlier ones (the higher revision wins). The engine's ticks in it
(musterEndTick, each waypoint's) are game time, as ENVOY_MATERIALIZE's deadline; its waypoints are the
party's route from the segment it is on (`route[seg]` and `times[seg]`), in chart coordinates. Its
joint flag and partner are zero until alliances land (CONFLICT-LIFE.md §2.16).

### 9.5 How Java would apply them

**Territory.** The plugin keeps each faction's circle and counts, inside it, the signals its mask
names (TERRITORY_SIGNAL). It needs no body mod for this: block break and place events and a mixin
or event for kills are vanilla and NeoForge.

**A notice** is the faction showing it has noticed, in the world only: no faction news goes to chat
or the action bar (Ben, 2026-10-22); feuds, raids and deaths reach players as gossip (the news
system). The plugin plays it when the player it names is in range (the Frostmaw's roar from toward
its den, MOWZIE.md §4.11; for a culture with nothing to play, a sign at the place or nothing) and
then journals NOTICE_SHOWN with the notice's serial (FACTION_NOTICE's 60). Until then it holds the
notice for the session. A notice the session ends on unplayed comes again at the next session start,
after its faction's TERRITORY and the journal's replay, so across logouts and restarts it waits for
the player's next entry. A plugin that plays
nothing answers NOTICE_SHOWN at once.

**Difficulty.** The plugin journals DIFFICULTY at every session start and on every change of the
world's difficulty. On Peaceful the engine sends no war band and no envoy, and recalls the ones in
the world: a war band with BAND_RECALL 6, an envoy in view with BAND_RECALL 4 (passing; it walks off
and stays abstract, MZB-132): the plugin needs no rule of its own.

**An envoy** is spawned as one group at (x, y, z) (SURFACE), peaceful, and walks to or waits at the
target as BAND_STEER says. Interacting with it shows the demand: stop the signal, or pay `count`
of the item before `deadlineTick`. Paying is a TRIBUTE_PAID with the envoy's agent (the items leave
the player's inventory; an op id); refusing or attacking is an ENVOY_ANSWER. An envoy that is
killed is a CASUALTIES record like any member's (and an insult: ENVOY_ANSWER 3). Any player's blow
on the envoy or a guard lifts the guard. A blow from the player the demand is to (or from any
player, for a demand to none) closes it on the plugin's side (the envoy takes nothing more) and is
answered once with an ENVOY_ANSWER 3: the engine books the insult against that player's district
and refuses the demand (DEMAND_SETTLED 4). A blow from another player is answered once per player
with an ENVOY_ANSWER 3 naming them: the engine books it as their own insult, at the envoy's place
and against no district, and refuses nothing; the demand stays open to its player and the plugin
goes on taking that player's tribute (MZB-145). ENVOY_ANSWER is an envoy party's: the engine
ignores it for any other party (MZB-144).

**The raid director.** A war band that arrives where a player watches materializes in **waves**:
each BAND_MATERIALIZE with an `arcRadius` spawns its members on the arc of that radius around
(arcX, arcZ), the watching player's position, within `half width` degrees of `bearing`: the side
the band came from, at the edge of the entity-ticking area (PLACEHOLDER 112 blocks). A wave is never
more than the spawn quota of members in Minecraft at once (PLACEHOLDER 16); the engine sends the
next wave as casualties free room, a wave interval apart. Hostile members attack the target
position and players. Every death is a BAND_CASUALTIES with the killer; the war chief's is a
CHIEF_KILLED (op id). When the engine recalls the band (bored, routed, done) the survivors walk
toward (towardX, towardZ) and the plugin despawns them once out of view or after the timeout, and
reports BAND_DEMATERIALIZED with who is left. Items a body really carried off from a container are
BAND_LOOTED (op id). The fight itself, and any block a body breaks, is vanilla's and the body mod's;
the industry plugin observes it as a fight at the district (IND_FIGHT, I21), which floors later
abstract raids there.

**A band seen on the march** (its leg crosses a player's interest area) is materialized at its
place (`arcRadius` 0) and steered along its road (BAND_STEER, one per waypoint, with the tick it
should get there); it is recalled (reason 4) where its walk leaves the view. Bands on their way home
are not materialized.

**Mowzie's later records** (MOWZIE.md §4; the Java half of each comes with the slice that builds its
engine half, §9.6). The train sampler reports RAIL_PASS for each Create train through a camp's range
(§4.3). Against an AMBUSH_SPOT it samples the track point, sends TRAIN_PASSED for each train within
the reach (watched when a player watches the spot), and stops at the removed flag; it plays the
robbed-train roar for TRAIN_ROBBED when that train next reaches a station with a player near (§4.4).
The camp reconciler (§4.9) sends a census when a camp comes in and CAMP_OUT when it goes out, applies
a CAMP_RECONCILE's lines only while its epoch is in and answers with CAMP_BODIES, and reports each
camp body's death once (CAMP_DEATH). CHIEF_BODY is written into a chief's body (health, crystal, trade
memory by `setTradedPlayersCompound`) before its stack, spawn line or hand-back (§4.5).

### 9.6 What the engine side does with them (rtsim-conflict)

- **Factions** come from FACTION_FOUND (or the composition's own data): a culture from the
  structure, a camp joined to the world as a haven node (C24: what they steal is stocked and sold
  there), a territory, a chief and an heir (knowledge notables: temperament from the seed bends
  thresholds, tribute, patience and forgiveness, C25; the heir inherits the chief's strongest
  grudges, C24), and a warrior pool recruited back in closed form.
- **Grievance** per (faction, actor) is a closed form between events (exponential forgiveness plus
  the continuous signals over their tolerance), so catch-up is flat in elapsed time; impulses jump it.
  Its next rung is the time the curve crosses that rung's threshold: one booked event per feud.
- **The ladder**: notice (FACTION_NOTICE), an envoy with a demand (ENVOY_MATERIALIZE when seen),
  scouts, a raid, an invasion, a siege. De-escalation: tribute paid in full (back to notice, a share
  of the grievance gone, a respite), the signal cut by the deadline, gifts and trade at the haven,
  the war chief killed (the band routs, the faction is cowed, the feud drops rungs, the heir holds
  the grudge), and plain forgiveness falling under a rung (hysteresis).
- **War bands** muster from the pool, march the road graph as legs in the world's leg index (owner
  bit 59), may ambush a booked train leg on the way (I24), get bored at the target on a booked
  departure brought forward by losses (C24), and go home with their loot. **Unwatched**, a raid on a
  player district goes through the industry threat interface (never better than observed, capped,
  applied on load even with the owner offline: C16 to C18), and a raid on a village or a rival
  haven moves stock between the world's markets (taken into the band's hold, stocked at the haven
  on its return: conservation exact). A district raid's threats carry the band's own loads (its
  units' carry and blocks broken, `Threat::loads`, C2-1); threats without loads keep the industry
  engine's per-class tunables. **Watched**, the director above.
- **Villages that hate a faction** hear when a player breaks one of its bands (a rescue report, C24),
  and remember the raids on them (a robbery report against the faction).
- **Peaceful** (DIFFICULTY 0, MOWZIE.md §4.12). No war party (raid, invasion, siege, plunder) leaves:
  `plan_feud` books no climb to a war rung, and a war party mustering when Peaceful begins stands
  down at its muster's end (its warriors back in the pool, its feud a rung down, so that it climbs
  again later). When Peaceful begins, every war party already out stops at once (MZB-15): one on the
  road goes home, its feud a rung down (MZB-29); a watched fight ends; a siege waiting at its target
  goes home. Those in Minecraft get BAND_RECALL 6. No war body spawns while it is Peaceful, and an
  ambush booked before never falls. Notices and scouts go on, and grievance keeps its curve. No
  envoy leaves either, its body being a stand-in that cannot exist on Peaceful (MZB-132): the climb
  to Envoy waits like a war rung's, an envoy in view walks off (BAND_RECALL 4) and stays where it is,
  abstract, and its muster's end or its wait is held; no demand is made or comes due, so a feud
  holds at its notice with its grievance kept. A refusal that reaches the engine on Peaceful is
  recorded, but its climb to a war rung waits for Normal (MZB-134). When Peaceful ends every feud is
  planned again, its dwell clock paused over the Peaceful span (a span of no time moves nothing,
  MZB-135) and shifted by a per-feud jitter in [0, dwell) from the faction and the player (MZB-16), so
  no burst of raids follows; every open demand's deadline moves by the span it lived through, and the
  envoys held go on to it, their patience moved by the part of the span they spent waiting
  (MZB-132, MZB-136). The flag and the span's start are saved (MZB-14 to MZB-17).
- **Kept notices** (ledger 14). A feud's last FACTION_NOTICE is kept in its row until a NOTICE_SHOWN
  for its serial, faction and player, or until the feud is forgotten; a later notice replaces it.
  Serials are engine-wide, so a forgotten and re-made feud never reuses one (MZB-28). Each session
  sends it again after its faction's TERRITORY and the journal's replay (MZB-18 to MZB-21, MZB-31).
- **Withdrawn demands** (ledger 14): DEMAND_SETTLED 5 above. The feud holds on Envoy to the old
  deadline, then a fresh envoy brings a new demand; a withdrawal never brings the climb forward
  (MZB-22).
- **Deferred kinds.** The engine halves of CHIEF_STATE (slice M6), HAVEN_CHEST, TRADED and CHIEF_PAID
  (M9), RAIL_PASS (M7), TRAIN_PASSED (M8) and the camp records (M4) come later, as do the commands,
  which the engine does not send yet, and FACTION_FOUND's census (M4). Until then rtsim-jni decodes
  each, translates its ids, counts it (`ConflictLinkStats::deferred`) and drops it before the shared
  clock runs for it: no module catches up for a deferred record. One with an op id spends it in
  the engine, in rtsim-jni as in `Conflict::ingest`, so the same record again is not counted twice
  (MZB-23).

## 10. The world model: HABITAT (PLAN.md §5.1, SCALE.md §2, §4; the core's habitat sampler)

**Status: a boundary addition made overnight on 2026-10-20 for Ben's review.** Choice Y4 in
docs/overnight-choices-herdsmc.md flagged a habitat record as his call; it was built as Y4
recommended so the engine knows terrain, and every choice below (X2-#, in
docs/overnight-choices-worldmodel.md) can be undone. The engine side is `crates/rtsim-space`
(`habitat.rs` is the reference codec, with round-trip tests) and `crates/rtsim-jni/src/engine/
worldmodel.rs`; the Java side is the core's `habitat` package (`HabitatSampler`, `HabitatRecords`
mirrors this section). Vanilla is built in (D4), so the sampler is the core's own `SessionPlugin`.
Vanilla facts below were checked in the decompiled 1.21.1 sources with NeoForge 21.1.249's patches.

### 10.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x00A0 to 0x00AF | Java to engine | world-model latest-wins observations |
| 0x0900 to 0x09FF | Java to engine | world-model journaled observations (none yet) |
| 0x8900 to 0x89FF | engine to Java | world-model commands (`cmdSeq` at 8) |

The bridge's, phase 1's, the industry's (`docs/industry-boundary.md` §1), the dragons' (§6.1), the
ships' (§7.1), the herds' (§8.1) and the conflict's (§9.1) ranges stay as they are; these three are
next to the conflict's.

### 10.2 Cells, points and dictionaries

- **A cell** is 64 x 64 blocks (SCALE.md §2's field cells), `cellX = floor(x / 64)`; a 512-block
  region holds 8 x 8 cells, index `(cellX mod 8) + 8 (cellZ mod 8)`.
- **A point** is one chunk's centre column (block 8, 8 of the chunk): 16 points a cell, point `i`
  being chunk `(i & 3, i >> 2)` of the cell. Every mask below has bit `i` for point `i`.
- **What a point is.** The top block of the `MOTION_BLOCKING_NO_LEAVES` heightmap
  (`ChunkAccess.getHeight` returns that block's y; the heightmap counts fluids, so a lake's top is
  its water): **water** when that block's fluid is in `minecraft:water` (`FluidTags.WATER`),
  **ice** when the block is in `minecraft:ice` (`BlockTags.ICE`), **other** when it holds another
  fluid or the column has no block at all (the void), **land** otherwise. Under water,
  `OCEAN_FLOOR` gives the floor. The biome is the chunk's noise biome at the top block
  (`ChunkAccess.getNoiseBiome`, quart coordinates).
- **Dictionaries.** Biomes travel as **dictionary kind 0x30** (Java's, per world:
  `minecraft:plains`); creature kinds as body kinds (kind 3: `minecraft:sheep`); the dimension as
  kind 1.

### 10.3 HABITAT (0x00A0, latest wins)

| Offset | Type | Name |
|---|---|---|
| 4 (aux) | u32 | n: creature entries |
| 8 | u32 | dimension (dictionary 1) |
| 12 | u32 | fingerprint (§10.4) |
| 16, 20 | i32 | cellX, cellZ |
| 24 | u16 | known: points sampled (the vanilla sampler sends whole cells only: 0xFFFF) |
| 26, 28, 30 | u16 | water, ice, other: points of each kind |
| 32 | u32 | biome: the most common biome of the known points (dictionary 0x30; ties go to the earliest point) |
| 36 | u32 | landBiome: the most common biome of the land points (0: no land) |
| 40 | u32 | tags: tag bits of every known point's biome, together |
| 44 | u32 | landTags: tag bits of landBiome |
| 48, 50, 52 | i16 | surfaceMin, surfaceMax, surfaceMean: top block y over the points |
| 54, 56 | i16 | waterMean, floorMean: mean water top and `OCEAN_FLOOR` top over the water points (0: none) |
| 58 | | reserved |
| 60, 64 | f32 | temperature, downfall: vanilla's biome values (`Biome.getBaseTemperature`, the climate settings' downfall), mean over the points |
| 68 | f32 | celsius: a climate plugin's degrees at the cell's centre when sampled (`ClimateSource`); NaN without one |
| 72, 73 | u8 | season, seasonCount: the plugin's calendar when sampled (IND_CONDITIONS' shape); 0, 0 without seasons |
| 74 | | reserved |
| 80 | | n entries of 8 bytes: u32 body (dictionary 3), u16 share, u16 generated |

**Tag bits** (vanilla `BiomeTags` and NeoForge's common `Tags.Biomes`, checked with
`Holder.is`): 0 `minecraft:is_ocean`, 1 `minecraft:is_deep_ocean`, 2 `minecraft:is_river`, 3
`minecraft:is_beach`, 4 `minecraft:is_mountain`, 5 `minecraft:is_hill`, 6 `minecraft:is_forest`, 7
`minecraft:is_taiga`, 8 `minecraft:is_jungle`, 9 `minecraft:is_savanna`, 10 `minecraft:is_badlands`,
11 `c:is_swamp`, 12 `c:is_desert`, 13 `c:is_plains`, 14 `c:is_snowy`, 15 `c:is_cold`, 16 `c:is_hot`,
17 `c:is_wet`, 18 `c:is_dry`, 19 `c:is_mushroom`, 20 `c:is_underground`, 21 `minecraft:is_overworld`,
22 `minecraft:is_nether`, 23 `minecraft:is_end`, 24 `c:is_sparse_vegetation`, 25
`c:is_dense_vegetation`, 26 `c:is_mountain/peak`, 27 `c:is_mountain/slope`, 28 `c:is_plateau`, 29
`c:is_windswept`, 30 `c:is_stony_shores`; 31 reserved.

**Creature entries.** For each herd species of the core's vanilla adapter (§8.2's five), when either
number is non-zero: **share**, the species' part of its biome's `CREATURE` spawn weight
(`Biome.getMobSettings().getMobs(MobCategory.CREATURE)`), per mille, averaged over the cell's land
points (vanilla plains: sheep 12 of 46, 260); **generated**, the animals of the species vanilla
spawned in the cell at chunk generation this session (NeoForge's `Mob.getSpawnType()` is
`CHUNK_GENERATION`; such entities join with `loadedFromDisk` true, in the call that then posts their
chunk's `ChunkEvent.Load` as a new chunk). The engine keeps the highest count it was ever sent.

### 10.4 When Java sends it

- **Sampled** once all 16 of a cell's chunks are loaded in full (`ServerChunkCache.getChunkNow`),
  at the end of a tick under a budget (8 cells and 0.25 ms a tick, PLACEHOLDERS), from the queue of
  cells whose chunks loaded. A cell whose chunks are not all loaded leaves the queue and comes back
  with its next chunk. A cell is sampled at most once per load of its chunks.
- **Sent** only when its **fingerprint** differs from the one the engine holds (HABITAT_KNOWN), or
  when chunk-generation animals were counted in it. The fingerprint is Java's: FNV-1a 64 over the
  little-endian bytes of eight words (the masks; the two biome names' FNV-1a; the tag bits; the
  heights; the floor; temperature and downfall; the shares), folded to 32 bits by xoring its halves.
  Degrees, the season and the counts are left out: they change without the ground changing. The
  engine stores it and gives it back; it never computes one.
- **Not before** the engine's HABITAT_KNOWN list has ended this session. At a session start every
  chunk already loaded counts as a first load.

### 10.5 HABITAT_KNOWN (0x8900, per session)

| Offset | Type | Name |
|---|---|---|
| 4 (aux) | u32 | flags: 1 END (the last of the list; the record carries nothing else) |
| 16 | u32 | dimension (the engine's dictionary, kind 1) |
| 20, 24 | i32 | regionX, regionZ (512-block regions) |
| 28 | u32 | n |
| 32 | | n entries of 8 bytes: u8 cell index in the region, 3 reserved, u32 fingerprint |

The engine sends one per region it holds, then END, right after every SESSION_START; Java keeps the
fingerprints for the session and samples nothing until END. So a cell crosses the boundary once per
change, and a cell the engine lost (pages older than the world save, pages dropped) is sent again
when its chunks next load. Nothing answers it. The stub engine keeps no habitat: its list is END
alone.

### 10.6 What the engine side does with it

- **rtsim-space's `WorldModel`** is what every module queries, by block rectangle (SCALE.md §2's
  rule: queries, never storage): `area` (points expected and known, land, water, ice, other,
  heights, year-round degrees, vegetation, downfall), `season_offset`, `creatures`. A Deep Time
  provider can replace the sampled one behind the same handle (PLAN.md §11, SCALE.md §12).
- **Unknown ground is avoided** (X2-5): a rectangle with a point nobody sampled is not passable for
  herds and not water for ships; founding waits for it.
- **Herds**: §5 item 14, X2-9 to X2-16. **Ships**: §5 item 12, X2-17.
- **Degrees**: a cell's year-round °C is the plugin's reading less the season it was taken in, else
  vanilla's temperature on mic-climate's scale (-0.5 to 2.0 over -20 to +56 °C, X2-8); a dimension
  where a plugin reported seasons adds mic-climate's default season offsets by sub-season.


## 11. The world's topology: TOPOLOGY (DECISIONS.md C31; docs/choices-topology.md)

How each level's edges are joined: flat (vanilla, with the world border), an x wrap (Toroidal World
alone) or a glued domain (Deep Time's Petroff–Guyou layout: east and west slide by 2b, north and
south are half-turns about (0, ∓b/2), with an edge_stop border on north and south until the wrap
engine joins them). The engine measures every distance, leg, query and key through it.

### 11.1 Kind ranges (reserved)

| Range | Direction | Class |
|---|---|---|
| 0x00B0 to 0x00BF | Java to engine | topology latest-wins observations |

### 11.2 TOPOLOGY (0x00B0, latest wins, aux = dimension)

Reference codec `crates/rtsim-space/src/topology/record.rs`, mirrored by the core's
`topology/TopologyRecords.java`; the bytes of the 16k Guyou record are pinned on both sides.

| Offset | Type | Field |
|---|---|---|
| 8 | u8 | kind: 0 flat, 1 x wrap, 2 glued |
| 9 | u8 | n, the edges that follow (at most 4) |
| 10 | u8 | flags: 1 the rectangle below is set |
| 11 | u8 | source: 0 the bridge's default (world border), 1 Toroidal World, 2 Deep Time's layout, 3 a plugin |
| 12 | u32 | reserved |
| 16, 24, 32, 40 | i64 | x0, z0, x1, z1. Flat: the world border. X wrap: the lap [x0, x1) and the border's z (the whole i32 range when there is none). Glued: the domain, x in [x0, x1), z in [z0, z1] |
| 48 | | n edges of 24 bytes: 0 u8 side (0 west, 1 east, 2 north, 3 south), 1 u8 quarter turns k, 2 u8 flags (1 stopped by edge_stop), 3 to 7 reserved, 8 i64 tx, 16 i64 tz |

Size 48 + 24n. An edge's motion takes what lies beyond it back inside, as Deep Time's wrap engine's
`CellIsometry(k, tx, tz)`: k clockwise quarter-turns seen from above, then the translation; on block
cells k = 2 is (x, z) to (−x − 1 + tx, −z − 1 + tz), on points (−x + tx, −z + tz). These are the
manifest's `projection.edges[].iso` (FORMAT.md "The world's layout").

### 11.3 When Java sends it

- For every level at the session's first hand-over, as a **sticky preamble record**: written right
  after the dictionary and MODULES, so before any journal entry the hand-over replays
  (`BoundarySession.preamble`). Never dropped for space: a preamble that does not fit waits for the
  next hand-over, with everything after it.
- Again when a level's topology changes: the bridge looks every 200 ticks (PLACEHOLDER) and at once
  after `RtsimApi.topologyChanged()`.
- Detection order (TP18): a registered `TopologySource` (Deep Time's layout, TP19), then Toroidal
  World's shape, then the world border.

### 11.4 What the engine side does with it

- **One value, shared** (`engine/topology.rs`): the engine keeps every frame's topology as one
  `Arc<Topologies>`, logs each change, and hands it to every module at each step (a pointer
  comparison when nothing changed). Not saved: Java sends it again at every session start (TP6). A
  frame never described is flat, exactly the behaviour before this record (TP1).
- **Canonical at the door** (TP4): positions Java reports are folded into the domain on entry;
  HABITAT cells past a slid edge are re-keyed, past a turned edge dropped (TP17).
- **Commands carry chart positions** (TP11): a spawn is at the image of the agent nearest the player,
  and a target or waypoint continues from the body's position, so either can lie a little past an
  edge. Java takes every target as its image nearest the body each time it steers.
- **Edge_stop** (TP12): nothing is materialized across a stopped seam, targets go round it, and
  bodies are dematerialized before they would cross it.

## 12. The far view: FAR_LEG and the far feed (research/distant-horizons-2026-10-21.md §6; docs/choices-dh-feed.md)

What moves far from a player, for a client with a far renderer (`mic_rtsim_dh` on Distant Horizons). The
engine says what each dragon is doing (FAR_LEG, §12.1), the server gathers that with what moves in the game
(Ice and Fire's embodied dragons, Create's trains, §12.3), and sends each subscribed client once a second
what changed in its view (§12.2). Choices are DHT25 and up in `docs/choices-dh-feed.md`.

### 12.1 Kind ranges (reserved) and FAR_LEG

| Range | Direction | Class |
|---|---|---|
| 0x00D0 to 0x00DF | Java to engine | far view latest-wins observations (none yet) |
| 0x0C00 to 0x0CFF | Java to engine | far view journaled observations (none yet) |
| 0x8C00 to 0x8CFF | engine to Java | far view commands (`cmdSeq` at 8) |

0x00C0 to 0x00CF, 0x0A00 to 0x0AFF and 0x8A00 to 0x8AFF are TALK's (§13, TALK.md §9, TK10); 0x0B00 to
0x0BFF and 0x8B00 to 0x8BFF stay free for the next simulation families (DHT26).

**FAR_LEG (0x8C00, aux = state, size 80).** Reference codec `crates/rtsim-ecology/src/far.rs`, read by
the core's `far/FarLegs.java`; the bytes of one record are pinned on both sides (`far::tests` and
`FarLegsTest`).

| Offset | Type | Field |
|---|---|---|
| 16 | u64 | agent (the dragon's agent id, as DRAGON_MATERIALIZE's) |
| 24 | u32 | body: the species' entity type (dictionary 3) |
| 28 | u32 | dimension (dictionary 1) |
| 32 | u8 | kind: 1 dragon (the core's `FarKind`) |
| 33 | u8 | variant: the colour, 0 to 3 within the species |
| 34 | u8 | stage, 1 to 5, at the record's time |
| 35 | u8 | reserved |
| 36 | i32 | y: the dragon's feet, as its entity's y (the far feed's LEG draws its middle, §12.2) |
| 40, 44, 48, 52 | i32 | x0, z0, x1, z1 |
| 56, 64 | i64 | t0, t1, game time |
| 72 | u32 | radius: a hunt's circle in blocks (at least 24, PLACEHOLDER); 0 otherwise |
| 76 | u32 | reserved |

| aux | State | Meaning |
|---|---|---|
| 0 | GONE | the dragon no longer exists (died, or its row was removed): forget it; only the agent is set |
| 1 | FLY | flying a straight leg from (x0, y, z0) at t0 to (x1, y, z1) at t1 |
| 2 | HUNT | hunting around (x0, z0) at height y until t1, drawn circling at `radius` |
| 3 | ROOST | at rest at (x0, y, z0): its lair, or where it settled; x1 = x0, z1 = z0 |
| 4 | BODY | embodied (or leaving its body): the entity is the truth; (x0, y, z0) is where the engine last had it |

- **When the engine sends it** (DHT27): every dragon at a session start, in one bounded pass over the
  rows (as HABITAT_KNOWN's resend, §10.5), then one record whenever what a dragon does changes: a leg
  booked or cancelled, a state tracked, a dragon created or removed. Nothing passes over every dragon on
  a step. An engine whose far feed is off marks nothing; rtsim-jni turns it on at every session start.
- **Positions are chart positions** (TP11): an end may lie past a seam. The link turns engine ticks into
  game time when it writes the record.
- **What Java does with it**: the session keeps the last record per dragon (`RtsimSession.farLegs`, not
  saved) and the far feed draws it. A BODY record is never drawn: the body's own source gives the same id
  at a higher priority (§12.3). A record whose dimension the dictionary does not name drops the dragon;
  a short record or an unknown state counts as malformed. Switching the ecology module off clears the
  table and drops any record still on its way (the engine drops its own queue, GONE records included);
  the engine resends every dragon when the module comes back. A changed dragon whose page will not load
  is skipped and counted (`far_read_failed`): Java draws its last leg until it next changes.

### 12.2 The payloads and the far feed's message

Three NeoForge payloads, all registered optional (`PayloadRegistrar.optional()`), so a client without them
joins a server with them and the other way round. None goes through the engine.

- **`mic_rtsim:far_sub`**, client to server: five varints, `version` (2), `radius` (blocks, at most
  16,384, PLACEHOLDER), `tier` (0 OFF, 1 LOW, 2 MEDIUM, 3 HIGH), `budget` (objects, capped by the tier at
  0, 48, 128 and 320, PLACEHOLDER) and `kinds` (a mask of `1 << kind`: 2 dragons, 4 carriages). Radius,
  budget or kinds 0 unsubscribes. A subscription of another version is ignored. A client sends it only
  when the server has the channel, at most once a second, and only when it changed.
- **`mic_rtsim:far`**, server to client: one byte array (at most 1 MiB), a far feed message, once a second
  while subscribed and something changed (the first one always).
- **`mic_rtsim:far_shape`**, server to client: `i64 hash` (big-endian, as `FriendlyByteBuf.writeLong`) and
  one byte array (at most 32 KiB, PLACEHOLDER): a far shape, what an object looks like (DHT61, DHT62).

The message (`far/FarWire.java`; unsigned LEB128 varints, `zig` a zigzag varint):

```
u8 version (2), varint flags (1 RESET), varlong gameTime, string dimension, zig ox, oy, oz, varint count, records
INTRO  1: handle, i64 id, u8 kind, u8 style, u8 variant, u8 size, varint rgb, i64 shape, zig age
LEG    2: handle, u8 state, zig y-oy, zig x0-ox, zig z0-oz, zig x1-ox, zig z1-oz, zig t0-gameTime, varlong t1-t0, varint radius
SAMPLE 3: handle, u8 state, zig lx-8ox, zig ly-8oy, zig lz-8oz, zig tx-lx, zig ty-ly, zig tz-lz
REAL   4: handle, zig entity (-1: none)
GONE   5: handle
```

- **Looks** (version 2, DHT60): an INTRO's `shape` is the content hash of what the object looks like (0:
  none, drawn as a box), its `age` the object's age in days (a dragon's, for its size between stages; -1:
  the source does not say, as the engine's legs do not). `style`, `variant` and `size` are a dragon's
  species, colour and stage.
- **Handles.** The server gives each object a small handle per subscriber in its INTRO; later records
  use it. An object the filter no longer chooses is held for two sweeps (PLACEHOLDER), then GONE. GONE
  frees the handle, and GONE records come first in a message so their handles can be reused in it. The
  hold covers the hand-over's gap: a dragon whose body was just captured is in neither source until the
  engine's next leg lands, and comes back on its own handle.
- **Positions** are relative to the header's origin (the subscriber's block): legs in whole blocks,
  samples in eighths of a block with the trailing point relative to the leading one. Times are relative
  to the header's game time. A position is the object's middle, as a body source samples it: a LEG's y
  is FAR_LEG's (the feet) plus the species' centre for the stage, which its plugin registers
  (`FarLegs.Style`; DHT42).
- **Only what changed** (`far/FarView.java`): an INTRO when an object is new or its looks changed, a LEG
  when the leg changed, a SAMPLE when it moved by an eighth of a block, and the same SAMPLE once more the
  first sweep it did not (it stopped: the client stops running it on, DHT44), a REAL when its entity id
  changed. RESET tells the client to forget every object first. The server sends it on every subscription
  (a client subscribes again when its wants change or a message did not decode) and on a new dimension,
  and then everything the filter chose.
- **A body joining a level** (`FarFeed.embodied`, through `RtsimApi.farEntityJoining`): as a dragon's body
  or a carriage's entity joins a server level, each subscriber holding that object gets a message of its
  own at once (flags 0, one REAL), ahead of the entity's spawn packet, so the client hides its far object
  the frame the entity arrives rather than at the next sweep (DHT40). The sweep's item then carries the
  same id and sends nothing more.
- **On the client** (`client/FarFeedReceiver.java`): legs become far registry movers on the far clock,
  timed through the client's game time; a hunt circles at 0.5 blocks a tick (PLACEHOLDER), spiralling
  onto its circle over 40 ticks from where the dragon was drawn and turning the way it was flying. A
  sampled mover is run on (DHT44): from where it is drawn, it heads for where its last two samples say
  it will be when the next one comes, and 10 ticks on past that (PLACEHOLDER) should the next be late,
  then waits; one that stopped or brakes to a stop stops there. A message that does not decode drops
  every object, and the client subscribes again, so the server's RESET brings them back. A dragon whose
  INTRO says age -1 keeps the age an earlier INTRO of the same object gave (its body's, handed back to
  its leg).

**Shapes** (`far/FarShape.java`, DHT61, DHT62). A shape is a kind's raw layout, deflated (raw DEFLATE, no
zlib header); its hash is over the raw bytes: FNV-1a 64, then `h ^= h >>> 33; h *= 0xff51afd7ed558ccd;
h ^= h >>> 33`, and 0 becomes 1. The server sends each shape to a subscriber once, after the message whose
INTRO first names it, nearest object first and at most 64 KiB a sweep (PLACEHOLDER; the rest go the next
sweep; until they come the client draws nothing for the object for a short grace, about 2 s, then its box,
DHT63). A RESET starts over on both sides: the server sends the
shapes again after it, and the client forgets the ones it had when it reads it. Past 1,024 shapes sent
(PLACEHOLDER) the server starts the subscriber's view over with a RESET, so neither side's set grows for a
whole session. A client inflates each shape as it arrives (at most 1 MiB raw), checks the hash and the
layout, and drops one that fails. A carriage's raw layout (`far/CarriageShape.java`, version 1, plain bytes as the message's):

```
u8 version (1), u8 assembly (Direction 3D data value: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east),
varint palette size (at most 4,096), palette: varint global block state id each,
zig min x, y, z, varint size x, y, z (1 to 1,024 each), varint block count (at most 65,536),
per block, in (y, z, x) order: varint gap in the box's linear index ((y * sz + z) * sx + x) since the
  last block (the first from -1), varint palette index,
u8 bogey count (at most 4), per bogey: zig x, y, z, string style id, string size id (each at most 256
  bytes), u8 upside down (0 or 1)
```

Coordinates are the contraption's own (Create's `Contraption.Blocks`, relative to its anchor, the leading
bogey). The block state ids are the global ids chunk packets use, so they hold between one server and the
clients joined to it. Block entity data, actors, storage, seats and glue are left out.

### 12.3 Sources and the filter

A `FarSource` (`RtsimApi.registerFarSource`) says what moves in a level; the feed asks each once a second
for each level a subscriber is in, and none when nobody is subscribed.

| Source | Ids | What it gives |
|---|---|---|
| the engine's legs (core, priority 0) | the dragon's agent id | FLY, HUNT and ROOST legs from FAR_LEG |
| Ice and Fire's bodies (`mic_rtsim_iceandfire`, priority 10) | the dragon's agent id | each puppet's body as a sample, FLY or ROOST, with its entity's network id and its age in days |
| Create's trains (`mic_rtsim_create`, priority 0) | `0x54 << 56`, 48 bits of the train's id, the carriage index | every carriage's leading and trailing points, read as Create's map sync reads them, with its entity's network id while the server has one, and its shape (its blocks and bogeys, DHT61) |

- **One item per id**: the highest priority wins, so a dragon's body replaces its leg on the same handle
  and gives it back when it goes (the hand-over, research note §5).
- **The image nearest the subscriber**: through a seam, each item is moved to its image nearest the
  subscriber before it is filtered (§11).
- **The filter** (`far/FarFilter.java`): the kinds the subscriber draws, within its radius plus 64 blocks
  at this game time, dragons first and then nearest first, up to its budget.
- **The entity id** lets a client hide the proxy while it has the real entity near it; a carriage
  between the server's simulation distance and the client's render distance has no entity, and the
  client draws it there itself (DHT33, DHT45). The REAL goes out as the entity joins (§12.2), not only at
  the sweep.

## 13. Talking to NPCs: TALK_ASK, TALK_FACTS and TALKED (TALK.md §9; docs/choices-talk.md TK10, TK32 to TK59, TK60 to TK70)

What a talker tells a player who crouches and clicks on it. Java asks the engine what the holder
(a village, a caravan, later a camp or a ship) knows; the engine answers with the holder's mood, its
market's shortest and fullest goods, its ports' state and up to four pieces of news, each naming who
did what to whom and where. TALK.md §9 holds the reasons and the lines Java builds from them; this
section is the wire. A ribbit's credited talk goes the other way, as TALKED (§13.7).

Reference codec `crates/rtsim-jni/src/talk_wire.rs` (every record's bytes are pinned by its tests).
The Java codec is `talk/TalkRecords.java`, pure (no Minecraft types), and `TalkRecordsTest` pins the
same bytes as the Rust tests. The engine side is `engine/talk.rs`; the Java side is §13.6.

### 13.1 Kind ranges

| Range | Direction | Class |
|---|---|---|
| 0x00C0 to 0x00CF | Java to engine | TALK latest-wins observations: TALK_ASK (0x00C0) |
| 0x0A00 to 0x0AFF | Java to engine | TALK journaled observations: TALKED (0x0A00, §13.7) |
| 0x8A00 to 0x8AFF | engine to Java | TALK commands (`cmdSeq` at 8): TALK_FACTS (0x8A00) |

0x0B00 to 0x0BFF and 0x8B00 to 0x8BFF stay free.

### 13.2 TALK_ASK (0x00C0, latest wins, aux = n)

Java's open questions, sent each tick until answered (dropped and sent again when a buffer is full,
as every latest-wins kind is). Size 8 + 56n; each ask starts at 8 + 56i:

| Offset | Type | Field |
|---|---|---|
| 0 | u64 | askId (Java's, per session, never 0) |
| 8, 16 | u64 | playerMost, playerLeast |
| 24 | u32 | holder kind: 1 village (Java's village id), 2 caravan (a puppet's agent), 3 faction (the camp's key), 4 ship (an agent) |
| 28 | u32 | dimension (dictionary 1); the engine's frame is its id, so one above 65,535 is refused |
| 32 | u64 | holder |
| 40, 44 | i32 | the talker's x, z |
| 48 | u32 | flags: 1 news, 2 faction facts |
| 52 | u32 | reserved |

### 13.3 TALK_FACTS (0x8A00, aux = news n, at most 4; per session)

The answer to one ask, one tick or more later. No op id, never kept, never resent after a restart:
its sequence is the session's (`session_seq`), and Java drops an ask's wait when it lands or after
20 ticks (TK5). Size 48 + 120n:

| Offset | Type | Field |
|---|---|---|
| 8 | u64 | cmdSeq |
| 16 | u64 | askId |
| 24 | u32 | flags: 1 HOLDER (the engine knows the holder), 2 PORT_CLOSED, 4 BLOCKADED, 8 FACTION (a faction block follows; slice 4) |
| 28 | i32 | the holder's sentiment toward the player (rtsim-knowledge's scale; 0 for a player it has never met) |
| 32 | u32 | scarcest good (dictionary 2; 0 none) |
| 36 | u32 | its stock per target, thousandths |
| 40 | u32 | most glutted good (0 none) |
| 44 | u32 | its stock per target, thousandths |
| 48 | n x 120 | news, best first |

Without HOLDER everything after the flags is zero and n is 0 (TK32, TK38). News comes only when the
ask's flag 1 is set; PORT_CLOSED and BLOCKADED are facts about the holder and come either way.

A **news item** (120 bytes, offsets from its start): 0 u16 report kind, 2 u8 severity, 3 u8 the
report's flags, 4 u32 talk flags (1 YOU_DID, 2 TO_YOU, 4 TO_US, 8 BY_US, 16 SEEN), 8 u32 ticks ago
(saturating), 12 i32 dx, 16 i32 dz (the place from the talker, the short way round a seam, in the talker's own chart,
TK47; 0 and 0 across frames, TK34), 20 reserved, 24 ref who, 56 ref whom, 88 ref near (the village nearest the
place, TK35).

A **ref** (32 bytes): 0 u8 class, 1 u8 flags (1 DEAD, 2 POS), 2 reserved, 4 u32 what (dictionary 5
culture for villages, camps and chiefs; dictionary 3 body for dragons and lairs), 8 u64 a, 16 u64 b,
24 i32 dx, 28 i32 dz (where it is from the talker, the short way round, in the talker's chart, when POS). Classes: 0
unknown, 1 player (a, b the UUID), 2 village (a Java's village id), 3 player base, 4 caravan (a its
seed, b its home village), 5 camp (a the faction key), 6 chief or notable (a its seed, b its camp),
7 dragon (a its seed, b its lair key), 8 lair (a its key), 9 haven, 10 ship. How each is found, and
when one is class 0 or DEAD, is TK36 and TK37.

### 13.4 What the engine side does with them

- **Observe:** each ask with a new askId is queued (at most 256 wait; the last 1,024 answered ids are
  remembered; all cleared at SESSION_START, TK39). One that is already answered or waiting is
  ignored when it comes again. An askId of 0, a dimension no frame can be and a record that does
  not decode are refused and counted.
- **Write point:** after the step's simulate, after the world model's commands, so the answer reads
  the world as it stands at this hand-over (TK43). Each answer's dictionary names go out first; a
  buffer that cannot hold the next answer keeps its ask for the next one.
- **Read only:** it changes nothing in the world. With phase 1 off an ask is answered at once
  without HOLDER (TK32).
- **TALKED** is the one record that does change it, and is journaled: §13.7.

### 13.5 Test order 40: `TalkReport`

TEST mode only (rtsim-host's fake Java keeps the TALK_FACTS records it is sent, `Java::talk_facts`,
since slice 3, and the conflict and ports engines do not record news yet). Posts one report on a village's board: village u64, kind u16,
severity u8, flags u8, who and whom as (class u8, a u64, b u64) in §13.3's classes, place x i32 and
z i32 (each within the pack's range, -8,388,608 to 8,388,607), ago u32 ticks. Refusals, and what
it does to the knowledge tables, are TK40.

### 13.6 What the Java side does with them

- **The plugin.** `talk/TalkAsks` is a `SessionPlugin` that owns 0x00C0 to 0x00CF and 0x8A00 to 0x8AFF
  (`TalkRecords.isTalkCommand`). A talker's click opens an ask with a Java id (per session, from 1);
  at the end of every tick all open asks go out as one TALK_ASK through `boundary().direct` (the
  dimension is defined first, `dimId` before `direct`; a full buffer drops the record and the next tick
  writes it again). An ask closes on its TALK_FACTS, after 20 ticks, or when the session starts, stops or
  fails; its callback runs once, with the answer or with null. At most 32 are open. On the pure-Java stub,
  with no session or a failed one, no ask opens and the talker speaks at once (TK49).
- **The answer.** `accept` decodes the record (a malformed one is counted and dropped, an answer for an
  ask already closed counts as late) and resolves the dictionary names from the session's `EngineNames`
  once, by the ref's class: dictionary 5 for a village, camp or chief (the culture), dictionary 3 for a
  dragon, lair or ship (the body entity id), dictionary 2 for the goods. The talk reads it as a
  `TalkAnswer`; a talker's candidates see only `api/TalkFacts` (no wire types).
- **Holders.** A villager (not a child, TK16) of an adopted village asks as holder kind 1 with Java's village id; a wandering
  trader that is a live `human_caravan` group's member as kind 2 with the group's agent. Kinds 3 and 4 are
  not asked by Java yet (TK38).
- **Test hooks.** `TalkReport` is `NativeEngine.talkReport(...)` (`NativeCodec.Orders.talkReport`), with
  `NativeEngine.TalkActor` for the (class, a, b) of an actor. `TalkAsks.mute` writes no ask, so the 20
  ticks' timeout closes it. `Refs.force` forces names or descriptions (TK52). The GameTests are
  `gametest/TalkNewsGameTests`, each in a batch of its own because of the process-wide hooks.
- **Nothing is written back by an ask.** An ask changes nothing in the world; a villager's daily credit
  stays where slice 1 put it (TK1, vanilla gossip), and the engine hears of a talk only with TALKED
  (§13.7), which the Ribbits plugin sends for a credited talk with a resident ribbit (TK60).

### 13.7 TALKED (0x0A00, journaled, aux 0): a ribbit's talk credits its village

A player's first talk of a game day with a resident of a ribbit-culture village (TK60, TK70). The village's feeling
toward the player rises by `TALK_CREDIT`, 3 (TK2). Not a report: nothing happened that news could carry.

| Offset | Size | Field |
|---|---|---|
| 0 | u16 | kind 0x0A00 |
| 2 | u16 | size, 64 |
| 4 | u16 | aux, 0 |
| 6 | u16 | reserved |
| 8 | u64 | obsSeq |
| 16 | i64 | game time |
| 24 | u64 | op id: 63 random bits with `Obs.JAVA_OP_BIT` set, drawn by the journal's `commitWithOpId` |
| 32 | u64 | the village's Java id |
| 40 | u64 | the player's UUID, most significant bits |
| 48 | u64 | the player's UUID, least significant bits |
| 56 | u32 | the talker's body: its entity type's name in dictionary 3 (`ribbits:ribbit`); the engine does not read it |
| 60 | u32 | reserved, 0 |

Pinned by `talk_wire::tests::the_talked_bytes_are_pinned` and Java's `TalkRecordsTest.theTalkedBytesArePinned`,
the same 64 bytes for obsSeq 0x1234, game time 123456, op id 0x0A0B0C0D0E0F1011, village 0x0102030405060708
and player (0x1111222233334444, 0x5555666677778888); `TalkRecordsTest` also builds it at the literal offsets above.

**Exactly once.** Three layers, none of them new:

1. Java sends at most one per village, player and game day (TK60), and writes that day on the player before it
   journals; a talk with no session, a failed one or phase 1 off writes nothing (TK66).
2. The kernel's `Replay` admits each journaled observation once, by its sequence and by its op id (the
   `op_of` hook for TALKED returns the op id at 24): a record the engine has already seen, which a resent
   hand-over or a replay after a crash brings again, is `AlreadyApplied`; a copy under a new sequence with
   the same op id (`recommit`) is `DuplicateOp`. Phase 1 never sees either.
3. What `Replay` admits becomes the phase 1 input `Talked { key, player }` (persist tag 21), appended to the
   input log like every other input, so a crash replays it onto the same world once and the +3 is not
   applied twice.

**The engine side** (`engine/phase1.rs` `talked`, `engine/talk.rs`). It adds `TALK_CREDIT` to the village's
feeling toward the player through `Knowledge::adjust`, and makes the player's actor if the village has
never met them, as a trade does (a credited talk is a real meeting, unlike an ask, TK45). A TALKED that does
not decode, names a village the world does not know or a base, or is by the nil player is refused and
counted (`talked_refused`, warned in the log at each power of two) and never applied and never put in the input log (TK68). With phase 1
off it is counted ignored, like every other kind. A scripted village (a test's) is left alone. Applied
ones count in `talked`. Nothing is answered: the next TALK_ASK for that village reads the new feeling in
TALK_FACTS.

**The Java side** is `RibbitTalker.credit` in the Ribbits plugin: one `beginJournaled` and `commitWithOpId`
of 64 bytes, after the day is written on the player.

Tests: `rtsim-world` (the input applies, makes the player, is refused for a village that is not one,
replays to the same world), `rtsim-jni` (`engine/talk.rs`: the credit shows in the next ask's sentiment,
refusals, a scripted village), `tests/protocol.rs` `mod talk` (a talk credits live and across a crash;
a malformed one is refused and the next still applies), and the GameTests `ribbit_talk_credits_the_village_once_a_day`
and `ribbit_puppet_talks_without_credit` on the Ribbits jar.
