# The Java/engine boundary

**Status:** the contract between the core bridge `mic_rtsim` (Java) and the engine (Rust,
`crates/rtsim-jni`, a mic_native plugin). Written 2026-10-20 with the Java side and its pure-Java
stand-in, `StubEngine`; the Rust side followed the same night (§4). Both sides mirror the layout:
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
**Command flags:** 1 `MORE` (the engine holds more commands in its overflow ring for later steps).

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
  kinds and member bodies share it). Ids start at 1.
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
  transfer after a crash it must give it the same op id, or the bridge cannot tell it is a duplicate.

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
- COMMAND_RESULT status: 1 applied, 2 duplicate op id (already applied, skipped), 3 failed.

**SESSION_START and the kernel's `Replay::reconcile`.** `journalFirstObsSeq` and `worldNextObsSeq`
are the two numbers `reconcile(journal_first, world_next)` takes. `worldNextObsSeq` is the world
save's own next sequence, read before Java moves past `pagesObsSeq`, so a world save older than the
pages shows up as `EngineAhead` (P29). Java's sequences in this session then start at
`max(worldNextObsSeq, pagesObsSeq + 1)`, which keeps `Replay::admit` contiguous.

**WORLD_SAVED and `Replay::forget_ops_through`.** The bridge records `nextObsSeq - 1` at the
overworld's save event and chains a marker onto NeoForge's SavedData write chain
(`IOUtilities.withIOWorker`), so the marker runs after the journal's file is written. The next
hand-over carries the value; the engine may then forget op ids applied at or before it.

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
at or below the journal's `lastAppliedCmdSeq` (sent back in every observation header as `wordA`, which is the
kernel's `Outbox::ack_through`). After a restart the engine resends its outbox (`Outbox::after`)
with the commands' original sequences; Java applies those above `lastAppliedCmdSeq` and skips any
whose op id it already applied. New commands must get sequences above `lastAppliedCmdSeq` too, so
the engine starts its counter at `max(next_seq, lastAppliedCmdSeq + 1)` (`start` returns it). Bodies
start at 16.

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8001 | DICT | dict kind | 16 u32 id, 20 u32 nameLen, 24 UTF-8 name | 24 + name, aligned | |
| 0x8002 | SPAWN_GROUP | member count | 16 u64 agent, 24 u32 epoch, 28 u32 groupKind, 32 u32 dimension, 36 u32 flags (0), 40 f64 x, 48 f64 y, 56 f64 z, 64 member blocks | 64 + blocks | MATERIALIZED or SPAWN_REJECTED |
| 0x8003 | DESPAWN_GROUP | 0 | 16 u64 agent, 24 u32 epoch, 28 u32 reason | 32 | DEMATERIALIZED |
| 0x8004 | STEER_GROUP | 0 | 16 u64 agent, 24 u32 epoch, 28 f32 speed, 32 f64 x, 40 f64 y, 48 f64 z | 56 | |
| 0x8005 | TRANSFER_TO_CONTAINER | 0 | 16 u64 opId, 24 u32 dimension, 28 u32 item, 32 i32 x, 36 i32 y, 40 i32 z, 44 i32 count | 48 | COMMAND_RESULT |

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

**Still open:** a crash that leaves the engine's pages ahead of the world save (Proposal P29), and a
journal write that fails (`SavedData.save` logs the IOException and the IO chain goes on, so
WORLD_SAVED still reports it).

## 4. The engine side: rtsim-jni

`crates/rtsim-jni` is a mic_native plugin (a cdylib linking `mic-native-rt`) and the only crate that
uses `jni`; the bridge's `engine/NativeEngine.java` loads it through `MicNative.loadPlugin` and binds
its natives with `System.load`. Choices are in `docs/overnight-choices-jni.md` (J1 to J28).

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
6. **Commands**: transfers go through the `Outbox` and are resent after a restart with their
   original sequences and op ids, before anything new; puppet commands are per session. DICT
   records come before their first use in the same buffer. What does not fit waits (`MORE`).
7. **Op ids**: Java's have the top bit set; the engine's leave it clear and start at 1.
