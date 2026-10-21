# The industry boundary: what the Java sampler sends and what it applies

**Status:** written 2026-10-20 overnight with the engine side, `crates/rtsim-industry` (C15 to C19).
The Java side is not written. This page, `crates/rtsim-industry/src/obs.rs`, `cmd.rs` and `wire.rs`
change together; `wire.rs` is the reference codec, with round-trip tests.

**It extends** `docs/boundary.md` (the bridge's buffers, hand-over, journal). Everything there holds:
little-endian, records on 8-byte boundaries with sizes a multiple of 8 and at most 65,528 bytes,
unknown kinds skipped by `size`, padding and reserved fields zero. Choices this page makes are
numbered F# in `docs/overnight-choices-industry.md`.

## 1. Kind ranges (F1)

| Range | Direction | Class |
|---|---|---|
| 0x0040 to 0x005F | Java to engine | industry latest-wins observations (state; may be dropped when full) |
| 0x0400 to 0x04FF | Java to engine | industry journaled observations (events; never dropped) |
| 0x8400 to 0x84FF | engine to Java | industry commands |

The bridge's own kinds stay where `docs/boundary.md` puts them. **The bridge must reserve these
ranges.**

## 2. Ids and dictionaries

### 2.1 Ids

| Id | Type | Allocated by | Notes |
|---|---|---|---|
| district | u64 | Java's district index (SavedData) | stable for the world; never reused (F3) |
| inventory | u32 | Java, per district revision | index into the district's `IND_INVENTORY` list |
| station, train | u64 | Java | stable for the world (a hash of Create's UUID is fine) |
| epoch | u32 | engine, per district | reconcile rounds, 1, 2, 3, ...; a chunk attachment holds the last applied |
| op id | u64 | both | the kernel's `journal::OpId` convention; industry engine ids carry namespace 0x49 in bits 56 to 62 (F4) over 56 bits derived from the command's identity (W3, §7) |

**Op-id bit.** Java's ids have the top bit set (`Obs.JAVA_OP_BIT`, `OpId::bridge`) and the
engine's have it clear (`OpId::engine`), as DECISIONS.md O10 and `docs/boundary.md` §2.2 now agree.
Industry engine ids also carry namespace 0x49 in bits 56 to 62, so they never collide with another
engine allocator's.

### 2.2 Dictionaries (F2)

Three new dictionary kinds, sent with the bridge's `DICT` records (observation 0x0002, command
0x8001):

| Kind | Name | Entry name format |
|---|---|---|
| 0x10 | GOOD | `<class>/<max stack>/<registry id>[/<component hash>]` |
| 0x11 | NAME | UTF-8 text: station names, package addresses, district names |
| 0x12 | BLOCK_STATE | the block state's string form, `minecraft:oak_log[axis=y]` |

**GOOD names.** `<class>` is `i` (item, counted in items), `f` (fluid, millibuckets), `e` (energy, in
the energy kind's unit; FE is `e/0/fe`) or `d` (a finite deposit, in its plugin's unit). `<max stack>`
is the item stack's max stack size with its components applied, and 0 for other classes.
`<component hash>` is a hex hash of the stack's `DataComponentPatch` (Java decides the hash; only
equality matters), omitted when the patch is empty. A Destroy `MixtureFluid` with a given
composition is one good (INDUSTRY.md §3.1). Examples: `i/64/minecraft:iron_ingot`,
`i/1/minecraft:diamond_pickaxe/9f3a12c4`, `f/0/destroy:mixture/77ab01ff`, `d/0/oreexcavation:vein`.

**Commands use the engine's dictionary** (`docs/boundary.md` §2.2), and the engine defines each GOOD
entry with the same number and name Java gave it, so the ids coincide. It sends the DICT before first
use in each session.

## 3. Observations

### 3.1 Latest-wins (no `obsSeq` prefix)

Offsets from the record start; the 8-byte header is `u16 kind, u16 size, u32 aux`.

**Entry16** (goods lists): `0 u32 good, 4 u32 reserved, 8 i64 amount`.

| Kind | Name | aux | Fields | Size |
|---|---|---|---|---|
| 0x0040 | IND_STOCK_SAMPLE | n entries | 8 i64 gameTime, 16 u64 district, 24 u32 inventory, 28 u32 sweep, 32 n × Entry16 | 32 + 16n |
| 0x0041 | IND_SWEEP_END | flags: 1 live, 2 creative block, 4 overstressed | 8 i64 gameTime, 16 u64 district, 24 u32 sweep, 28 u32 records, 32 u32 inventories, 36 u32 reserved | 40 |
| 0x0042 | IND_DEPOSIT_SAMPLE | plugin: 1 Ore Excavation, 2 Diesel oil, 3 Destroy crude | 8 i64 gameTime, 16 u64 district, 24 u32 good, 28 u32 sweep, 32 i64 remaining, 40 u32 dim, 44 i32 chunkX, 48 i32 chunkZ, 52 u32 flags (1 infinite) | 56 |
| 0x0043 | (retired: IND_CHANNEL_SAMPLE) | | read as an unknown kind (I13 as corrected, IR13) | |
| 0x0044 | (retired: IND_LEDGER_REQUEST) | | gone with the ledger (I18 as decided, IR14); read as an unknown kind | |
| 0x0045 | IND_WATCHED | 1 a player is near enough to fight for real, 0 not | 8 i64 gameTime, 16 u64 district | 24 |

**Sweeps.** The sampler reads a district's boundary inventories round-robin, K per tick (INDUSTRY.md
§3.2), numbering each full pass a sweep. Each inventory's contents go in one IND_STOCK_SAMPLE, or in
several with the same sweep when they do not fit (the engine adds them up). Deposits read in the
sweep go in IND_DEPOSIT_SAMPLE. After the last one, IND_SWEEP_END says how many records the sweep
sent. **The engine uses a sweep only if it received exactly that many records**; a dropped record
throws the sweep away, so a dropped latest-wins record never corrupts a total (F5).

- The district's stock at the sweep's end is the sum of the inventories' latest samples. Inventories
  read earlier in the sweep lag by a fixed phase, which shifts a rate fit's intercept, never its slope.
- `live` (flag 1) is set only if every chunk of the district was block-ticking for the whole sweep.
  Only live sweeps are learning points; partial sweeps still update what the engine knows each
  inventory holds.
- Postbox inventories count the contents of their packages (`PackageItem.getContents`), as I3 says.

### 3.2 Journaled (prefix 8 u64 obsSeq, 16 i64 gameTime)

| Kind | Name | aux | Fields from 24 | Size |
|---|---|---|---|---|
| 0x0400 | IND_DISTRICT | not-learnable reason (0: learnable) | 24 u64 district, 32 u32 revision, 36 u32 dim, 40 u32 name (NAME), 44 u32 inventoryCount | 48 |
| 0x0401 | IND_DISTRICT_CHUNKS | n chunks | 24 u64 district, 32 u32 revision, 36 u32 part, 40 n × (i32 x, i32 z) | 40 + 8n |
| 0x0402 | IND_INVENTORY | kind flags | 24 u64 district, 32 u32 revision, 36 u32 index, 40 i32 x, 44 i32 y, 48 i32 z, 52 u32 itemSlots, 56 i64 fluidCapacity, 64 i64 energyCapacity | 72 |
| 0x0403 | IND_DISTRICT_REMOVED | 0 | 24 u64 district | 32 |
| 0x0404 | IND_EDIT | 1 placed, 2 broken, 3 configured, 4 command | 24 u64 district, 32 u64 playerMost, 40 u64 playerLeast, 48 i32 x, 52 i32 y, 56 i32 z, 60 u32 reserved | 64 |
| 0x0405 | IND_DISCARD | 1 unattributed session, 2 creative block, 3 overstressed, 4 progressive contraption, 5 other | 24 u64 district | 32 |
| 0x0406 | IND_LOAD_STATE | 0 unloaded, 1 partial, 2 live | 24 u64 district, 32 u32 tickingChunks, 36 u32 totalChunks | 40 |
| 0x0407 | IND_CHUNK_LOADED | 0 | 24 u64 district, 32 u32 dim, 36 i32 x, 40 i32 z, 44 u32 appliedEpoch | 48 |
| 0x040A | IND_CHUNK_UNLOADED | 0 | 24 u64 district, 32 u32 dim, 36 i32 x, 40 i32 z, 44 u32 reserved (0); at `ChunkEvent.Unload` (I7, IR6) | 48 |
| 0x0408 | IND_CONDITIONS | 0 | 24 u32 season, 28 u32 seasonCount, 32 i64 seasonLength, 40 i64 seasonOrigin | 48 |
| 0x0409 | IND_POWER_LINK | 0 | 24 u64 sink, 32 u64 source (0: none, the coupling ends) | 40 |
| 0x0410 | IND_PORT | n entries | 24 u64 district, 32 u32 portKind, 36 u32 inventory (0xFFFFFFFF none), 40 u64 counterpart, 48 u64 opId (0 none), 56 n × Entry16 (amount: + into the district) | 56 + 16n |
| 0x0411 | IND_TRAIN_ARRIVED | n carriages | 24 u64 train, 32 u64 station, 40 u64 district, 48 u32 flags (1 simulated), 52 u32 reserved, 56 carriage blocks | 56 + blocks |
| 0x0412 | IND_TRAIN_DEPARTED | n carriages | 24 u64 train, 32 u64 station, 40 u64 district, 48 u64 destination (0 none), 56 i64 etaTicks, 64 carriage blocks | 64 + blocks |
| 0x0413 | IND_STATION | 0 | 24 u64 station, 32 u64 district (0 none), 40 u32 dim, 44 i32 x, 48 i32 y, 52 i32 z, 56 u32 name (NAME), 60 u32 reserved | 64 |
| 0x0414 | IND_PACKAGE | n entries | 24 u64 district, 32 u32 inventory, 36 u32 direction (1 in, 2 out), 40 u32 address (NAME), 44 u32 packages, 48 u64 opId (0 none), 56 n × Entry16 | 56 + 16n |
| 0x0420 | IND_RECONCILE_RESULT | n entries | 24 u64 opId, 32 u64 district, 40 u32 epoch, 44 u32 dim, 48 i32 chunkX, 52 i32 chunkZ, 56 u32 m blocksDestroyed, 60 u32 blocksSkipped, 64 n × Result24, then m × Pos16 (the blocks broken) | 64 + 24n + 16m |
| 0x0421 | IND_CARRIAGE_RESULT | n entries | 24 u64 opId, 32 u64 train, 40 n × Result24 (first field: carriage) | 40 + 24n |
| 0x0430 | IND_SPAWN_SAMPLE | 0 | 24 u64 district, 32 i64 observedTicks, 40 i64 darkBlocks, 48 u32 spawns[4] | 64 |
| 0x0431 | IND_FIGHT | class 0 to 3 | 24 u64 district, 32 u32 attackers, 36 f32 attackerStrength, 40 u32 attackersKilled, 44 u32 blocksDestroyed, 48 u32 machinesDestroyed, 52 u32 guardsLost, 56 i64 itemsStolen, 64 i64 durationTicks | 72 |
| 0x0432 | IND_SURVEY | flags: 1 mobGriefing | 24 u64 district, 32 u32 revision, 36 u32 perimeterColumns, 40 u32 perimeterGaps, 44 u32 darkSpawnable, 48 u32 machineBlocks, 52 u32 roofedMachines, 56 u32 killMachinesAtGaps, 60 u32 ammoGood (0 none), 64 f32 turretStrength[4], 80 u32 guards[4], 96 u32 ammoPerEngagement, 100 u32 reserved | 104 |
| 0x0433 | IND_SURVEY_BLOCKS | n blocks | 24 u64 district, 32 u32 revision, 36 u32 part, 40 n × Block24 | 40 + 24n |

**Sub-records:**
- **Carriage block:** `0 u32 index, 4 u32 itemSlots, 8 i64 fluidCapacity, 16 u32 entryCount,
  20 u32 reserved, 24 entryCount × Entry16`; 24 + 16n bytes.
- **Result24:** `0 u32 inventory (or carriage), 4 u32 good, 8 i64 requested, 16 i64 applied`.
- **Pos16:** `0 i32 x, 4 i32 y, 8 i32 z, 12 u32 reserved`.
- **Block24:** `0 i32 x, 4 i32 y, 8 i32 z, 12 u32 state (BLOCK_STATE), 16 u32 flags (1 perimeter,
  2 roofless, 4 machine, 8 boundary inventory), 20 u32 inventory (0xFFFFFFFF none)`.

**Codes:**
- **Port kinds:** 1 train cargo, 2 package, 3 player container session, 4 player toss, 5 player pickup,
  6 stock block broken by a player, 7 retired (was an Ender Transmission channel; I13 as corrected), 8 reserved
  for engine commands applied (IND_RECONCILE_RESULT already counts as the port, so Java does not
  send an IND_PORT for a reconcile), 9 other.
- **Not-learnable reasons** (INDUSTRY.md §6): 1 rotation crosses the boundary, 2 electric line crosses
  the boundary, 3 mixture not steady, 4 finite deposit unknown, 5 moving contraption, 6 spawn-fed
  farm; others are shown as "not learnable".
- **Threat classes:** 0 ground melee, 1 ground ranged, 2 flying, 3 siege. The sampler maps entity
  types to classes (zombies 0, skeletons 1, phantoms 2, creepers 3; the full table is the bridge's).

### 3.3 What each observation means to the engine

**Membership.** IND_DISTRICT declares or revises a district; its chunk list follows in
IND_DISTRICT_CHUNKS parts and its inventories in IND_INVENTORY records with the same revision. A new
revision replaces the old lists. Detection (I2) is the Java side's.

**Edits and staleness.** IND_EDIT (a `BlockEvent` with a player inside the district) makes a learned
district Stale (I5: stale freezes) and discards the open window. IND_DISCARD discards the open window
only. Machine edits by machines and chunk loaders switching are not edits (INDUSTRY.md §3.3).

**Load states.** IND_LOAD_STATE is sent when the count of block-ticking chunks crosses 0 or the total.
- **Live:** the replay catches up to now and stops, and the engine reconciles everything it did
  (a full round).
- **Partial** or **Unloaded:** the replay covers the chunks that are not loaded (I7, Ben's "simulate
  the rest", IR5 to IR8). A process of the district's model runs in the replay only while every good
  it touches lives in unloaded chunks; one that reaches a loaded chunk runs nowhere in the sim (it
  may run for real; never in both places).

**Chunk loads and unloads (I7).** IND_CHUNK_LOADED is sent at `ChunkEvent.Load` for a chunk of a
district, carrying the epoch in the chunk's `mic_rtsim:reconcile` attachment (0 if none), and
IND_CHUNK_UNLOADED at `ChunkEvent.Unload`. Once a tick's records are in, the engine brings the
replay's coverage up to date: chunks that loaded leave it in a partial reconcile round (their
deltas only, each chunk on its own epoch; the rest of the change stays held per inventory and rides
on in the replay of the chunks still unloaded), chunks that unloaded join it. The engine answers a
load with every pending IND_CHUNK_RECONCILE for that chunk above its epoch. A Java side that never
sends IND_CHUNK_UNLOADED is read the old way: when no chunk ticks, every chunk counts as unloaded.

**Ports** (INDUSTRY.md §3.3). Everything that crosses the boundary while loaded:
- train cargo diffs at `Train.arriveAt` and `leaveStation`;
- train mail at postboxes, as IND_PACKAGE;
- player container sessions (`PlayerContainerEvent.Open` to `Close`, the session's diff);
- tosses, pickups, and stock blocks broken by players.

Ender Transmission channels are not ports: their endpoints are one district's (I13 as corrected).

Learning subtracts ports from stock change (`N = S - S0 - P`), so they are never production. Every
applied IND_CHUNK_RECONCILE is reported as an IND_RECONCILE_RESULT, which the engine also treats as a
port.

**Trains.**
- IND_TRAIN_ARRIVED at a station whose district is **loaded** (`simulated` clear) feeds only the line
  statistics; the cargo diff comes as IND_PORT when the train leaves.
- At a **simulated** station (`simulated` set: the district is unloaded and the carriage has no entity
  in that dimension), the engine decides the exchange from the learned station pattern (I15) and
  answers with IND_CARRIAGE_DELTA.
- IND_TRAIN_DEPARTED books the leg for line edges and threats (§8.1, §13.3).

**Results.** IND_RECONCILE_RESULT and IND_CARRIAGE_RESULT report, per entry, what was asked and what
was applied. A shortfall is not an error: the engine trims what it funded (§4.2).

**Power lines.** IND_POWER_LINK is the PowerGrid plugin's: a `TransmissionLine` feeds the sink
district's loads from the source district's generators. While both are simulated, the sink runs only
while the source runs (I13). A source that is live is assumed to be running (F21).

**Watched.** IND_WATCHED says whether a player is within the district's entity-ticking area, so
attacks there are real fights (INDUSTRY.md §13.1). The sampler sends it when it changes and every few
seconds while it holds; an unloaded district is never watched. Attacks on a district that is
unwatched resolve abstractly: applied at once, as a reconcile round, if its chunks are loaded; at the
next load if not (C18).

**Threat data.**
- **IND_SPAWN_SAMPLE:** natural hostile spawns counted while a player was in the district at night,
  per dark spawnable block in and around it (I20).
- **IND_FIGHT:** a real fight in the district, the observation that caps abstract outcomes (I22).
- **IND_SURVEY and IND_SURVEY_BLOCKS:** the defence vector and the exposed block set (I21),
  recomputed after edits.

## 4. Commands

Every command has the bridge's `cmdSeq` at 8; bodies start at 16. Every one carries an
op id at 16 that Java journals when it applies the command (COMMAND_RESULT, `docs/boundary.md`
§2.3), and skips when it sees again.

| Kind | Name | aux | Fields from 16 | Size |
|---|---|---|---|---|
| 0x8400 | IND_CHUNK_RECONCILE | n deltas | 16 u64 opId, 24 u64 district, 32 u32 epoch, 36 u32 dim, 40 i32 chunkX, 44 i32 chunkZ, 48 u32 destroyCount, 52 u32 reserved, 56 n × Delta32, then destroyCount × Destroy16 | 56 + 32n + 16m |
| 0x8401 | IND_CARRIAGE_DELTA | n entries | 16 u64 opId, 24 u64 train, 32 u32 reason (1 exchange, 2 loot), 36 u32 reserved, 40 n × (u32 carriage, u32 good, i64 delta into the carriage) | 40 + 16n |
| 0x8402 | (retired: IND_CHANNEL_DELTA) | | never written; read as an unknown kind (IR13) | |
| 0x8403 | IND_DEPOSIT_DELTA | plugin | 16 u64 opId, 24 u32 dim, 28 i32 chunkX, 32 i32 chunkZ, 36 u32 good, 40 i64 delta | 48 |
| 0x8404 | IND_WRITE_PACKAGE | n entries | 16 u64 opId, 24 u64 district, 32 u32 inventory, 36 i32 x, 40 i32 y, 44 i32 z, 48 u32 address (NAME), 52 u32 count, 56 n × Entry16 (one package's contents) | 56 + 16n |
| 0x8405 | (retired: IND_LEDGER) | | gone with the ledger (I18 as decided, IR14); read as an unknown kind | |

- **Delta32:** `0 u32 inventory, 4 u32 good, 8 i32 x, 12 i32 y, 16 i32 z, 20 u32 reserved,
  24 i64 delta`.
- **Destroy16:** `0 i32 x, 4 i32 y, 8 i32 z, 12 u32 expectedState (BLOCK_STATE)`.

### 4.1 Applying IND_CHUNK_RECONCILE (I8, I23)

1. Hold it until the chunk is loaded. The engine sends it in answer to IND_CHUNK_LOADED, so it
   normally arrives within a tick or two of the load.
2. If the chunk's `mic_rtsim:reconcile` attachment has an entry for this district with
   `appliedEpoch >= epoch`, or the op id is in the journal's applied set, skip it (answer DUPLICATE).
3. Apply **every removal first**, then the additions, each through the inventory's capability
   handler at `(x, y, z)`. A removal takes up to what is there; an addition puts in up to what fits.
   Deposit goods (class `d`) go to the deposit's plugin (`OreData.extract`, Destroy's
   `chunk_crude_oil`) instead.
4. Break each destroy entry whose block still has `expectedState`, with `level.destroyBlock(pos,
   true)`, so its contents drop; skip the others. Never place fire.
5. Set the attachment's epoch for this district to `epoch`, in the same tick, so the contents and the
   epoch reach disk together in the chunk's NBT (`neoforge:attachments`).
6. Journal IND_RECONCILE_RESULT with every entry's requested and applied amount, the positions of
   the blocks broken, and the count skipped.

A chunk that loads with several pending epochs gets them in order. A crash at any point replays
correctly: a chunk saved before step 5 still has the older epoch and its older contents.

### 4.2 Shortfalls

- **A removal that comes up short** (a machine that ticked first took the input) trims the outputs it
  funded, by the model's ratios (INDUSTRY.md §5). The engine does this:
  - additions not yet applied (in chunks still unloaded) are reduced;
  - additions already applied get a removal in a new epoch.
- **An addition that does not fit** stays in the engine's pending stock for the district and is
  retried at its next load. The replay's capacity model is conservative, so this
  should be rare.
- **A trim removal that comes up short** (the outputs already moved on) is booked as an untracked
  mint in the ledger, so totals stay exact and the gap is visible.

### 4.3 The rest

- **IND_CARRIAGE_DELTA:** applied to `Carriage.storage` through `TrainCargoManager`'s handlers (whose
  `changeDetected` resets the idle-cargo counter), wherever the train now is. Java journals
  IND_CARRIAGE_RESULT with what it applied.
- **IND_DEPOSIT_DELTA, IND_WRITE_PACKAGE:** these stores are level SavedData, so Java applies them at
  once, loaded or not. Each is answered with the bridge's COMMAND_RESULT. The engine routes a deposit
  delta by good first (IR11): Destroy's level-wide pollution (greenhouse, ozone depletion, acid rain)
  goes as IND_DEPOSIT_DELTA every `live_pollution_ticks` while its district replays (I14: "level-wide
  types apply live while unloaded"), the rest at the reconcile; chunk types ride the chunk's reconcile.

## 5. What the sampler must never do

- Sample machine inventories (basins, depots, crushers, vats) as stock. They are work in progress
  (I3).
- Count a player's transfer as anything but a port.
- Send a learning sweep (`live`) while any chunk of the district is not block-ticking.
- Apply a reconcile without writing the epoch in the same tick.

## 6. The Java side (2026-10-20)

Written overnight in the core's `industry` package and the Create plugin `mic_rtsim_create`
(`neoforge/`); choices H1 to H32 in `docs/overnight-choices-industrymc.md`. The industry plugins (Ore
Excavation, Diesel Generators, Ender Transmission, PowerGrid, Destroy, climate, radar) and the threat
sampler followed the same day; choices Q1 to Q26 in `docs/overnight-choices-indplugins.md`. The Java
constants are `IndRecords.java`, which mirrors wire.rs field for field (the contract test pins every
record below against wire.rs fixtures).

- **Sent:** IND_STOCK_SAMPLE, IND_SWEEP_END, IND_WATCHED, IND_DEPOSIT_SAMPLE (Ore
  Excavation, Diesel, Destroy crude oil and pollution);
  IND_DISTRICT, IND_DISTRICT_CHUNKS, IND_INVENTORY, IND_STATION, IND_DISTRICT_REMOVED, IND_EDIT (aux 1, 2,
  4), IND_DISCARD (aux 1, 5), IND_LOAD_STATE, IND_CHUNK_LOADED, IND_CHUNK_UNLOADED (at ChunkEvent.Unload), IND_CONDITIONS, IND_POWER_LINK, IND_PORT
  (kinds 1, 3, 4, 5, 6), IND_TRAIN_ARRIVED, IND_TRAIN_DEPARTED, IND_PACKAGE (loaded and unloaded
  districts; the mail is out of the train's cargo port, Q6), IND_RECONCILE_RESULT, IND_CARRIAGE_RESULT,
  IND_SPAWN_SAMPLE, IND_FIGHT, IND_SURVEY, IND_SURVEY_BLOCKS.
- **Applied:** IND_CHUNK_RECONCILE (§4.1, every step; a deposit delta through its plugin, in the chunk's
  tick), IND_CARRIAGE_DELTA, IND_DEPOSIT_DELTA (by
  the plugin that owns the good: Diesel's chunk oil, at once), IND_WRITE_PACKAGE (a loaded postbox or the
  station port's offline buffer).
- **Forms Java decides:** a GOOD component hash is 16 hex digits (H2); a deposit good is
  `d/0/<kind id>/<16 hex digits of FNV-1a over dimension|chunkX|chunkZ>`, or over `pollution|<district>` for
  attributed pollution (Q3, Q9); a BLOCK_STATE name is `BlockStateParser.serialize(state)`, which a destroy
  entry is compared with; station and train ids are SplitMix64 of Create's UUIDs (H19); the counterpart
  of a player port is the player's UUID halves XORed.
- **Not sent:** temperature (IND_CONDITIONS has no field for it; proposal IND_CLIMATE, Q20, not built).
  Not-learnable code 6 is never sent in IND_DISTRICT (I9): `IndRecords.NotLearnable.sent` turns it into 0.
  While the industry module is off Java sends nothing at all, and when it is on again it announces every
  district (IND_DISTRICT with its chunks, inventories and stations), load state, watched flag, calendar,
  survey and power link again (U10, IR15). The channel
  records (IND_CHANNEL_SAMPLE, IND_CHANNEL_DELTA, port kind 7) are retired from wire.rs, IndRecords and
  the fixtures (I13 as corrected: an Ender Transmission channel's endpoints are one district, Q7).

### 6.1 IND_APPROACH (H23): reconcile ahead of arrival

Ben chose "reconcile ahead of arrival" for the stale-tick race. A latest-wins record (H23); the engine
acts on it since this rework (IR9, IR10): a **closing** record within `approach_distance_blocks`
stops the district's replay and sends every chunk's reconcile at once (`Round::Ahead`). Java holds each
until its chunk loads and **applies it at the chunk's `ChunkEvent.Load`, before any machine there
ticks**, journaling its IND_RECONCILE_RESULT before the chunk's IND_CHUNK_LOADED (which then carries the
epoch now applied). The replay stays stopped while closing records keep coming and goes on from the
reconciled state `approach_hold_ticks` after the last one (the player turned away).

| Kind | Name | aux | Fields | Size |
|---|---|---|---|---|
| 0x0046 | IND_APPROACH | 1: closing in since the last one | 8 i64 gameTime, 16 u64 district, 24 u32 distance (blocks from the nearest player to the district's chunk box), 28 u32 etaTicks (distance / closing rate; 0xFFFFFFFF unknown) | 32 |

Sent every 20 ticks for each unloaded district within `approachRadiusBlocks` (256, PLACEHOLDER) of a
player; `approachSignal = false` stops it.

`wire.rs` reads it (`Obs::Approach`, W16) and the engine keeps the latest on the district
(`District::approach`). Java should send it for partly loaded districts too (a player walking toward
the unloaded part), and apply held reconciles at `ChunkEvent.Load` (today it applies them at the next
server tick): the Java pointers in docs/choices-indrework.md.

## 7. The engine side: rtsim-jni

Written 2026-10-20 with `crates/rtsim-jni/src/engine/industry.rs` (choices W1 to W23 in
`docs/overnight-choices-industryjni.md`). What the native engine does with these records:

- **Reading.** Every industry record decodes through `rtsim_industry::wire`. Journaled ones pass the
  kernel's `Replay` first: IND_PORT and IND_PACKAGE also under Java's op id; IND_RECONCILE_RESULT and
  IND_CARRIAGE_RESULT by sequence only, and the engine takes each once by state, so a result
  journaled again from a chunk (the Java side's pending-record pattern) is applied once whatever
  order Java journals it and its COMMAND_RESULT in (W11). The engine's own session start runs after
  the replayed journal (W19). Latest-wins records are handed to the engine in game-time
  order with the journaled ones, journaled first on a tie (W9): a hand-over carries its latest-wins
  records first, but within a tick a load, a port or a result happens before the stock is sampled.
  Kinds this build does not read are counted and skipped.
- **Dictionaries.** The engine's GOOD, NAME and BLOCK_STATE ids are Java's own; it writes the DICT
  (kind, Java's id, Java's name) before the first command that uses it in a session. Dimension ids
  in commands are the engine's own dimension dictionary (the one its other commands use): read them
  by name, as for every command.
- **Op ids** are derived from the command's identity (W3): the same decision made again after a
  crash carries the same id, and a different one never takes an id an earlier one had.
- **Answers.** IND_CARRIAGE_DELTA, IND_DEPOSIT_DELTA and IND_WRITE_PACKAGE are
  kept from the moment they are written until their answer (IND_CARRIAGE_RESULT, or COMMAND_RESULT
  applied, duplicate or failed) is admitted, and written again first after a restart with their
  original sequences and op ids (W4). IND_CHUNK_RECONCILE is resent at the chunk's next
  IND_CHUNK_LOADED instead (F27). A failed IND_CARRIAGE_DELTA counts as nothing applied, taken back
  from the engine's exchange record; a failed deposit or package command is logged and the
  engine keeps its books as if applied (W10). A command waits a while for a dictionary name Java
  has not sent yet (W21).
- **The switch (U10, replacing W8).** With `Module.INDUSTRY` off the engine ignores every industry
  record, journaled ones included (counted), runs nothing and writes nothing; its queue, outbox and
  pages wait as they are. Switched on in a session, its own session start runs at that game time, so it
  catches up over the time it was off; Java then announces its districts and load states again, as at
  a session start (IR15; `Industry.endOfTick` and `IndustryPlugin.announce`).
- **Mode.** The native engine applies. Read-only (phase I-A, F25) is retired with the ledger (I18,
  IR14): the `nOpen` flag is read and ignored with a warning.
- **Threats** from other systems arrive as an order (`IndustryThreat`, W12) until those systems run
  in the engine.
- **Tests** can order an industry command (`INDUSTRY_COMMAND`, W17): a record with Java's own ids,
  written as the industry engine's own, so the industry GameTests run on either engine.
- **IND_APPROACH** reconciles ahead of arrival (§6.1).

## 9. The glass box's records (GLASSBOX.md §1.4, G-B19)

**Status:** phase G1, 2026-10-20 (branch `feat/glassbox-g1`). The Rust reference codec is
`crates/rtsim-industry/src/glass/codec.rs` (types in `records.rs`); the Java mirror is
`neoforge/core/.../industry/glass/GlassRecords.java`, pinned by `GlassWireContractTest` against
`glass-wire-fixtures.txt` (printed by `neoforge/tools/industry-wire-fixtures` with the `glass`
argument). Choices are GB# in `docs/choices-glassbox-g1.md`.

### 9.1 Kinds

| Kind | Name | Class | Direction |
|---|---|---|---|
| 0x0048 | IND_GRAPH_NODE | latest-wins | Java to engine |
| 0x0049 | IND_GRAPH_EDGE | latest-wins | Java to engine |
| 0x004A | IND_GRAPH_RECIPE | latest-wins | Java to engine |
| 0x004B | IND_GRAPH_DOMAIN | latest-wins | Java to engine |
| 0x004C | IND_GRAPH_END | latest-wins | Java to engine |
| 0x004D | IND_PROBE_SAMPLE | latest-wins, not counted in a sweep | Java to engine |
| 0x040B | IND_CHUNK_TICKING | journaled | Java to engine |
| 0x8406 | IND_PROBE_SET | command | engine to Java |

**A compile** is a district's graph: NODE, EDGE, RECIPE and DOMAIN records carrying the district's
compile number (Java's counter, from 1), then IND_GRAPH_END saying how many records it sent. **The
engine uses a compile only if it received exactly that many records with its number** (as sweeps,
F5); a dropped record throws the compile away. Java resends the last compile every
`glass.resendTicks` (flag RESEND), so a lost compile is redone without a request (GB4).

### 9.2 Layouts

Every graph record (0x0048 to 0x004B) starts `8 i64 gameTime, 16 u64 district, 24 u32 compile, 28 u32 index`
(the record's index within its kind). `NONE` is 0xFFFFFFFF.

| Kind | aux | Fields from 32 | Size |
|---|---|---|---|
| IND_GRAPH_NODE | node kind | 32 u32 model, 36 u32 domain (NONE), 40 u32 inventory (NONE), 44 u32 flags, 48 i32 x, 52 i32 y, 56 i32 z, 60 u32 reserved, 64 u32 rule, 68 u32 n rates, 72 i64 capacity, 80 i64 param0, 88 i64 param1, 96 i64 param2, 104 i64 param3, 112 n × Rate24 | 112 + 24n |
| IND_GRAPH_EDGE | edge kind | 32 u32 from, 36 u32 to, 40 u32 order, 44 u32 flags, 48 u32 capNum, 52 u32 capDen (0/0 unlimited; units per tick), 56 u32 latency, 60 u32 n filter, 64 i64 param, 72 n × u32 good | 72 + 4n, padded to 8 |
| IND_GRAPH_RECIPE | inputs + outputs | 32 u32 recipeType, 36 u32 heat (at least), 40 u32 n inputs, 44 u32 n outputs, 48 u32 duration, 52 u32 name (NAME, 0 none), 56 n × Term24 (inputs first) | 56 + 24n |
| IND_GRAPH_DOMAIN | n members | 32 u64 network, 40 u32 n levels, 44 u32 current level, 48 u32 driver node (NONE), 52 u32 flags (1 overstressed now, 2 stress disabled), 56 f64 live stress, 64 f64 live capacity, 72 L × Level24, then n × Member24 | 72 + 24L + 24n |
| IND_GRAPH_END | flags: 1 complete, 2 not compilable, 4 black box, 8 resend | 16 u64 district, 24 u32 compile, 28 u32 window (checksum window ticks; 0 the engine's `glass.checksumWindowMin`), 32 u32 records, 36 u32 nodes, 40 u32 edges, 44 u32 recipes, 48 u32 domains, 52 u32 reason, 56 u64 fingerprint, 64 u64 frame (0 the level; else a Sable sub-level id), 72 u32 revision, 76 u32 compileTicks | 80 |
| IND_PROBE_SAMPLE | n entries | 16 u64 district, 24 u32 sweep, 28 u32 node, 32 u32 compile, 36 reserved, 40 n × Entry16 | 40 + 16n |
| IND_CHUNK_TICKING | 1 ticking, 0 not | (journaled prefix) 24 u64 district, 32 u32 dim, 36 i32 chunkX, 40 i32 chunkZ, 44 reserved | 48 |
| IND_PROBE_SET | n nodes | (cmdSeq at 8) 16 u64 district, 24 u32 compile, 28 reserved, 32 n × u32 node | 32 + 4n, padded to 8 |

**Sub-records:**
- **Rate24:** `0 u32 level, 4 u32 recipe (NONE: the transfer, or the only recipe), 8 u32 batch,
  12 u32 ticksNum, 16 u32 ticksDen, 20 u32 flags (1 shared capacity, 2 first cycle)`: at speed level
  `level`, one operation on `batch` crafts (machines) or units (movers) every `ticksNum/ticksDen`
  ticks. Java ran the machine's own countdown for these numbers (G-B5).
- **Term24:** `0 u32 good, 4 u32 flags (1 catalyst, 2 chance), 8 i64 num, 16 i64 den`: `num/den` of
  the good per craft; chance outputs at their expectation (G-B11).
- **Level24:** `0 u32 selector, 4 i32 speedNum, 8 u32 speedDen, 12 reserved, 16 f64 capacity`: the
  root source's RPM as a rational and the domain's capacity in SU at that level; `selector` is the
  driver's value that picks the level (a boiler's level, 0 to 18), 0 without a driver.
- **Member24:** `0 u32 node (NONE), 4 u32 count, 8 i32 ratioNum, 12 u32 ratioDen, 16 f64 impact`:
  `count` blocks at speed `ratio × root speed` and `impact` SU per RPM each.

**Codes** (the Rust `records.rs` modules, mirrored in `GlassRecords`):
- **Node kinds:** 1 store, 2 buffer, 3 machine, 4 mover, 5 junction, 6 heater, 7 source, 8 sink,
  9 black box, 10 controller, 11 port, 12 boiler.
- **Edge kinds:** 1 adjacency, 2 belt, 3 chute, 4 pipe, 5 chain, 6 air zone, 7 arm reach, 8 heat
  (param: level), 9 control (param: 1 stop when on, 2 run when on), 10 observe, 11 line, 12 drop.
- **Node flags:** 1 bounded, 2 any output blocks, 4 stochastic (params 2 and 3: a craft's output
  second moment as a fraction), 8 fluid, 16 calibrated (TUNABLE), 32 on a moving frame, 64 pass-through (an in-place machine on a line passes goods no recipe matches).
- **Edge flags:** 1 deny list, 2 fluid, 4 bounded, 8 a filtered side.
- **Rules:** 1 to 7 the brass tunnel's modes (split, forced split, round robin, forced round robin,
  prefer nearest, randomize, synchronize), 8 prefer first, 9 merge, 10 andesite split, 11 side insert.
- **Models, recipe types, heat levels:** as `records.rs` lists them (`model::MILLSTONE` 10, ...).

**Node params by kind:** heater: remaining burn ticks, heat level, burn ticks per fuel item,
insertion threshold; boiler: tank blocks, engines, active heat, water supply (mB per tick × 1,000);
controller: on-above, off-below, flags (1 inverted), level at compile; animal source: animals,
eligible animals, a craft's output second moment as num/den.

### 9.3 What the graph means (the contract both halves build to)

GLASSBOX.md §2 gives Create's formulas and §3 the solve; this is how they map onto the records, so the
extractor and the solver agree. All rates are **per tick**: items in units, fluids in millibuckets.

**Indices.** Node, edge, recipe and domain indices are the records' `index` fields, 0 to count - 1 within
the compile. A node's `inventory` is the IND_INVENTORY index of END's `revision` (stores the sampler
reads); NONE for a store the sampler does not read.

**Rate entries.** A node in a domain uses the entries whose `level` is the domain's level index (into
its Level24 list); a node without a domain uses level 0. An entry gives `batch` per `ticksNum/ticksDen`
ticks: its capacity is `batch × ticksDen / ticksNum` per tick. A node with no entry for the level has
capacity 0 there (a stopped or overstressed domain). FIRST_CYCLE entries are latency only, never rate.
A node with no entries at all is unlimited (a junction, a buffer, a water source). An entry with `batch`
0 has capacity 0: Java writes one at level 0 for a limited node that runs at no level (a powered funnel, a
machine whose domain is stopped), so that it is not read as unlimited (GB45).

**Nodes.**
- **Store:** supplies each good it held at its inventory's latest sweep and accepts while it had room;
  neither is rate-limited (edges and the next node limit them). Stores are the only nodes whose flows the
  checksum measures.
- **Buffer:** work in progress (a depot, a chute slot, a belt's contents): conserves each good, in
  equals out.
- **Machine:** crafts of recipe r per tick `c_r ≥ 0`, time-shared: `sum over r of c_r / capacity_r ≤ 1`.
  Each craft takes the recipe's inputs (catalysts are needed, not consumed) and makes its outputs, chance
  outputs at their expectation. With ANY_OUTPUT_BLOCKS every output must be accepted in full
  (`c_r × out ≤ acceptance`). A machine fed from a basin or a depot is one node: the edges into the basin
  or depot are its inputs and the edges out of it its outputs. A depot worked by two (a press and a fan
  zone) is a BUFFER whose LINE edges run through each in turn, BOUNDED (GB37). A machine on a line (PASS_THROUGH) passes
  goods none of its recipes takes. A recipe with `heat` above NONE runs only with a Heat edge in whose
  heater's level is at least `heat`, at that heater's duty.
- **Mover:** one SHARED entry: units per tick across every good. Its in-edges are tried in `order`, and
  its outflow splits among its out-edges by its `rule` (an arm's selection mode; NONE is one output).
- **Junction:** unlimited unless it has an entry; splits by its `rule` over out-edges in `order`
  (GLASSBOX.md §3.3). Filtered sides (FILTERED_SIDE) are tried before the rest.
- **Heater** (a blaze burner): `params` are remaining burn ticks, heat level when lit, burn ticks per
  fuel item (the shortest of the fuels that reach it; 0: never runs out, a creative burner; 1 while no
  fuel reaches it) and the insertion threshold. Each fuel is a FUEL recipe whose `heat` is the level it
  burns at and whose `duration` its burn ticks (GB49). It burns while lit
  whatever it heats, so it accepts fuel up to `1 / burnTicks` items per tick, and its **duty** is
  `min(1, fuel accepted × burnTicks)`. L counts only the fuel supplied; U also counts remaining burn.
- **Boiler:** accepts all the water it is offered and voids it (`BoilerData`'s handler returns the
  amount). Its level is `min(activeHeat, min(18, ceil(w) / 10), min(18, tankBlocks / 4))` with `w` the
  water accepted in mB per tick and `activeHeat` the sum of its Heat edges' params over lit heaters. It
  drives its domain: the domain's `driver` is the boiler and each Level24's `selector` is a boiler level
  (Java tabulates every level the engines can see, with each level's speed and capacity). A heater with
  duty strictly between 0 and 1 makes the boiler's goods bounded: L counts heaters at duty 1, U every
  heater with duty above 0.
- **Source:** emits its recipe's outputs (no inputs) at its entries' rate; unlimited without entries
  (a water source block under a pump: the pump limits it). An animal source (ANIMAL_SHEEP) is STOCHASTIC:
  a craft is one shearing, its rate the closed form of vanilla's regrowth for `params[1]` eligible
  animals among `params[0]` the deployer can pick, and `params[2] / params[3]` the craft output's second
  moment (14/3 for wool's `1 + nextInt(3)`). Java writes the rate entry as one shearing per
  `(2034 + params[0] × c) / params[1]` ticks, c the deployer's cycle: a sheared sheep eats after 2,034
  ticks on average (`EatBlockGoal`) and each cycle shears a random entity of the deployer's box (GB53). A source that is not accepted waits (unsheared sheep keep
  their wool): nothing is lost.
- **Sink:** accepts everything (burned or voided) up to its entries' rate, unlimited without.
- **BlackBox:** a node the compiler could not model. L takes it as producing nothing; a good with a
  black box on its path has no U (it is reported unbounded).
- **Controller** (a threshold switch, a factory gauge): `params` are on-above, off-below, flags (1
  inverted) and the observed level at compile, in items or mB. Its Observe edges come from the stores it reads (a
  filter on the edge limits the goods counted, empty counts all, in units); its Control edges go to
  the nodes it gates (`param` 1 stops the target while the switch is on, 2 runs it only while on). It
  is on when the observed level reaches on-above, off when it falls to off-below, else as before
  (hysteresis). "On" is the switch's state before inversion; the Control `param` already includes the
  inversion (an inverted switch powers its targets while off, so it runs them only while on: 2), and the
  inverted flag is for display only (GB51). In G1 the engine evaluates it at every sweep from the observed stock, so each sweep
  interval is predicted in the regime it was in; G2 turns a cycling switch into the closed form of G-B8.
- **Port:** an exchange across the district (a station, a postbox): G1 reports its goods unbounded.

**Edges.** `capacity` is `capNum / capDen` units per tick (0/0 unlimited); an edge carries only the goods
its filter lets through. `order` ranks a node's out-edges for its rule (distance order for PREFER_NEAREST,
the arm's output list, the tunnel's sides); `latency` is the ticks from tail to head. Heat edges carry
`param` (for a basin, the heater's heat level when lit; for a boiler, its active heat when lit); Control
and Observe edges carry no goods.

**Kinetic domains.** At level `l`, the stress is `sum of count × |ratio × speed(l)| × impact` over
members and the domain is overstressed when that exceeds the level's capacity, as Create's
`KineticNetwork` does in floating point; then every member has capacity 0. STRESS_DISABLED domains are
never overstressed. Without a driver the domain stays at its `current` level.

**What is predicted and measured.** Per good, the solve gives the flow into stores and the flow out of
stores, each as L (the fixed point of §3.2 with bounded rules taken against production) and U (the LP
maximum with routing free). The predicted net change of the district's stock is `in - out`; the window
measures the slope of the sampled stock. A good is **exact** when no bounded node, edge, rule or
unconverged round lies on its paths: it passes when `|measured - (inL - outL)| ≤ z × σ + q`. A good is
**bounded** otherwise: it passes when `inL - outU - tol ≤ measured ≤ inU - outL + tol` with
`tol = z × σ + q`. A good is **unbounded** when a black box or a port lies on its paths.

### 9.4 What the engine does with them (G1, read only)

- Stores the latest complete compile per district, solves it (the floor L and the ceiling U per
  good, G-B1, G-B15) and checks it against one window of live sweeps (G-B14). Nothing changes the
  world: the replay still runs the C19 black box.
- On a mismatch it derates the limiting node, sends IND_PROBE_SET for the buffers upstream of it, and
  after `glass.mismatchStrikes` windows demotes the node's kind to measured, in its own state only.
- IND_PROBE_SAMPLE feeds the localization; IND_CHUNK_TICKING is kept per district for G2's partial
  loads (I7, G-B6).
