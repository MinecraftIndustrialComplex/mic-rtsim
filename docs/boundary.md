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
  (`ribbits:houses/small_house_red_1`), 8 processor list (`ribbits:main`). Ids start at 1.
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
| 0x0020 to 0x0021, 0x0120 to 0x0126 | phase 1 | | §4.1 | |

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

**Still open:** a crash that leaves the engine's pages ahead of the world save (Proposal P29), and a
journal write that fails (`SavedData.save` logs the IOException and the IO chain goes on, so
WORLD_SAVED still reports it).

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
  loaded chunks only; puppets are never residents.

### 4.2 Commands

| Kind | Name | aux | Fields | Size | Answered by |
|---|---|---|---|---|---|
| 0x8010 | MARKET | goods n | 16 u64 village, 24 u64 asOfObsSeq, 32 u32 currency (item), 36 u32 reserved, 40 i64 treasury, 48 i64 treasuryTarget (Mbar; 0 never refuses), 56 i32 worstNum, 60 i32 worstDen (rtsim-trade `markup_for`'s worst), 64 n × good | 64 + 40n | |
| 0x8011 | MERCHANT_OFFERS | offers n | 16 u64 agent, 24 u32 epoch, 28 u32 member, 32 n × offer | 32 + 40n | |
| 0x8012 | WRITE_GOSSIP | 0 | 16 u64 opId, 24 u64 village, 32 u64 playerMost, 40 u64 playerLeast, 48 u32 gossip type, 52 i32 amount, 56 u32 report kind, 60 u32 flags (1 FIRST_HAND) | 64 | COMMAND_RESULT |
| 0x8013 | SENTIMENT | entries n | 16 u64 holder (village), 24 i32 dislikeAtOrBelow, 28 i32 friendAtOrAbove, 32 u32 flags (1 REPLACE), 36 reserved, 40 n × (u64 playerMost, u64 playerLeast, i32 sentiment, u32 reserved) | 40 + 24n | |
| 0x8014 | PLACE_HOUSE | 0 | 16 u64 opId, 24 u64 village, 32 u32 dimension, 36 u32 template, 40 u32 processors (0 none), 44 i32 x, 48 i32 y, 52 i32 z, 56 u8 rotation, 57 u8 mirror, 58 u16 flags (1 SEARCH, 2 Y_FROM_VILLAGE), 60 u32 radius, 64 u64 seed, 72 i32 yOffset, 76 reserved | 80 | HOUSE_PLACED, then COMMAND_RESULT |
| 0x8015 | SPAWN_RESIDENT | 0 | 16 u64 opId, 24 u64 village, 32 u32 dimension, 36 u32 body (entity type), 40 u32 profession (0 the body's default), 44 u32 flags (1 AT_HOUSE), 48 f64 x, 56 f64 y, 64 f64 z, 72 u64 house (the PLACE_HOUSE op id) | 80 | COMMAND_RESULT |

**MARKET good** (40 bytes): 0 u32 item, 4 u32 reserved, 8 i64 stock, 16 i64 target (Sbar), 24 i64 anchor
(p*), 32 i32 rNum, 36 i32 rDen. Glut (more than twice the target) and cash refusal (treasury above
twice its target) follow from these numbers, as rtsim-trade's `level_of` decides them.

**MERCHANT_OFFERS offer** (40 bytes): 0 u32 costA item, 4 i32 costA count, 8 u32 costB item (0 none),
12 i32 costB count, 16 u32 result item, 20 i32 result count, 24 i32 maxUses (0 = 64, PLACEHOLDER),
28 i32 xp, 32 f32 priceMultiplier, 36 u32 flags (1 the merchant sells, 2 the merchant buys: TRADE's
side). Counts must fit one stack; a bad offer is skipped.

Gossip type: 1 major_negative, 2 minor_negative, 3 minor_positive, 4 major_positive, 5 trading
(rtsim-knowledge `GossipType`). Report kind: rtsim-knowledge `ReportKind` (1 assault ... 8 resident
killing).

### 4.3 How Java applies them

- **MARKET** (C13, C14, C23; latest wins). The bridge keeps the snapshot per village and prices residents
  from it on the server thread (PLAN.md §8.5): before a player starts trading (the interaction event,
  before vanilla's `updateSpecialPrices`, so reputation still applies on top), after every trade, and
  uses only at a restock. A **market-shaped** offer (one cost and one result, one of them the currency,
  the other a plain good the market trades) is repriced to rtsim-trade's `Market::offer` bundle (the
  Java `MarketBook` mirrors it and its unit tests use the Rust vectors), in place, one for one: the sim
  never adds or removes a resident's offers. Uses = min(vanilla max uses, stock / items for what the
  village sells, treasury / coins for what it pays). Vanilla's demand is held at 0 on these offers.
  Ribbit villages (no gossip) apply `markup_for(sentiment, worst)` from the SENTIMENT snapshot; vanilla
  villagers get none (their reputation pricing carries C20). A bundle that does not fit one stack is
  clamped only where that is worse for the player, else the offer shows crossed out.
  **asOfObsSeq**: every observation at or below it is in the snapshot. The bridge keeps each of its own
  trades under its TRADE's sequence and replays those above it on top of a new snapshot, so a snapshot
  the engine computed before a trade never gives the money back.
- **MERCHANT_OFFERS** (C21; latest wins). Replaces a puppet merchant's offers (vanilla's random wandering
  trader trades and Ribbits' rolled merchant trades are dropped at spawn). Every result comes out of the
  member's trade cargo (a wandering trader's: its group's llama chests; a ribbit merchant's: its pack)
  and every payment goes into it; uses = min(maxUses, cargo of the result / result count, room for each
  payment / its count). Ignored for an agent or epoch that is not embodied.
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
  being built) or at (x, y, z), once its chunk is entity-ticking, under the spawn quota. Residents are
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
   0x4900 to 0x4902 and page 1:0x49.

10. **Phase 1** (`engine/phase1.rs`, choices V1 to V28 in docs/overnight-choices-phase1jni.md): the
   phase 1 world (`rtsim-world`: settle, trade, travel, knowledge) runs inside the step.
   - **Observations.** VILLAGE_ADOPTED joins a village (a no-op for one it has); TRADE (a resident's
     at its village's market, a puppet's at its caravan) and CARGO_CHANGED are booked as the counts
     the bridge reports, once by op id; HURT and RESCUE become reports of the caravan or the village
     (a resident's first-hand hurt is still a report: rtsim-knowledge never writes back what vanilla
     saw itself); TAKEN and DIED of a phase 1 puppet take or kill its member; BASE marks and unmarks
     player-base nodes; HOUSE_PLACED lets a grown house's residents move in; the first
     VILLAGE_CENSUS sets a village's population and later ones only raise it; VILLAGE_GOSSIP is read
     as latest state.
   - **Commands.** MARKET (`asOfObsSeq` the highest observation the step applied) for villages near
     a player or just traded at, when they changed; MERCHANT_OFFERS when a caravan materializes;
     SENTIMENT for ribbit villages whose feelings may have changed; WRITE_GOSSIP from
     rtsim-knowledge's vanilla gossip (amounts below 2 raised to 2); PLACE_HOUSE then SPAWN_RESIDENT
     for growth. The last three carry op ids derived from what they are (§2.2) and are kept and
     resent like transfers. Caravans and touring bands a player can see are materialized as C25
     groups (SPAWN_GROUP with `FLAG_SURFACE`, cargo in the chests), steered toward where the sim
     walks them, and captured back when no player is near or the sim's caravan moved on.
   - **Pages.** Table 0x5000: a meta page and the world's input log in chunks of 1,024 inputs. The
     world is deterministic, so `start` rebuilds it by replaying the log and checks the replayed
     state hash against the one saved.
   - **Tests.** Orders 16 to 21 script the phase 1 commands (the stub's `market`, `merchantOffers`
     and so on); order 22 marks a village as a test's own, which the engine then leaves alone; orders
     23 (`P1_SPAWN_NEAR`) and 24 (`P1_ADVANCE`) materialize a caravan off the flows at a chosen place
     and run the world ahead of game time (TEST mode only).
