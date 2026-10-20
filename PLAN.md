# mic-rtsim: a simulated world for MIC

**Status:** design for Ben's review, 2026-10-19. Nothing is built. This replaces the baseline plan of the
same day.

**Read it with:**
- `DECISIONS.md`. It is authoritative. Every D, A, C and X entry there is final, and this plan cites
  them by number.
- `SCALE.md`. It holds the scale architecture: the detail ladder, flows, lazy regions, paging,
  compaction, complexity contracts and galactic planets. This plan points to it and does not repeat it.
- `research/economy-2026-10-19.md`: the phase 1 economy (C13).
- `research/veloren-rtsim-2026-10-19.md`: what Veloren's rtsim really does, read from its source.

**Rules this document follows:**
- Anything not in DECISIONS.md is a **Proposal Pn (Ben)**, with a recommended option and its
  alternatives. §17 lists them all. No work depends on a proposal until Ben answers it (D2).
- Every number is a verified fact (Appendix A), a recipe or tick value, physics, arithmetic from those,
  or a value labelled TUNABLE, PLACEHOLDER or ESTIMATE. Anything unverified says UNVERIFIED.

**What changed from the baseline:**
- Content goes through plugins, one per mod; the core needs no pack mod (D4). Cobblemon is gone. No
  bespoke entities (D3): the goblin, the custom humanoid and the dialogue screen are gone.
- Phase 1 is caravans and markets between vanilla and Ribbits villages (C6 to C14). Dragons are phase 2
  (C3). The rest comes later, in a proposed order (P11).
- §2 matches A6 to A8 and X1 to X3. The adversarial review of 2026-10-19 is applied (list under P10).
- Veloren claims are corrected against the source-level research. A Veloren economy quote in the
  baseline was found in no source and is removed.

## 0. What the sim is

- **Backend infrastructure for emergent gameplay** (D3). It adds no entities, blocks or items unless
  absolutely necessary. Bodies, structures and items come from vanilla and from other mods.
- **The core depends on no pack mod** (D4). A plugin is built around one mod. With no plugin installed,
  the core still runs on vanilla content. Cobblemon is out entirely.
- **Bodies are mapped by data** (§1.4): an abstract body kind maps to whatever entity types are
  installed, with vanilla fallbacks. Bodies look and feel like Minecraft mobs, not players (D7).
- **Ribbits has extra high priority** (D5). It is in phase 1 with four behaviours (C7).
- **Choices are judged only on late-stage performance on a weak PC** (D1), never on writing effort.
- **Scale comes first** (D6). The sim's cost grows with what players observe, how many things happen,
  and how many coarse cells exist. It never grows with how many things exist (SCALE.md §0).

| Phase | Content | Basis |
|---|---|---|
| 0 | Spikes: the host, the potato harness, the scale bench, the puppet and journal GameTests, the storage bench | A6, A9, A11, A16, P4 |
| 1 | Caravans and markets between vanilla and Ribbits villages; slights remembered; four Ribbits behaviours | C6 to C9, C11 to C14 |
| 2 | Dragons on Ice and Fire: slights remembered, migrations on scheduled legs, preying on caravans and herds | C3, A15 |
| Later | Ships, herds, conflict, industry, kaiju, space, history, civilians | order is P11 |

## 1. Layers

### 1.1 The shape

```
 Minecraft server thread (Java)                 rtsim engine (Rust cdylib on mic_native's pools)
 ┌──────────────────────────────────┐           ┌──────────────────────────────────────────┐
 │ mic_rtsim: core bridge, NeoForge │  obs      │ kernel: clock, timing wheel, lazy        │
 │ only. Observers, puppet registry,│ ────────► │   regions, storage, pages, journal       │
 │ journal SavedData, quotas,       │           │   replay, counter RNG                    │
 │ vanilla body adapters            │ ◄──────── │ generic modules: knowledge, settlements, │
 ├──────────────────────────────────┤  cmds     │   trade, travel, ecology, conflict,      │
 │ plugins mic_rtsim_<mod> (P1):    │           │   ships, industry, cosmos                │
 │ adapters and data for one mod    │           │ data: body kinds, goods, cultures ...    │
 └──────────────────────────────────┘           └──────────────────────────────────────────┘
        four reused buffers (two per direction), one hand-over per tick, never per entity
```

- **The engine** knows nothing about Minecraft or any mod. It owns every simulated thing.
- **The core bridge `mic_rtsim`** depends on NeoForge only. It is the only Java that talks to the engine.
  It holds the vanilla adapters, because vanilla is built in.
- **A generic module** is Rust rules plus the schemas of the data it loads. It names no mod.
- **A plugin** carries Java adapters and data for one mod, and no Rust of its own. A rule that no
  generic module has becomes a new generic module, so the engine stays mod-free (part of P1).
- **The engine owns no threads.** It is a plugin of the pack-wide native host (§2).

### 1.2 Generic modules

| Module (crate) | Rules | Data it loads | First phase |
|---|---|---|---|
| knowledge (`rtsim-knowledge`) | reports, witnesses, sentiment, gossip carried by travellers | report kinds, slight weights | 1 |
| settlements (`rtsim-settle`) | adopting villages, households as cohorts, plots, growth | cultures, house template lists | 1 |
| trade (`rtsim-trade`) | storehouse markets, currencies, exchange, news (C13) | goods, recipes, currencies | 1 |
| travel (`rtsim-travel`) | edges, legs, flows, sampled travellers, the encounter index | edge rules | 1 |
| ecology (`rtsim-ecology`) | hunger, forage, herds, migration, predation | species | 2 (dragon hunger), later (herds) |
| conflict (`rtsim-conflict`) | grievance, war bands, Lanchester resolution | unit types | later |
| ships (`rtsim-ships`) | lanes, timetables, ship classes | ship classes, routes | later |
| industry (`rtsim-industry`) | districts as processes between breakpoints | goods | later |
| cosmos (`rtsim-cosmos`) | bodies, orbits, megaprojects | bodies | later |

### 1.3 Plugins and their hooks

Mod ids follow P1. Every hook below is verified unless marked (Appendix A).

| Plugin | Mod (version) | Hooks | Phase |
|---|---|---|---|
| built in (core) | vanilla 1.21.1 | `WanderingTrader`, `TraderLlama`, `Merchant.getOffers/overrideOffers/notifyTrade`, `Villager.getGossips()` (`GossipContainer.add/remove`), village structures by `StructureTags.VILLAGE`, gamerule `doTraderSpawning`, `ServerLevel.isPositionEntityTicking`, `EntityJoinLevelEvent`, `AnimalTameEvent` | 1 |
| `mic_rtsim_ribbits` | Ribbits 4.1.6 | `RibbitEntity` (`Merchant`: `getOffers`, `overrideOffers`, `restock`; `getHomePosition`; `setRibbitData(RibbitData(profession, umbrellaType, instrument))`); a mixin on `RibbitApplyBuffGoal`; `RibbitPlayMusicGoal` around a `masterRibbit`; structure `ribbits:ribbit_village`; the 14 templates under `data/ribbits/structure/houses` | 1 |
| `mic_rtsim_carryon` | Carry On 2.2.6.13 | `tschipp.carryon.events.EntityPickupEvent(ServerPlayer, Entity)`, cancellable, posted on `NeoForge.EVENT_BUS` | 1 (A10) |
| `mic_rtsim_iceandfire` | Ice and Fire CE 2.1-beta.1 | `iceandfire:fire_dragon`, `ice_dragon`, `lightning_dragon`; `DragonBaseEntity` NBT (`AgeTicks`, `Hunger`, `Gender`, `Variant`, `HomeAreaX/Y/Z`); `IafDragonFlightManager.setFlightTarget(Vec3)`; tag `dragon_targets`; roost and cave structures | 2 |
| `mic_rtsim_surveyor` | Surveyor 1.2.4, through Sinytra Connector | `WorldLandmarks.of(Level).put(...)`, `Landmark.global(id, builder)` with `POS`, `NAME`, `LORE`, `COLOR`, `TIME`. A Fabric jar with intermediary names (§7) | first phase that shows map markers |
| `mic_rtsim_numismatics` | Numismatics 1.1.0 | `GlobalBankManager.BANK`, `BankAccount` by UUID; coins worth 1, 8, 16, 64, 512 and 4,096 spurs; no coin recipe | later (P9 mint rule) |
| `mic_rtsim_aeronautics` | Sable 2.0.5 + Create Aeronautics 1.3.2 | the spawn recipe in §7; `SubLevelHoldingChunkMap` freezes unloaded ships; `SubLevelLoadingTicketType` | later (ships) |
| `mic_rtsim_create` | Create 6.0.10 | `GlobalRailwayManager`, `RailwaySavedData` (`create_tracks`), `GlobalPackagePort.offlineBuffer`, `FactoryPanelBehaviour`, `LogisticsNetworkSavedData` (`create_logistics`), `TableClothBlockEntity`, `BlockStressValues` | later (industry) |
| `mic_rtsim_destroy` | Destroy (MIC fork) | `PollutionHelper`, `PollutionEvents`: smog and radioactivity per chunk; greenhouse, ozone depletion and acid rain per level | later |
| `mic_rtsim_deeptime` | Deep Time (not in the pack yet) | `WorldData`, `Rivers`, `DeepTimeClimate` (API_VERSION 4), `DeepTimeSites`; event log writer 2; Deep Time files read natively | with Deep Time |
| `mic_rtsim_nuclear` | mic_nuclear, Create Nuclear 1.3.2-beta.3 | the kaiju trigger (P27); hooks UNVERIFIED | later |
| `mic_rtsim_kaiju` (name P1) | the kaiju body mod (P8) | UNVERIFIED | later |

Later candidates that would also become plugins: TaCZ (`IGunOperator` on every `LivingEntity`), L2
Hostility (`l2hostility:level`), Create Big Cannons, Create Radars (a mixin on `IRadar.getTracks`),
CC: Tweaked (`PeripheralCapability`), Lootr.

### 1.4 Body kinds, mapped by data

- **A body kind is an abstract role:** merchant, pack animal, musician, dragon, raider, ship. Each kind
  lists candidates in order: an entity type, the mod it needs, an NBT or data template, and its adapter.
  The first installed candidate wins; the last is vanilla where vanilla has one.
- **A kind with no installed candidate stays abstract.** It acts only through consequences (losses,
  prices, news) and never appears (part of P15). Every candidate must look like a mob (D7).
- **The adapter interface is spawn, capture and steer.** No `Entity` appears in the engine.

| Kind | Phase 1 and 2 bodies (C11) | Fallback |
|---|---|---|
| human merchant | `minecraft:wandering_trader` | vanilla |
| pack animal | `minecraft:trader_llama`, leashed to the trader | vanilla |
| ribbit merchant | `ribbits:ribbit` with `RibbitData` profession merchant | none needed: without Ribbits, no ribbit villages generate |
| ribbit musician | `ribbits:ribbit` with an instrument in its `RibbitData` | as above |
| dragon | `iceandfire:fire_dragon`, `ice_dragon`, `lightning_dragon` | abstract (P15) |
| raider (later) | P8 candidates | `minecraft:pillager` |

The file format is **Proposal P15 (Ben)**.

## 2. The pack-wide native host `mic_native`

**The problem.** Native mods that each bring a pool sized to the core count fight each other, the
render thread, C2ME and Distant Horizons, and each brings its own loader, CPU probe and profiler.

**The fix.** One host mod, `mic_native` (repo `mods/mic-native`), hosts every native mod in the pack,
including the optimization effort's rewrites (A6). Its licence is MPL-2.0 (A7).

| Part | What it does |
|---|---|
| Loader | Finds, checks and loads every native library with one `natives.json` scheme (Deep Time's schema 2). `cpuprobe` picks the CPU variant once for all of them. |
| Foreground pool | Normal OS priority, for anything a player waits on: a game thread calling a native rewrite, a loading screen. Deep Time's world-creation and first-start bakes run here (X1), and so does the rtsim's history generation. Sizing is a PLACEHOLDER (P24). |
| Background pool | The lowest *normal* OS priority: nice 19 on Linux, `THREAD_PRIORITY_LOWEST` on Windows, utility QoS on macOS. For work nobody waits on: the rtsim during play, prefetch, previews. Not idle-class (`SCHED_IDLE`): there a browser or Discord would win too, and an idle thread holding a lock could stall a game thread while no core is free. Sizing is a PLACEHOLDER (P24). |
| IO lane | Page writes, page reads, prefetch and the rtsim's saves. It never runs compute, so a slow disk never holds a compute thread. |
| Governor | CPU and memory accounting per plugin. It tells background plugins when they are over budget, so they stretch (§4.6). It replaces or feeds a plugin's own memory cap, such as Deep Time's (X3), and never runs alongside it. |
| Profiler | Named zones from every plugin. Tracy in dev builds, plus a ring buffer that `/micperf` reads in game. |
| Service registry | Native plugins publish versioned C-ABI services by name and call each other without Java. |
| GPU device (later) | One wgpu device per process, shared by plugins that compute on the GPU (Deep Time's climate path does today). |

**Rules for every plugin:**
- **No threads or pools of its own.** It submits work to the host.
- **Synchronous calls stay on the caller's thread.** Deep Time's terrain columns on C2ME's workers and
  its Distant Horizons sections stay direct calls, because chunk loading waits on them (X1).
- **Per-job enter and exit hooks** keep per-session thread-locals working (X3). Deep Time sets its
  compute mode, log ring and allocator block cache this way today, through rayon's handlers.
- **No priority inversion.** Foreground work never waits on background work, and background work never
  holds a lock a game thread takes.
- **Pool threads never call into Java** (§3.2).
- **Snapshots, not locks.** A service that foreground code may call publishes an immutable snapshot (an
  `Arc` swapped atomically). Readers take no lock.

**C ABI rules:**
- **Each plugin is its own cdylib** linking a small `mic-native-rt` crate that wraps the host's function
  table. Rust's own ABI is not stable between libraries built separately.
- **Every export is wrapped in `catch_unwind`** and returns an error code; a panic never crosses.
- **Memory is freed only by the library that allocated it;** every allocating export has a free export.
- **`mic-native-rt` installs a counting allocator** in each plugin. It feeds the governor.
- **No private-pool fallback.** A plugin needs the host; Deep Time may require `mic_native` (A8).

**Deep Time and the move (A8, X1 to X3):**
- After Deep Time's P1-19 gate closes and its PGO and allocator work lands, `NativeLoader`, `cpuprobe`
  and `tools/natives` move into `mic_native`. No copy stays in Deep Time.
- Deep Time's per-session thread-locals and its six small private sim-stage pools move onto the
  per-job hooks. That is real work on Deep Time's side.
- Deep Time's results are identical at any pool size (its tests check 1, 3 and 8 threads), so the host
  may choose the size.

### 2.1 What a 2-core potato really gives the sim

- In singleplayer, the render thread and the server thread already occupy both cores.
- **Normal-priority threads use any slack first:** C2ME's chunk workers, Distant Horizons' builders,
  Sodium's meshing, the GC, the JIT compilers, and the host's own foreground pool. The background pool
  gets what is left.
- **With uncapped FPS, the render thread may never sleep.** The pack ships `enableVsync:true` and
  `maxFps:120` in `options.txt`, but players change these.
- **Linux** still gives a nice-19 thread a small share of a contended core: weight 15 against 1,024 for
  a normal thread, in the kernel's weight table.
- **Windows** runs a `THREAD_PRIORITY_LOWEST` thread only when nothing higher is ready. Its
  anti-starvation boosts do exist, and briefly raise a starved thread.
- **So on a busy 2-core machine the sim can get very little CPU.** It then falls behind (§4.6), and
  `/micperf` shows the lag. The pack's minimum spec must leave some slack (risk R10).

**The potato profile (A11)**, defined once at pack level and used by every perf gate:
- the laptop pinned to 2 **physical** cores. `taskset` picks logical CPUs, and two SMT siblings share
  one core, so the harness picks two CPUs on different cores;
- C2ME and Distant Horizons on;
- FPS capped and FPS uncapped, as two scenarios;
- x86-64 baseline natives (no `x86-64-v3`);
- the integrated server (client and server in one JVM);
- plus runs on a Windows PC.

### 2.2 Native code can feed the sim directly

Observations are Rust traits; today one implementation decodes the bridge's buffer. A native rewrite
(chunk storage, entity tracking, Create's railway graph) can publish a host service the engine reads
directly. The rtsim's coarse pathfinding and region index are host services in turn.

### 2.3 Everything the rtsim touches in other mods is listed (P7)

- **A patch ledger.** Each target mod gets one adapter class holding every mixin and internal access.
  `mods/mic-rtsim/PATCHES.md` lists them (mod, version range, class and member, reason), including the
  puppet fixes (§6.1), taking a creature (§6.2), Surveyor's intermediary names (§7) and the villager
  patches (§8.5). Two mods patching one method at different priorities is a trap already hit in the
  mic_nuclear reviews.
- **Contract tests.** Each adapter has GameTests that pin what it needs (a puppet is never saved, a
  dragon accepts a flight target). An agent rewriting a mod runs them.

### 2.4 Few, cheap hooks into Minecraft

- **One listener per hot event for the whole rtsim**, in the core: damage, deaths, block break and
  place, chunk load and unload, entity join. It rejects most events with a lookup in a primitive set
  before any module sees them. Modules subscribe inside the engine, not on the NeoForge bus.
- **Mixins only where no event exists.** Core: `Entity.shouldBeSaved`, `Entity.saveAsPassenger`, and
  villager restock (§8.5). Ribbits plugin: `RibbitApplyBuffGoal`. Later: `IRadar.getTracks`.
- **Separate mod ids cost almost nothing;** bundling them later is a build change (jar-in-jar).

### 2.5 Shared crates and one harness

- **Shared crates** (MPL-2.0, A7): generational arenas, counter RNG, the timing wheel, the region index,
  the LZ4 + CRC32 page codec (from Deep Time's tile packs), the bench harness.
- **One scenario runner for the whole pack:** fixed world, seed and route; MSPT and frame time at p99;
  GC pauses from JFR; native zone times. The candidate is mc-runtime-test (real client, headless). The
  rtsim's gates are scenarios in it, so a rewrite that eats another system's slack shows up.

## 3. Core language: Rust (A4), and why

A4 is decided. This section keeps the reasoning, corrected by the review, in case phase 0 numbers ever
reopen it.

### 3.1 What makes a modpack feel bad on a weak PC

In singleplayer the server and client are one JVM sharing one heap, one GC and 2 to 4 cores. Players
notice work on the server thread (TPS drops), GC pauses (they stop the render thread too), and threads
that delay the game threads, by taking their core or by delaying safepoints.

### 3.2 The comparison, corrected

| Criterion | Java core | Rust core |
|---|---|---|
| Server-thread cost | boundary only | boundary only |
| GC impact | Sim state lives on the shared heap. Sparse data (memories, relations, caches), every addon in ordinary Java, and every temporary object become traced graphs or garbage. GC frequency rises for the whole process. | Proportional to events applied, bounded by the quotas. The sim's own state never touches the heap. |
| Heap pressure | Counts against `-Xmx`, which the pack already mostly fills. | Native memory counts against RAM, not `-Xmx`. |
| OS thread priority | **Possible:** JNA 5.14.0 ships with Minecraft, so Java can call `setpriority` or `SetThreadPriority`. | Set by the host on every platform. |
| Safepoints | **The real cost.** A Java thread at low priority that is preempted while running Java code delays every stop-the-world safepoint, so the render thread waits for it. | A native thread in native state does not hold up a safepoint. Hence the rule: pool threads never call into Java. |
| Work per simulated thing | C2 auto-vectorizes simple loops. The Vector API is incubating through JDK 25 and needs a launch flag a mod cannot add. Java 21 has no value types (Valhalla's JEP 401 targets a JDK 28 preview). | Ahead-of-time LLVM, structs by value, arenas, SIMD picked per CPU at load (`cpuprobe`). |
| Warm-up | Interpreted, then C1, then C2, while about 190 mods JIT on the same cores. | Full speed from the first call. |
| Parallel safety | A race is a silent desync, found late. | Our own executor runs on the chosen storage (P4). Declared read and write access is checked when the schedule is built. The executor is the one unsafe module. Reductions run in fixed chunk order, so results do not depend on thread timing. |
| Shared with Deep Time and rewrites | Outside the host's pools and governor. | Joins the host (§2). |
| Visible in Spark | yes | no; `/micperf` and Tracy replace it |
| Failure mode on a potato | Frame stutter that grows over a long save. | The sim falls behind its schedule (§4.6). |

**Consequences of "pool threads never call into Java":**
- On-demand climate reads go through the buffers (the bridge samples mic-climate on the server thread
  and writes the values), or the engine reads Deep Time's files natively.
- The engine never upcalls. Everything it needs from Minecraft arrives as an observation.

**Cadence, not language, decides the bulk cost.** A million things updated once per in-game day is
1,000,000 / 24,000 ticks, about 42 rows per tick. The gap is GC, safepoints and warm-up, which Java
cannot fix by working harder. Phase 0 measures the JNI hand-over and the storage bench (P4) on the
potato profile; a contradicting number reopens this section, and Ben is told.

## 4. How it stays cheap

### 4.1 Budgets

These are PLACEHOLDER targets until Ben sets them from phase 0 data (P24).

| Budget | Placeholder | Basis |
|---|---|---|
| Server-thread cost of the bridge | ≤ 1 ms per tick at p99 | 2% of the 50 ms tick |
| Sim objects on the Java heap | none beyond the four buffers | §3.2 |
| Sim threads | none of its own; the host's background pool | §2 |
| Spawns and despawns per tick | a fixed quota (TUNABLE); the rest queue | spreads bursts |
| Blocks placed per tick (growth, later damage) | a fixed quota (TUNABLE) | as above |
| Puppets per player | a cap per body kind (TUNABLE) | client frame time, entity ticking |
| Save hitch | a signal only; copy and IO on the host's IO lane | §4.9 |

### 4.2 Relevance bands and the detail ladder

SCALE.md §2 defines the detail ladder: individual, group, field, flow, aggregate, strategic. On top of
it, every individual sits in one relevance band, which decides scheduling order:

| Band | Who | How it moves | Ladder level |
|---|---|---|---|
| **E: Embodied** | agents with a ticking puppet near a player | Minecraft moves the puppet; the engine shadows it | individual |
| **N: Near** | agents in a player's interest area but not embodied, or about to be | legs at block resolution | individual |
| **F: Far** | notable individuals elsewhere: a robbed caravan, a dragon, a ship | scheduled legs on coarse graphs, closed-form position | individual or group |
| **Abstract** | unobserved traffic and populations | flows on edges (A13), lazy regions (A15) | flow, field, aggregate, strategic |

- **Promotion** follows rising relevance (a player approaches, an event, notability); **demotion** waits
  a little so edge-walking does not thrash. Quantities are conserved (`fold`, `sample`, SCALE.md §2).
- **Entry and exit rates must match** (Sunshine-Hill and Badler 2010), or a road looks full on arrival
  and then empties.

### 4.3 Time: events, closed forms and scheduled encounters

- **Travelling things are not ticked.** A leg has a start time, a path and a speed, so position is a
  formula. The cost is the event at the end of the leg.
- **Regions nobody reads are lazy** (A15, SCALE.md §4). They catch up in closed form when read or at
  breakpoints.
- **Encounters are scheduled, not searched for** (A15):
  - Every leg is rasterized into **one shared region-and-time index**, widened by the detection radius
    of whatever travels it. Dragons, caravans and player interest areas all use the same index.
  - A meeting is booked as an event stamped with **both legs' generations.** When it fires, a stale
    generation on either side cancels it.
  - **When a plan changes, its bookings are cancelled in cascade:** the leg's own bookings, then the
    meetings that depended on them.
- **Periodic work is staggered** by a hash of the row id, so nothing spikes. Veloren thinks for unloaded
  NPCs every 10th tick, staggered by seed (`npc_ai/mod.rs:93-97`).
- **Logic is independent of the timestep.** Chances are hazard rates, `1 - (1 - p)^dt`. Veloren converts
  per-second chances for large dt the same way (`ai/mod.rs:89-110`).
- **Timed events** sit on a hierarchical timing wheel: constant time to schedule and to pop.

### 4.4 Lazy individuals and derived identity

- **A group row holds counts per cohort** and each cohort's mean and spread. Members are latent: no row.
- **A member is derived when it must exist:** identity and looks from `hash(group seed, member index)`
  plus the cohort statistics. Walk away and back, and the same animal or trader comes back.
- **Deaths are handled so identity stays honest:**
  - The group keeps a **monotonic next index** for births, so a new member never reuses an old identity.
  - It keeps a **small dead-index set**, only for members a player has seen.
  - **Cohort deaths are drawn from unseen indices.** A member you saw never quietly dies off-screen and
    comes back.
- **A member becomes notable** and gets its own row only when something makes it matter: a player names,
  tames, breeds, robs or hurts it. Promotion runs under a budget; demotion is by importance.

### 4.5 Threads and the tick boundary

```
 tick t   server thread: [ Minecraft tick ......... ][ bridge: apply cmds(<t), write obs(t), submit ]
          host background pool:                                          [ step to t: E and N first,
                                                                           then due events, write cmds ]
 tick t+1 server thread: [ Minecraft tick ......... ][ bridge: apply cmds(t) ... ]
```

- **Four buffers in all, two per direction,** reused forever. While Java writes one observation buffer,
  the engine reads the other; the same holds for commands. The engine allocates them; Java sees them as
  direct `ByteBuffer`s. Records are fixed binary layouts with no format version before release.
- **Sizing.** Each buffer is sized from the per-player puppet caps and the per-tick quotas, so a normal
  tick always fits.
- **Merging.** Records are merged per agent within a tick (the latest state wins), except deaths, which
  are always kept.
- **Overflow.** A tick that still does not fit spills into an engine-allocated overflow ring. Nothing is
  dropped.
- **Hand-over.** Sequence numbers are read and written through
  `MethodHandles.byteBufferViewVarHandle`, with `getAcquire` and `setRelease` on aligned offsets. One
  JNI call per tick submits the step job.
- **The bridge never waits.** If the step has not finished, the bridge applies nothing new that tick.
  Commands are at least one tick late.
- **Where immediacy matters, Java answers from cached state and reports afterwards.** A trade offer's
  price is computed on the server thread from the market snapshot the engine last sent (§8.5).

### 4.6 Falling behind

- **The governor's budget** is per wall-clock second. When the sim is over it, or when Deep Time or
  another background plugin needs the pool, the engine stretches abstract work, lowest relevance first.
- **Each step runs E and N work first,** so they are the last to fall behind. Under starvation they are
  late too. Puppets then hold their last intent (keep walking to the same waypoint) until new commands
  arrive.
- **Stretched work stays exact where it is closed-form.** It just happens later. Decisions that depend
  on when they ran can differ (§4.7).

### 4.7 Determinism

- **RNG is counter-based,** keyed by (world seed, system, entity, slot). Random processes commit only at
  fixed slot boundaries (SCALE.md §4).
- **Parallel steps are bulk-synchronous.** Cross-region effects are messages applied next step in a
  fixed order. Results are identical at any thread count.
- **The governor's stretch decisions depend on wall-clock load.** So the flight recorder logs each
  step's stretch decisions as inputs, beside the observations. A replay feeds them back.
- **What stays exact without the log:** independent closed-form processes. **What can differ:** a
  decision that ran late under load, such as a caravan departing one step later.
- **The determinism gate:** after N sim days, replaying the same observations and stretch log with 1
  worker and with 8 workers gives the same state hash.

### 4.8 Memory

Row layouts follow SCALE.md §9. Budgets below are layout budgets, with arithmetic.

| Thing | Row | Example count | Memory |
|---|---|---|---|
| Report | 32 B | 1,000,000 | 32 MB |
| Sentiment entry | 8 B (6-byte handle, i8, 1 spare) | | |
| Memory slot | 8 B (6-byte handle, u8 salience, 1 spare) | | |
| Notable person | 96 B + 16 memory slots × 8 B + 16 sentiment entries × 8 B = 352 B | 100,000 | 35.2 MB |
| Herd (group row) | 64 B | 10,000 | 640 KB |
| Latent member | 0 B | any | 0 |
| Market | about 1 KB (ESTIMATE, economy research) | | |
| Active edge-good | about 100 B (ESTIMATE, economy research) | | |
| Forage cell, 64 blocks, Deep Time 16k planet (13,312 × 6,656 blocks → 208 × 104) | 8 B | 21,632 | 173 KB |

- **Every table has a cap** and an eviction rule by importance: player involvement, then recency, then
  magnitude (SCALE.md §6). Caps are sized from the memory budget (A16).
- **Avoid Veloren's inverted cleanup:** its cap drains a max-heap of absolute values, so it deletes the
  strongest sentiments first (`sentiment.rs:78-96`).

### 4.9 Saves and the journal (A9)

**Engine pages:**
- Pages go to `<world>/rtsim/`. A manifest is written last. Global tables are separate from region
  pages. Each page is LZ4 with a CRC32, using the shared codec. Saves are incremental per page (SCALE.md
  §8).
- At Minecraft's save event, the engine snapshots dirty pages at a step boundary (copy on write). The
  host's IO lane compresses and writes them. Veloren clones its whole `Data` on the tick thread every
  60 s (`tick.rs:540-547`); that is what we avoid.

**The journal keeps the sim and Minecraft consistent:**
- The bridge keeps a SavedData named `mic_rtsim:journal` inside Minecraft's level save. (On disk the
  file name needs no colon, which Windows forbids, for example `data/mic_rtsim_journal.dat`.)
- It holds the **last applied command sequence** and **every observation the engine has not yet
  acknowledged.**
- **Engine pages record the observation sequence they include.** On load, the engine replays the
  journal past that point.
- **Every cross-store transfer carries an operation id:** loot dropped from a caravan, coins, packages,
  vault deltas, trades. A replay of the same operation is a no-op.
- **The server thread never waits.** Appending is O(1) in memory. Vanilla writes the SavedData at its
  own save. The engine acknowledges an observation once a page that includes it is on disk, and the
  bridge trims the journal up to that point.
- **What to do when a crash leaves the engine's pages ahead of the world save** is **Proposal P29
  (Ben)**.

**Reboots:** AI plans restart, as Veloren's brains do. Leg schedules, cargo, ship clocks and route
assignments are saved state; Veloren resets its airship timetable on every boot (`migrate.rs:121-187`).
Before release, a format change wipes the save, with no migration (standing rule for unreleased MIC
mods).

### 4.10 Client cost

- **Clients only see puppets** (vanilla entity sync) and, when a phase uses them, map landmarks. No sim
  state is streamed. Puppets are capped per player and per body kind.
- **Later:** a client-only ambient body, if the optimization effort builds one (adapters are not tied
  to `Entity`); far herds and fleets as Distant Horizons render groups
  (`IDhApiCustomRenderObjectFactory`), looks UNVERIFIED.

### 4.11 Measuring it

- `/rtsim perf` is the rtsim's page of `/micperf`. Per-rule cost is tracked there and in CI bench deltas.
- `rtsim-cli`: `run` fast-forwards, `bench scale` is SCALE.md §10's bench, `replay` re-runs a recording,
  `inspect` describes one thing.
- **The flight recorder** logs observations and stretch decisions (§4.7), so game bugs reproduce outside.

## 5. The engine

### 5.1 Crates

| Crate | Role |
|---|---|
| `rtsim-kernel` | clock, timing wheel, lazy regions, storage facade (P4), pages, journal replay, counter RNG, profiler zones. No Minecraft, no JNI. |
| `rtsim-space` | frames, spatial indexes, world-model provider trait (sampled vanilla, Deep Time reader), coarse graphs, hierarchical paths |
| `rtsim-knowledge`, `rtsim-settle`, `rtsim-trade`, `rtsim-travel`, `rtsim-ecology`, `rtsim-conflict`, `rtsim-ships`, `rtsim-industry`, `rtsim-cosmos` | the generic modules (§1.2) |
| `rtsim-jni` | the cdylib loaded by `mic_native`; the only crate that uses `jni`; links `mic-native-rt` |
| `rtsim-cli` | run, bench, history, replay, inspect |
| `rtsim-viewer` | HTML maps of markets, flows and histories, in the style of Deep Time's viewer |

- The Java side is a Gradle multi-project: `neoforge/core` builds `mic_rtsim`, and `neoforge/<mod>`
  builds `mic_rtsim_<mod>` (names P1). It lives in the private repo
  `MinecraftIndustrialComplex/mic-rtsim` (A12), one monorepo of Gradle subprojects (A1).
- Generic building blocks come from the host's shared crates (§2.5).

### 5.2 Address space, built for planets from day one

- **Frames:** universe → star system → body → dimension (one Minecraft `Level`) → province → region
  (512 × 512 blocks, as Minecraft's region files) → chunk → block. Positions are (frame id, local
  coordinates). How many regions make a province is **Proposal P25 (Ben)**.
- **Bodies carry orbital elements.** Travel times between bodies come from orbital mechanics.
- **The strategic tier is multi-body even while only the overworld exists.** Retrofitting it later is
  the classic failure.

### 5.3 Ids that Minecraft sees

- Inside the engine, handles are a 32-bit index plus a 16-bit generation (SCALE.md §9). 64-bit ids exist
  only at the boundary.
- Anything that needs a UUID derives a stable one from (world seed, engine id), for example a later
  Numismatics `BankAccount`.

### 5.4 The module API

Each generic module registers its tables, its systems (each with read and write sets), its event
handlers, its **data schemas** (datapack JSON compiled into engine tables), and the **body kinds** it
needs. Veloren's rules bind handlers to typed events (`Rule::start`), and the handlers run
synchronously when an event is emitted (`lib.rs:305`). Ours queue events and declare access, so the
scheduler can run modules in parallel.

**Ways to extend, most common first:** datapack data (goods, cultures, body kinds, species, ship
classes); behaviour as data (utility curves and action plans compiled into flat tables, no script
interpreter in the hot loop); Rust generic modules, dormant until data or a plugin uses them; and Java
hooks on batched, low-frequency NeoForge events (caravan robbed, village grew, dragon killed). KubeJS
can listen to NeoForge events natively (UNVERIFIED for KubeJS 2101), so no KubeJS plugin is needed.

### 5.5 Agent AI

- **Decisions happen at decision points** (a leg ends, an interrupt, a schedule), never per tick. The
  agent scores a few plans with data-defined utility curves, commits, and the first leg schedules the
  next event.
- **Veloren's brain library is worth copying** (`ai/mod.rs`): actions that return Continue or Break; the
  combinators `then`, `repeat`, `stop_if`, `interrupt_with`, `when_cancelled`; and `choose` at three
  priorities (urgent 100, important 50, casual 0). Only a strictly higher priority replaces the running
  action, which then gets `on_cancel`.
- **Fights nobody watches** use Lanchester laws (square for ranged, linear for melee). **Abstract results
  must never beat observed ones** (X4 community lesson, UNVERIFIED): abstract resolution is tuned
  slightly worse for the stronger side, and checked against real fights in GameTests.

### 5.6 Knowledge: reports, sentiment and gossip

- **A report is a fact:** who did what to whom, where, when, how severe. Never an opinion. It expires by
  kind. Veloren keeps only Death and Theft reports, with lifetimes of 15 in-game days (murder), 5 (other
  deaths) and 1.5 (theft) (`data/report.rs:32-47`).
- **Witnesses** are bodies within a data-defined radius, plus a settlement's awareness of events in it.
  **Sentiment** is bounded and decays faster for weak feelings than strong ones, as in Veloren
  (`sentiment.rs:53-75`). Settlements and caravan owners keep sentiment per actor; players are actors.
- **Gossip moves physically.** Reports travel with caravans, bands and later ships. A village nobody
  trades with learns nothing. Veloren pushes reports into inboxes and syncs every tick; we gossip only
  what changed, and only when a carrier arrives.
- **Caps** follow §4.8; Veloren caps no reports at all.

## 6. The Minecraft bridge

### 6.1 Puppets without duplication

**The engine owns the truth, and Minecraft entities are puppets** (P3). The protocol:

1. **Tagging.** A puppet carries the attachment `mic_rtsim:agent`: engine id and materialization epoch.
2. **A live puppet registry.** The bridge keeps the (id, epoch) of every puppet it spawned this session.
   On `EntityJoinLevelEvent` (cancellable; it reports `loadedFromDisk`), a tagged entity whose
   (id, epoch) is not in the registry is discarded. That catches copies from any path: another mod's
   save, a structure template, a crash.
3. **Puppets are never saved by vanilla.**
   - A mixin on `Entity.shouldBeSaved` returns false for tagged entities.
   - A mixin on `Entity.saveAsPassenger` returns false too. Passengers are saved through their
     vehicle's `saveWithoutId`, which calls `saveAsPassenger` and never asks `shouldBeSaved` (verified
     in the 1.21.1 source).
4. **Handoff keys on ticking, not on loaded chunks** (the Veloren lesson).
   - A puppet appears only where `ServerLevel.isPositionEntityTicking` holds and a player is interested,
     and stays embodied only while it is actually ticking and a player is interested.
   - A chunk kept ticking by a chunk loader, a forced ticket or the spawn chunks (`spawnChunkRadius`
     default 2) keeps no puppet once no player is interested. A puppet in a loaded chunk that has
     stopped ticking is frozen, so the engine takes it back.
5. **Capture before removal.** The body adapter captures what the engine cares about (health, cargo,
   leash state) and sends `Dematerialized(id, epoch, state)`. Veloren re-rolls merchant wares on every
   load (`tick.rs:40-141`); we capture through a per-body adapter instead.
6. **Persistence while embodied.** Vanilla despawns non-persistent mobs beyond 128 blocks, and the pack's
   IaF config has `canDespawn: true`, so every puppet is persistence-required while embodied.
   - **Trap:** a wandering trader's `despawnDelay` of 0 stops its own countdown, but its trader llamas
     copy the trader's delay minus 1 and discard themselves at 0 or below (`TraderLlama.maybeDespawn`).
     The bridge keeps a positive delay on the trader and refreshes it.
7. **Crash safety comes free.** Puppets never reach disk; the journal covers the rest (§4.9).

**Where puppets appear.** At the edge of the entity-ticking area, out of sight. The pack's singleplayer
`options.txt` sets `simulationDistance:12`; a dedicated server defaults to 10. Wandering traders,
trader llamas and villagers have `clientTrackingRange(10)` (chunks). So in singleplayer the edge is
beyond the client's tracking range. On a server at simulation distance 10 the two coincide, and a
puppet may be seen appearing (UNVERIFIED geometry; a GameTest checks it, risk R1).

### 6.2 Taking a creature: it leaves the sim (A10)

| How it is taken | Hook | What the sim does |
|---|---|---|
| A lead | `PlayerInteractEvent.EntityInteract` with a lead on a tagged mob, plus a leash-holder check on each puppet pass (NeoForge 21.1 has no leash event) | the creature leaves the sim; if it belonged to a caravan, a **theft** report |
| Taming | `AnimalTameEvent` | leaves the sim |
| A Carry On pickup | `EntityPickupEvent` (Carry On's blacklist is per entity type, so the event is the hook) | leaves the sim; theft if it belonged to a caravan |
| Capture by another mod | the puppet leaves the level while ticking, not dead, and not removed by the bridge (`EntityLeaveLevelEvent`) | leaves the sim, logged as "removed by another mod" |

- **Leaving the sim** means: the tag is removed, the entity becomes ordinary and is saved by vanilla
  from then on, and the engine removes the agent. Its slot is removed from the flow, so totals stay
  exact (SCALE.md §3).
- A leashed trader llama no longer despawns in vanilla (`canDespawn` checks the leash holder), so the
  stolen llama simply stays.
- **The cargo a stolen llama carries** leaves with it, as a cross-store transfer with an operation id.
- All four hooks go in PATCHES.md.

### 6.3 What the bridge observes

| Observation | Source | Cadence |
|---|---|---|
| Players: position, dimension | server player list | every 20 ticks |
| Chunks loaded and unloaded, forced tickets | `ChunkEvent.Load/Unload`, ticket controllers | on event |
| Puppet ticking state | `isPositionEntityTicking` per puppet | each puppet pass |
| Damage and deaths of puppets and village residents | `LivingIncomingDamageEvent`, `LivingDeathEvent` | on event |
| Trades | `Merchant.notifyTrade(MerchantOffer)` on adopted villagers, sim traders and ribbit merchants | on event |
| Taking a creature | §6.2 | on event |
| Village census while loaded: villagers by profession, ribbits by profession, beds, workstations | entity and POI queries, filtered by village bounds | staggered, while loaded |
| Vanilla gossip from villager hurt or killed | `Villager.getGossips()` read on census | staggered |
| Recipes | `RecipeManager`, exported once per server start and datapack reload | on load |
| Climate (later) | the bridge samples mic-climate or Serene Seasons on the server thread and writes values into the buffer | per cell, staggered |

### 6.4 What the bridge does on command

- Spawn, despawn and steer puppets through adapters, under the per-tick quota.
- Write trade offers and uses for adopted villagers and sim merchants (§8.5).
- Write gossip into villagers, if P19 says so.
- Place structure templates under the block quota, never over player-placed blocks (§8.8).
- Later: Surveyor landmarks, Numismatics balances, Create package offline buffers.

### 6.5 Commands for Ben

`/rtsim perf` (µs per system, band counts, lag), `/rtsim inspect <agent|village|market|edge>` (with
prices and news age), `/rtsim near`, `/rtsim jump <days>`, `/rtsim record start|stop`.

## 7. Reuse table: need, mod, verified hook

Verified means seen in the pinned jar or source (Appendix A). The rest is marked.

| Need | Mod (version) | Verified hook |
|---|---|---|
| Caravan merchant body | vanilla `WanderingTrader` | `Merchant.overrideOffers(MerchantOffers)`, `notifyTrade`; `setDespawnDelay`; `wander_target` NBT |
| Caravan pack animals | vanilla `TraderLlama` | leashed to the trader by `setLeashedTo(trader, true)`; strength 1 to 5; chest inventory 3 rows × strength columns (`getInventoryColumns`) |
| Owning all traders (C8) | vanilla | gamerule `doTraderSpawning` (`GameRules.RULE_DO_TRADER_SPAWNING`), read by `WanderingTraderSpawner` |
| Villager trades through the treasury (C14) | vanilla `Villager` | `getOffers`, `overrideOffers`, `notifyTrade`; `restock` needs a mixin; `MerchantOffer` uses and max uses |
| Reputation, prices and golems | vanilla gossip | `GossipContainer.add(UUID, GossipType, int)`; `GossipType` weights; `updateSpecialPrices(Player)` lowers or raises prices by reputation; `DefendVillageTargetGoal` targets a player with reputation ≤ −100 |
| Ribbit merchants, bands, sorcerers | Ribbits 4.1.6 (LGPLv3, by YUNG; needs YUNG's API and GeckoLib) | `RibbitEntity extends AgeableMob implements GeoEntity, Merchant`; `setRibbitData`; goals GoHome, Stroll, Fish, WaterCrops, PlayMusic, ApplyBuff |
| Ribbit village growth | Ribbits 4.1.6 | 14 house templates under `data/ribbits/structure/houses`; vanilla `StructureTemplateManager` and `StructureTemplate.placeInWorld`. Jigsaw blocks inside the templates must be replaced on placement (UNVERIFIED for these templates) |
| Ribbit village positions | Ribbits 4.1.6 | structure set spacing 25 chunks, separation 8; swamp biomes only; kept 6 chunks from other village sets |
| Vanilla village positions | vanilla | structure set spacing 34 chunks, separation 8 |
| Taking a creature | NeoForge 21.1, Carry On 2.2.6.13 | `AnimalTameEvent`, `PlayerInteractEvent.EntityInteract`, `EntityLeaveLevelEvent`; Carry On `EntityPickupEvent` |
| Puppet safety | vanilla, NeoForge | `Entity.shouldBeSaved`, `Entity.saveAsPassenger`, `EntityJoinLevelEvent` (cancellable), `ServerLevel.isPositionEntityTicking` |
| Dragons | Ice and Fire CE 2.1-beta.1 | entity types, NBT keys, `setFlightTarget(Vec3)`, roost and cave structures |
| Map markers | Surveyor 1.2.4 + Antique Atlas 3.1.2 (listed as "Antique Atlas 4" in the pack) | `WorldLandmarks.of(Level).put(...)`, `Landmark.global`. Surveyor is a Fabric jar loaded through Sinytra Connector 2.0.0-beta.17 (its `neoforge.mods.toml` is a Connector placeholder). Its API uses intermediary names, so the plugin compiles against a remapped copy; recorded in PATCHES.md |
| Ships (later) | Sable 2.0.5 + Create Aeronautics 1.3.2 | `SchematicLoader.loadSchematic` → `container.allocateNewSubLevel(Pose3d)` → `plot.newEmptyChunk` → `template.placeInWorld(plot.getEmbeddedLevelAccessor(), ...)` → `updateLastPose()` |

Later-phase hooks (Numismatics, Create trains and packages, Destroy, Deep Time) are in §1.3 and
Appendix A. mic-grid-loader and create-power-loader stay the option for a base that must really run.

**No mod in the pack covers:** a simulation of unloaded areas, caravans with real cargo between
villages, markets that remember stock, or news that travels.

## 8. Phase 1: caravans and markets between vanilla and Ribbits villages

Decided: C6 to C9, C11 to C14. Ribbits has extra high priority (D5).

### 8.1 What the player gets

- Wandering traders with trader llamas, and ribbit merchants, travel between real villages with real
  goods and the news of what they saw.
- Every adopted village runs a storehouse market: prices follow its stock, labour costs and local
  resources. Humans pay in emeralds, ribbits in amethyst, and exchange rates form at border markets.
- Villager trades draw from and pay into the village treasury, so a poor village cannot pay much.
- Villages and caravan owners remember robbery, killing and leash theft. Fair trade helps.
- Ribbit sorcerers buff friends and leave out those who slighted their village. Touring bands play
  between ribbit villages. Ribbit villages grow from YUNG's own house templates.

### 8.2 Villages in the sim

- **Adoption.** The sim adopts each vanilla village (`StructureTags.VILLAGE`) and each
  `ribbits:ribbit_village` as a market. When a village joins is **Proposal P22 (Ben)**.
- **Where they are.** Vanilla village candidates sit one per 34 × 34-chunk cell (544 blocks), Ribbits
  candidates one per 25 × 25-chunk cell (400 blocks), swamp only, kept 6 chunks from other village
  sets. A candidate can fail its biome check, so real neighbours are often further apart. How many ribbit
  villages a Terralith and Tectonic world has is unmeasured (phase 0 census, risk R6).
- **Terralith's fortified villages are disabled** in `structurify.json`; vanilla villages remain.
- **Residents stay vanilla entities.** Villagers and village ribbits are saved by vanilla as always (C2:
  no civilian bodies). The sim observes them in the census and nudges them through offers, gossip and
  goals. Only travellers are puppets.
- **Edges.** Each market links to nearby markets as flow edges. The starting rule is **Proposal P18
  (Ben)**.

### 8.3 Bodies (C8, C11, C12)

- **Human caravans:** one wandering trader with two trader llamas, as vanilla spawns them
  (`WanderingTraderSpawner` spawns two llamas within 4 blocks). No guards (C12).
- **Ribbit caravans:** ribbit merchants (`RibbitData` profession merchant). Ribbits have no pack
  animals, so a ribbit caravan's capacity is a TUNABLE (P18).
- **The sim owns every wandering trader** (C8). The bridge sets `doTraderSpawning` to false when the sim
  starts. Removing the sim leaves the gamerule false until someone resets it.
- **No mods are added for phase 1** (C11).

### 8.4 The economy (C13)

The model is the storehouse market from the economy research. Summary, with its equations:

**State** (integers): per market a currency, stock `S[g]`, treasury `M`, a production regime per good,
error-diffusion remainders, cached `L[g]`, `Sbar[g]` and `Mbar`, a news ring and its flows. Per edge
and good: a rate, a quoted price, travel time `d`, and goods and money in transit.

**Labour cost and anchors:**
- `L_g = min over recipes r of (l_r + Σ a_ri L_i) / y_r`, with `y_r` capped by the local resource flow
  `F_r`. Cycles are solved once as `L = (I − Aᵀ)⁻¹ l`, linted by Hawkins-Simon. Recipes come from the
  server's `RecipeManager` (§6.3), so modded recipes count and there are no price tables.
- Local resources enter only through `F_r`: crops on observed farmland, trees, fish, ore, geodes,
  depletion. The anchor is `p*_g = L_g / L_mint`, labour cost relative to mining one coin.

**Price is a pure function of stock:** `p(S) = p* (1 + r (1 − S / Sbar))` on `[0, 2 Sbar]`, with target
`Sbar = T × max(c, y)` and consumption `c = pop × need`. Nothing integrates, so nothing drifts.
Production runs at capacity below `Sbar`, matches outflow at `Sbar`, and stops above it.

**Money:**
- **Emeralds for humans, amethyst for ribbits** (C9). Emerald ore spawns only in mountain biomes
  (vanilla; pack worldgen unchecked), so most human villages import money. Budding amethyst regrows
  clusters (economy research), so ribbits near geodes mint their own.
- A market mines while `M < Mbar = k × pop / L_mint`. Above `2 Mbar` it refuses cash and barters at
  anchor ratios.
- **Exchange rates form at border markets.** A border market is one with an edge to a market of the
  other culture. Foreign currency anchors at mint parity `L_mint(foreign) / L_mint(home)` and floats
  within plus or minus its transport cost (the gold points).

**Flows** (A13): on an event, a market allocates its exportable rate to edges whose news margin
`p_j − p_i − τ_ij` is positive, best first, up to the destination's advertised absorbable rate.
`τ_ij = upkeep × d_ij / capacity`. A change reaches `j` after `d_ij`. Money flows back at the quoted
price.

**Caravan capacity:** a human caravan carries 2 llamas × 3 rows × strength columns: 6 slots at strength
1 and 30 at strength 5. Caravans per day = goods rate ÷ capacity. Travel time `d` = path length ÷
caravan speed. The speed is measured in a phase 0 GameTest of a real caravan walking, not guessed.

**Prices travel only as news.** Each breakpoint emits one delayed news event per incident edge. Markets
relay their freshest snapshots at their own breakpoints. There is never a global price board (Gintis:
a live global board destabilizes, stale private news stabilizes).

**Breakpoints and catch-up.** Each good is linear in time within a regime. Breakpoints are `S` or `M`
crossing 0, the target or twice the target; input or deposit exhaustion; scheduled arrivals. Catch-up
walks the breakpoints, so its cost does not grow with elapsed time (A15).

**Conservation.** Per good: Σ stock + in transit + individual cargo = genesis + produced − consumed +
player in − player out; per currency likewise with mined and burned. Asserted per event in debug.

**Stability rule** (derived): `T ≥ 2 × max d` over active edges, so the band between `Sbar` and
`2 Sbar` absorbs the news lag. The tunables `r`, `T` and `k` are **P9 (Ben)**.

### 8.5 Trades through the treasury (C14)

**Villagers in adopted villages:**
- The sim never adds or removes a villager's offers. It keeps the list vanilla and MoreJS made, and only
  reprices it and sets its uses. The pack's `disable_villager_trading.js` removes only armorer,
  librarian, toolsmith and weaponsmith trades, through MoreJS, so farmers, fishermen, shepherds,
  fletchers, butchers, leatherworkers, masons, cartographers and clerics still trade.
- **Price.** Each offer is the integer bundle closest to the market's price, computed on the server
  thread from the last market snapshot: cost `C = ∫ p dS` in closed form, the buyer pays the ceiling and
  the seller gets the floor. No loop of trades makes money.
- **Uses.** An offer where the village sells goods has uses up to its stock. An offer where the village
  pays emeralds has uses up to its treasury. When uses run out, vanilla shows the trade crossed out.
- **Restock.** Vanilla restocks at the workstation. For adopted villages a mixin on `Villager.restock`
  hands uses to the treasury rule instead (PATCHES.md).
- **Report.** Each trade (`notifyTrade`) becomes an observation with an operation id. The bridge updates
  its cached stock and treasury at once, and the engine applies the trade at the next step.
- **Reputation.** Vanilla's own price change from reputation (`updateSpecialPrices`) still applies on
  top. Whether the sim writes into vanilla gossip is **Proposal P19 (Ben)**.

**Ribbit merchants in ribbit villages:** whether their trades go through the ribbit treasury in the same
way (`overrideOffers`, `restock`) is **Proposal P23 (Ben)**.

**Sim caravans:**
- A trader's offers come from its cargo when it materializes: it sells cargo goods and buys what its
  home market lacks, up to the cash it carries.
- Its prices are its destination market's prices, as of the freshest news the caravan carries.
- Whether traders at border markets also exchange emeralds and amethyst for players is **Proposal P21
  (Ben)**.

### 8.6 Caravans

- **Unobserved, a caravan is part of a flow** (A13). No row, no ticks.
- **Near a player,** travellers on the visible stretch are sampled from (edge seed, departure slot):
  identity, cargo from the flow's goods mix, position from departure time and speed. They appear at the
  edge of the entity-ticking area, out of sight (§6.1), and walk the leg with vanilla pathfinding toward
  engine waypoints.
- **At a market,** a caravan stays for a stay window (TUNABLE, P18) near the village's meeting point,
  trades, and leaves.
- **A caravan becomes an individual row** when something must persist: it is robbed, attacked, has a
  llama stolen, or trades with a player. Its departure slot is removed from the flow, so totals stay
  exact.
- **Whether cargo rides physically in the llamas' chests** is **Proposal P20 (Ben)**.

### 8.7 Slights and memory

**What counts, and who remembers:**

| Act | Seen by | Report |
|---|---|---|
| Hurting a trader, llama or ribbit merchant | the caravan and witnesses | assault |
| Killing one | the caravan's owner (its home market) once news arrives; witnesses | killing |
| Taking cargo (robbery) | the caravan's owner | robbery |
| Leashing or picking up a caravan's llama (§6.2) | the caravan's owner | theft |
| Hurting or killing a villager or ribbit resident | the village; vanilla records villager hurt and killed as gossip already | assault, killing |
| A fair trade | the village or caravan traded with | trade (positive) |

- **Whom players wrong in multiplayer** is per acting player for all of these. Signals with no owner,
  such as pollution later, are **Proposal P14 (Ben)**.
- **News spreads at the speed of trade.** A village learns of a robbery elsewhere only when a caravan or
  band arrives from a village that knows.
- **Starting values** (deltas, decay, lifetimes) are **Proposal P18 (Ben)**.

**Consequences, not refusals:**
- **Prices rise** for a disliked player: through vanilla gossip for villagers (P19), and through the
  sim's own quote for traders and ribbit merchants.
- **Iron golems** turn on a player whose vanilla reputation reaches −100 or lower, if P19 writes gossip.
- **Sorcerers leave the player out** of their buffs (§8.8). **Fair trade raises sentiment;** vanilla
  adds trading gossip on each trade.

### 8.8 Ribbits: four behaviours (C7)

**1. Merchant caravans.** Ribbit merchants travel between ribbit villages, and to human villages at
border markets. They price in amethyst; `overrideOffers` sets their offers from their cargo.

**2. Touring bands.**
- A band is a small group of ribbits with instruments in their `RibbitData`, one of them the master. It
  tours ribbit villages as a flow and, when it stops, plays through `RibbitPlayMusicGoal` around its
  `masterRibbit`. How the goal picks its master is UNVERIFIED, so the plugin may need to steer it.
- What a band does in the sim is **Proposal P16 (Ben)**.

**3. Sorcerers who remember.**
- A village's sorcerer ribbits run `RibbitApplyBuffGoal`. It finds players near the sorcerer through
  `Level.getNearbyPlayers` and gives them a random effect from a fixed list (regeneration, resistance,
  strength, jump boost, haste and health boost appear in the bytecode).
- The plugin's mixin filters that player list by the village's sentiment snapshot. A player the village
  dislikes is left out. A friend is included. The thresholds are TUNABLE (P18).

**4. Village growth from YUNG's own templates.**
- Growth places one of the 14 house templates under `data/ribbits/structure/houses` on a free plot at
  the village edge, through vanilla's `StructureTemplate.placeInWorld`, under the block quota, never over
  player-placed blocks.
- New residents are ribbits spawned with `setRibbitData` and homed to the new house. A home-position
  setter is UNVERIFIED (only `getHomePosition` is checked).
- The economy research holds population fixed in phase 1. What drives growth, and when the house
  appears, is **Proposal P17 (Ben)**.

### 8.9 Walkthroughs

Village names here are for the reader. Phase 1 shows no names in game.

**A. A caravan arrives.**
1. You live in Brindle, a vanilla village. No wandering trader ever spawns near you at random.
2. A wandering trader walks in from the east with two trader llamas. It came from Oakford, and it
   appeared out of sight at the edge of your simulation distance.
3. Its offers are what Oakford had too much of, such as wheat and leather. Brindle is short of wheat, so
   the wheat is dear. The trader also buys the iron Oakford lacks.
4. It stays near the bell for its stay window, then walks west toward Mirewater, a ribbit village. Once
   you stop watching, it is part of the Oakford to Mirewater flow again and costs nothing.

**B. Selling to a farmer through the treasury.**
1. Brindle's farmer buys wheat for emeralds. You sell a stack, and the offer pays a little less, because
   Brindle's wheat stock rose.
2. You keep selling. Each emerald comes out of Brindle's treasury. When it is empty, the offer shows
   crossed out.
3. Later an Oakford caravan buys Brindle's surplus wheat and pays in emeralds. The treasury refills and
   the offer reopens, paying more again because the stock fell. Your dumping drained one village, and
   the flows carried the glut outward.

**C. Money from the swamp.**
1. Mirewater's ribbit merchant prices everything in amethyst shards. Mirewater sits near a geode, so its
   ribbits are rich in amethyst and short of bread.
2. Brindle borders Mirewater, so Brindle is a border market. Its stock holds some amethyst from past
   caravans, and its rate between emeralds and amethyst follows both villages' labour costs, within the
   cost of carrying coins between them.
3. A ribbit caravan arrives in Brindle with amethyst and swamp goods, and leaves with bread.

**D. You rob a caravan.**
1. You kill a trader llama on the road. It drops its chest and cargo (P20).
2. The caravan becomes an individual row. Its slot leaves the flow. The loot is a transfer with an
   operation id, so a crash and replay never pays out twice.
3. The trader carries on and goes home to Oakford with the robbery report.
4. Later you visit Oakford. Its prices for you are higher, and if P19 writes gossip and your reputation
   is −100 or lower, its iron golem comes for you. Mirewater has not heard yet; it learns only when a
   caravan from a village that knows arrives there.
5. You trade fairly in Oakford for a while. Its opinion of you recovers slowly.

**E. You steal a llama.**
1. A caravan rests at Brindle's bell. You put a lead on one of its llamas.
2. The llama leaves the sim and becomes your ordinary llama (A10). Its cargo goes with it.
3. The caravan reports a theft. Its owner, Oakford, remembers it like a robbery.

**F. A sorcerer remembers.**
1. You visit Mirewater after trading fairly with ribbit caravans. Its sorcerer gives you regeneration.
2. Your friend robbed a ribbit caravan last week, and the news has reached Mirewater. The sorcerer casts
   right next to them and leaves them out.
3. Your friend trades fairly at Mirewater for a while. The sorcerer starts including them again.

**G. A band comes to town.** A few ribbits with instruments walk into Mirewater on their circuit of
the swamp, gather around their master and play. They bring the news from their last stop (P16).

**H. Mirewater grows.** Mirewater trades well all season. You come back after a long trip, and a new
ribbit house from YUNG's templates stands at the edge of the village, with a new ribbit in it (P17).

### 8.10 Phase 1 exit tests, in game

1. With the sim on, no wandering trader spawns at random; caravans arrive from real neighbouring
   villages, with offers that match their origin's surplus.
2. Walkthroughs A to H work on a test world with at least two vanilla villages and one ribbit village.
3. Selling to one villager drains its treasury, and the offer recovers after a caravan visits.
4. A robbery reaches a second village only after a caravan carries it there (timed in a GameTest).
5. A save and reload mid-caravan, and a kill -9 mid-caravan, give no duplicate trader or llama and lose
   no cargo (puppet and journal GameTests).
6. Conservation holds: goods and money totals match after a soak with scripted players.
7. Perf gates (P24) are green on the potato profile, capped and uncapped, and on the Windows PC.
8. The 1-worker and 8-worker replays of the same flight recording give the same hash.

## 9. Phase 2: dragons on Ice and Fire (C3)

**Model:**
- **A dragon is a notable individual** (a row): species (fire, ice, lightning), age, hunger, lair,
  territory, and sentiment toward players.
- **Lairs** are Ice and Fire's roost and cave structures, found by structure lookup.
- **Hunger** drives hunting legs (ecology module). The pack config sets `hungerTickRate` 3000,
  `targetSearchLength` 128 and `wanderFromHomeDistance` 40.
- **Migrations run on scheduled legs** (A15). A leg is booked into the region-and-time index at its
  start. A player entering a region looks up the legs booked there, so a world-spanning migration still
  crosses your house as a scheduled event, with no scans.
- **Prey.** Over an edge, a dragon is a hazard rate on the caravan flow: losses = rate × hazard,
  integrated in closed form (SCALE.md §3). What else dragons prey on in phase 2 is **Proposal P12
  (Ben)**.
- **Memory.** A dragon remembers a player who hurt it, and its next hungry season targets that player's
  home. What "home" means is **Proposal P26 (Ben)**.

**Minecraft side** (`mic_rtsim_iceandfire`):
- The dragon materializes as an IaF entity whose NBT comes from engine state (`AgeTicks`, `Hunger`,
  `Variant`, `Gender`, `HomeAreaX/Y/Z`). It is persistence-required.
- `IafDragonFlightManager.setFlightTarget(Vec3)` steers it. A goal picks targets using IaF's
  `dragon_targets` tag. Whether IaF dragons carry prey off is UNVERIFIED.
- The pack has dragon griefing off (`griefing: false`). What happens to buildings is **Proposal P28
  (Ben)**.
- **The pack's In Control spawner** spawns `iceandfire:fire_dragon` in the overworld (`persecond` 0.1,
  `maxthis` 200), with no biome condition. The baseline called it a desert spawner; that was wrong.
  Whether phase 2 removes it is **Proposal P13 (Ben)**.

**Walkthrough:**
1. Winter. Cinderjaw, a fire dragon in a roost east of Brindle, is hungry. Its migration leg south
   crosses the Oakford to Brindle road.
2. Nobody watches the road. The caravan flow loses llamas and cargo at the hazard rate. Oakford's
   prices for leather and wool rise as fewer caravans arrive, and the news spreads with the survivors.
3. You are on that road when the leg crosses it. Cinderjaw appears at the edge of the entity-ticking
   area, out of sight, and flies at the caravan in front of you.
4. You drive it off wounded. Its health is captured when it leaves. It remembers you, and next hungry
   season it hunts near your home.
5. You kill it. Brindle and Oakford, whose caravans it preyed on, think better of you (a positive report
   carried by caravans).

**Phase 2 exit tests:** the walkthrough works; a migration leg booked across the player's region fires
on time with no scan; a dragon's health and memory survive dematerialization and a restart; a killed
dragon is not silently replaced (if P13 removes the spawner rule).

## 10. Later content, in a proposed order (P11)

Each module below is sketched with its hooks and a short walkthrough. Bodies and structures for each are
**P8 (Ben)**.

### 10.1 Ships (C5)

- **Sky ships are timetabled as in Veloren:** position is a function of the clock, so unloaded and loaded
  ships agree without physics. Each ship's clock offset and route assignment are saved, unlike Veloren,
  which resets them on every boot. While unloaded, cruise altitude is forced (Veloren `!5091`: ships that
  were flying low got stuck on load).
- **Sky pirates** hunt sky lanes; **sea ships** run between port villages. Pirates are hazard rates on
  lanes until observed (SCALE.md §3).
- **Body candidates** (P8): Small Ships, Create Aeronautics Structures, the Seven Seas add-on (not yet
  surveyed). **How ports are provided** is also P8.
- **Hooks:** Sable's five-call spawn recipe; `SubLevelHoldingChunkMap` freezes unloaded ships, so the
  engine owns every NPC voyage. Saving a sub-level plot back to a template is UNVERIFIED; without it, hull
  damage persists as a number instead of holes.
- **Walkthrough:** a sky ship from Brindle's dock to a mountain village passes over your base at the hour
  its timetable says. You board it at the dock and ride. A pirate ship appears on a lane you are
  crossing; you fight it with real Create cannons, or flee. The lane's hazard rises in the news, and
  fewer caravans take it.

### 10.2 Ecology and herds

- Species as data; forage fields regrowing from climate; herds as group rows with cohorts; migration on
  scheduled legs; predation by players, wolves and dragons (SCALE.md §2, §4). The calendar is **P2
  (Ben)**. Naturalist is a body candidate (P8); vanilla animals are the fallback.
- **Walkthrough:** a flock drifts north-east up your valley in late spring. As its footprint enters your
  loaded chunks, sheep and lambs appear at the far edge, all moving the same way. You kill six; the count
  drops by six. You wall off the pass; next autumn the flock's puppets fail to pass, the route cost rises,
  and the year after it takes the long way round. The dragon that follows it goes hungry and turns to
  caravans.

### 10.3 Conflict, goblins and raiders

- Grievance per player per signal; escalation from notice to raid; war bands as rows on the road graph;
  a raid director that spawns waves at the edge of the entity-ticking area. Raids on player bases resolve
  only while a player of that base is online (TUNABLE behaviour).
- Body candidates (P8): Goblin's Tyranny, Illager Invasion. The fallback raider is `minecraft:pillager`.
  Pollution signals come from `mic_rtsim_destroy`; attribution is **P14 (Ben)**.
- **Walkthrough:** your Destroy refinery smogs the forest a goblin camp claims. Scouts appear at the
  forest edge. Ignore them, and a war band musters and marches down the trail over days. You are online
  when it arrives; waves come at the edge of your loaded area at night. You break it, and the human
  village downriver, which hates the goblins, sends its next caravan with better prices.

### 10.4 Industry, on the Create plugin

- **Unit:** one Create 6 logistics network (`create_logistics`), **P5 (Ben)**. While loaded, the engine
  snapshots factory gauges and measures their rates. While unloaded, each gauge is a process between
  breakpoints. Trains keep running, because Create ticks overworld trains with no chunk check. Remote
  cargo moves as packages through `GlobalPackagePort.offlineBuffer`.
- **Walkthrough: selling iron to a town by train.**
  1. A village with a Create station buys iron at its market (it needs a station; how villages get one is
     P8).
  2. Your train runs there while you sit at home. Create's mail transfer drops your iron packages into
     the station's offline buffer.
  3. The engine's market reads the buffer at its next step, prices the iron, and puts packages of the
     grain you ordered into the same buffer.
  4. **The engine's reply is at least one tick late,** so the grain rides the next train. Or your
     schedule waits at the station on a cargo condition until the grain is aboard (Create's item cargo
     condition; class name UNVERIFIED).
  5. Your account or your next train's cargo shows the trade. The village, with steady iron, grows.

### 10.5 Kaiju (C10)

- Late-game bosses that start appearing once players have nukes. The trigger comes from `mic_nuclear`
  and Create Nuclear, **P27 (Ben)**. The body candidate is the Alex's Caves port (P8).
- A kaiju is a notable individual on scheduled legs, like a dragon, with hazard rates on everything it
  crosses.
- **Walkthrough:** the first detonation in the world is observed. Weeks later, news arrives with a
  caravan that something huge has risen in the far ocean. Its leg crosses a sea lane; ships stop
  arriving at a port village, and prices there climb. Later its leg is booked across your region.

### 10.6 Space and planets (SCALE.md §12, A14, A17)

- The pack has no space mod; choosing one is Ben's decision when the time comes.
- **Planets:** an unvisited planet runs a cheap coarse Deep Time history that is authoritative; a visit
  runs only the refinement (A14). A planet is baked in the background only when free RAM allows, peaking
  at 3.2 to 4.2 GB; otherwise it stays "still forming" (A17). A background bake takes 30 to 45 min on a
  player's machine. A full world is about 100 MB of terrain data per 16k planet with the timelapse
  dropped.
- **A megaproject is a strategic process,** for example a self-replicating swarm turning Mercury into
  collectors. Its growth is limited by:
  - mass: Mercury is 3.30 × 10²³ kg;
  - energy: 1,361 W/m² at 1 AU ÷ 0.387² = 9,087 W/m² at Mercury's semi-major axis. With eccentricity
    0.2056 the flux runs from 6,249 W/m² at aphelion to 14,394 W/m² at perihelion (figures from the
    2026-10-19 review, with a = 0.3871 AU). Averaged over the orbit, S ÷ (a² √(1 − e²)) gives about
    9,286 W/m². Output = flux × collector area × the design's efficiency;
  - the replication time of the design the player researched.
- **Walkthrough:** you launch the first replicator. The strategic row grows in closed form; nothing ticks.
  Months later, power beamed home arrives at a receiver on your grid. It rises and falls between the
  aphelion and perihelion figures above, on whatever orbital period the space mod gives Mercury.

### 10.7 History generation (P6)

- At world creation, after Deep Time's bake when one is present, the abstract levels of every module
  run with large steps over N years. The world the player walks into is in the state those rules produce.
- History runs on the host's foreground pool, because the player is waiting (X1).
- It writes to Deep Time's event log as writer 2 (`civ:history`), with negative years before world
  creation (X2). History and refinement stay separable, and the history grid resolution stays a preset
  parameter (A14).
- **Walkthrough:** you open `rtsim-viewer` on a new world and see which villages traded, which caravan
  routes were robbed, and which roosts held dragons. In game, the villages' first prices and opinions
  already carry that history.

### 10.8 Civilians and villagers (C2)

Not now. Residents stay vanilla entities. Any future civilian body must look like a mob (D7).

## 11. Deep Time

- **The plugin** `mic_rtsim_deeptime` provides the world model in Deep Time worlds: terrain, water and
  rivers (`WorldData`, `Rivers`), monthly climate and Köppen, resource deposits (`DeepTimeSites`). Pool
  threads read Deep Time's files natively (§3.2).
- **The world is finite** (A3): 13,312 × 6,656 blocks at 16k Petroff-Guyou, or 26,112 × 13,056 at 32k.
- **The event log:** writer 2 (`civ:history`), `t` in years relative to world creation, negative before
  it (X2). Deep Time's viewer and timelapse then show the chronicle.
- **The format** `deeptime-format` (world tiles, event log, LZ4 + CRC32 codec) is LGPL and already
  exists. The rtsim reads it and appends to it.
- **Natives** move into `mic_native` (§2). Deep Time and the rtsim share threads, not one library;
  Deep Time can still become an engine module later (A2). **Evolution, later** (A3), uses the ecology
  module's species format.
- **Bake timing** (SCALE.md §12): a 16k planet over 1,000 Myr takes 826 to 955 s on the laptop with 8
  threads and 1,290 s on the quiet VM; Deep Time's target is 900 s on an average gaming rig. History adds
  to that wait.

## 12. Licence

- **The rtsim engine and content are AGPL-3.0** (A2). AGPL's network clause means a server that runs a
  modified rtsim must offer its players the source of that modification.
- **The host and the shared crates are MPL-2.0** (A7). Optimized forks of other people's mods link the
  host. MPL's copyleft is per file, so those forks keep their own licence (Create is MIT), and
  GPL-2.0-only code is not blocked. It also works with Rust's static linking, where LGPL's relinking
  terms are awkward.
- MPL-2.0 crates can link into Deep Time's LGPL-3.0-or-later cdylib once Deep Time's `cargo deny`
  allowlist adds MPL-2.0. Ben owns both, so any choice works between those two.
- Ribbits is LGPLv3 and Surveyor LGPL-3.0-or-later. The plugins ship none of their code.

## 13. Custom-roll ledger

Sizes are relative: S < M < L < XL.

| Item | Size | What it is |
|---|---|---|
| `mic_native` host | L | loader and natives moved from Deep Time, two pools, IO lane, governor, counting allocator, profiler and `/micperf`, service registry, `mic-native-rt` with C ABI rules |
| Patch ledger + contract tests | S | PATCHES.md and one GameTest set per adapter (P7) |
| Engine kernel | XL | clock, timing wheel, lazy regions, storage facade, pages, counter RNG, determinism tests, scale bench |
| Journal | M | `mic_rtsim:journal` SavedData, operation ids, engine replay, acknowledgement and trimming |
| JNI boundary and core bridge | L | four buffers, overflow ring, VarHandle hand-over, quotas, puppet registry, the two save mixins, taking-a-creature hooks |
| Body-kind mapping | S | data loader, candidate resolution, the vanilla adapters |
| Travel and flows | L | edges, legs, sampled travellers, the shared encounter index, cascade cancellation |
| Knowledge | M | reports, witnesses, sentiment with caps and decay, gossip by carrier |
| Settlements | M | adoption, census, plots, growth |
| Economy | L | labour-cost solver from recipes, storehouse markets, currencies, exchange, flows, news, conservation asserts |
| Treasury adapter | M | offer repricing, uses, restock mixin, gossip writer (if P19) |
| `mic_rtsim_ribbits` | M | merchant and band adapters, the buff-goal mixin, house placement |
| `mic_rtsim_carryon` | S | one event listener |
| `mic_rtsim_iceandfire` + dragon rules | M | dragon adapter, flight steering, hunting legs, migration bookings |
| `mic_rtsim_surveyor` | S | landmark writer against a remapped Surveyor |
| CLI, bench, replay, viewer | M | headless runs, potato benches, flight recorder, HTML maps |
| Later: ecology L, conflict L, ships L, industry on `mic_rtsim_create` L, `mic_rtsim_destroy` S, `mic_rtsim_numismatics` S, history XL, `mic_rtsim_deeptime` M, cosmos XL, kaiju M | | |

**Removed from the baseline:** the goblin entity, the custom humanoid and its TaCZ body work, the
dialogue screen, the Cobblemon adapters. **Data is the hidden bulk:** goods from item tags, cultures,
body-kind files, later ship schematics and species.

## 14. Phases

Each phase ends with something Ben can see in game and with the perf gates (P24) green on the potato
profile.

| Phase | Builds | Exit test |
|---|---|---|
| **0. Spikes** | the first `mic_native` (pools, IO lane, governor, C ABI rules); the potato harness with a whole-pack baseline before any sim exists; `rtsim-cli bench scale` (SCALE.md §10); puppet GameTests (no duplicates across save, reload, crash, passengers and other mods' saves); journal GameTests (crash at each point of the save order); the storage bench, `bevy_ecs` against our own columns (P4); a caravan speed GameTest; a ribbit-village census on the pack's worldgen | numbers written into this plan and brought to Ben; the Rust choice holds against them, or §3 is reopened |
| **1. Caravans and markets** | kernel, core bridge, journal, body kinds, knowledge, settlements, trade, travel, the treasury adapter, `mic_rtsim_ribbits`, `mic_rtsim_carryon` | §8.10 |
| **2. Dragons** | ecology (hunger), `mic_rtsim_iceandfire`, migration bookings, flow hazards | §9 exit tests |
| **Later** | in the order P11 settles: ships, herds, conflict, industry, kaiju, space, history | each module's walkthrough works in game |
| **With Deep Time** | `mic_rtsim_deeptime`, the event-log writer, the natives move (§2) | a Deep Time planet's history appears in Deep Time's viewer |

**Tests at every phase:** `cargo test` with property tests for conservation, `fold` after `sample`, and
the composition rule; deterministic replay with the stretch log; GameTests for every adapter; one smoke
test on the real pack server. Builds and tests run on the laptop, niced, one heavy job at a time.

## 15. Risks

1. **Puppet bookkeeping.** Duplicates or losses at chunk edges, crashes, passengers or another mod's
   saves; puppets seen appearing on servers at simulation distance 10. Phase 0 writes the GameTests
   first.
2. **Journal edge cases** (P29). Phase 0 crashes the server at each point of the save order.
3. **Economy drift** (Veloren: `MINIMUM_PRICE` is hit for almost every good, `replenish` creates stock,
   `check_money` is commented out, and towns grew without bound in issue #2034). Ours anchors prices to
   labour cost, conserves exactly, and tests for leaks, pumps and cobwebs (economy research §2).
4. **Abstract and observed results diverge.** Tuned against real fights in GameTests.
5. **Other mods' internals are not APIs.** Villager restock, Ribbits goals, Create fields and Surveyor's
   intermediary names can change. Adapters stay small, version-checked and in PATCHES.md.
6. **Too few ribbit villages** in Terralith and Tectonic swamps. The phase 0 census measures it.
7. **Deep Time's format is pre-release** and exact-match. The provider sits behind a trait.
8. **History time on a weak PC** adds to world creation. Benched before history ships.
9. **Optimization agents change the mods adapters depend on.** PATCHES.md and contract tests catch it.
10. **No slack on the weakest PCs** (§2.1). The sim falls further behind; `/micperf` shows it. The pack's
    minimum spec must leave some slack.

## 16. What the prior art taught

Veloren lessons come from the source-level research. Lessons marked "baseline" come from the first
research pass and were not re-read; their numbers have been dropped.

| Lesson | Source | Where applied |
|---|---|---|
| Flat id-keyed tables; binary save with field names and defaults; atomic writes off the tick thread; names from seeds | Veloren `data/mod.rs`, `actor.rs:531-539` | §4.9, §4.4 |
| Action combinators with priority `choose`; unloaded brains staggered by seed; parallel brains on a frozen snapshot | Veloren `ai/mod.rs`, `npc_ai/mod.rs` | §5.5, §4.3 |
| Timetabled airships, but save the clock offset and route; force cruise altitude while unloaded | Veloren `airship_travel.rs`, `migrate.rs`, `simulate_npcs.rs` | §10.1 |
| Bounded, decaying sentiment and reports with lifetimes, but capped, and never deleting the strongest feelings first; refill populations after a delay | Veloren `sentiment.rs`, `report.rs`, `architect.rs` | §5.6, §4.8 |
| Handoff by ticking, not loaded chunks; persist cargo; carry real goods | what Veloren lacks | §6.1, §8.6 |
| Prices from stock against cost anchors; conserve money; no global price board | economy research (Doran and Parberry, bazaarBot #17, Veloren economy, Gintis) | §8.4 |
| Sleep and wake with merged wake lists | Factorio Friday Facts (baseline) | §4.3 |
| Parallel phases that do not depend on order, then a serial apply | CK3 dev diary, Dyson Sphere Program dev log (baseline) | §4.7 |
| Abstract flows, physical only when observed | Dwarf Fortress world activities (baseline) | §4.2 |
| Generate detail lazily but consistently; match entry and exit rates | Sunshine-Hill and Badler 2010 (baseline) | §4.2 |
| A pop is a group by design | Victoria 3 (SCALE.md §2); Stellaris pop groups UNVERIFIED | §4.4 |

**Traps that hurt similar projects:** whole-population scans every tick (Veloren's `sync_npcs`,
`cleanup`, AI selection, `simulate_npcs` and load check); unbounded history (Dwarf Fortress figures,
RimWorld pawns; baseline); lossy handoff (Veloren re-rolls merchant wares); village life tied to loaded
chunks (MineColonies, Millénaire; baseline); unmeasured per-rule cost (CK3; baseline, number dropped).
Our answers: events and scheduled encounters, caps and compaction, per-body capture, an engine that
ignores what is loaded, and `/rtsim perf` with CI bench deltas.

**Veloren's performance numbers:** none upstream. One fork reports about 0.2 ms per 1,000 simulated
NPCs (second-hand, UNVERIFIED).

## 17. Proposals waiting for Ben

P1 to P10 are already in DECISIONS.md; they are restated briefly with what this plan adds. P11 onward
are new. Each has a recommended option first.

| # | Proposal | Recommended | Alternatives | Needed by |
|---|---|---|---|---|
| P1 | Names and the split | `mic-rtsim`; mod ids `mic_rtsim` and `mic_rtsim_<mod>`; plugins carry Java and data only, rules live in generic modules | other names; plugins that may carry their own Rust rule crates | phase 0 |
| P2 | Sim calendar | Serene Seasons' calendar (8-day sub-season by default), or mic-climate's `YearClock` in Deep Time worlds | the vanilla day count only | herds, growth |
| P3 | The engine owns the truth; puppets never saved by vanilla | yes, as §6.1 (A9 and A10 already assume it) | vanilla saves entities and the engine reconciles | phase 0 |
| P4 | Agent storage | settled by the phase 0 storage bench: `bevy_ecs` behind a facade with our own executor, or our own columns | | phase 0 |
| P5 | Factory unit | one Create 6 logistics network; remote cargo as packages | per-machine or per-chunk units | industry |
| P6 | History timing | at world creation, after Deep Time's bake when present | lazily, per region on first visit | history |
| P7 | Patch ledger and contract tests | yes, for every reach into another mod; every MIC mod keeps one | rtsim only | phase 0 |
| P8 | Bodies, structures and ports for later phases | candidates: Small Ships, Create Aeronautics Structures, the Seven Seas add-on (ships, ports); the Alex's Caves port (kaiju); Goblin's Tyranny (goblins); Illager Invasion (raiders); Naturalist (herds) | vanilla bodies only; abstract only | each later phase |
| P9 | Economy tunables | `r`, `T` above its bound `2 × max d`, `k`; a Numismatics mint rule later | | phase 1 design gate |
| P10 | The design-changing review fixes | approve the list below as applied | item by item | phase 0 |
| P11 | Order of later phases | ships, ecology and herds, conflict, industry, history, kaiju, space; civilians only when Ben asks (C2) | herds first (they feed dragons and conflict), then ships; industry first | after phase 2 |
| P12 | What dragons prey on in phase 2 | caravan flows, plus village livestock counted as market stock; wild herds wait for ecology | a minimal wild-herd model in phase 2; caravans only | phase 2 |
| P13 | In Control fire-dragon rule | remove it in phase 2 so the sim owns every dragon | keep it and adopt each spawned dragon into the sim on join; keep it and leave untagged dragons alone | phase 2 |
| P14 | Multiplayer attribution of unowned signals | direct acts per acting player; unowned signals such as pollution go to one shared "settlers" actor | add a claims or teams mod and attribute to the team; attribute each chunk to the player with most time or blocks there | conflict |
| P15 | Body-kind format | datapack JSON at `data/<ns>/mic_rtsim/body_kind/<kind>.json`: ordered candidates (entity type, required mod, NBT template, adapter); first installed wins; vanilla last; no candidate means the kind stays abstract | server config TOML; fixed in each plugin's code | phase 1 |
| P16 | Touring bands | bands tour ribbit villages as a flow, carry news like caravans, and play at each stop | also tour border human villages; also raise the host village's sentiment toward the band's home village | phase 1 |
| P17 | Ribbit village growth | a market breakpoint: when stock and treasury stay above target for a TUNABLE time, the village books one house; it is placed when its chunk next loads, under the block quota, never over player blocks (at worst it appears at the edge of view distance); population steps up at that breakpoint, so markets stay piecewise-linear | houses only, population fixed in phase 1; defer growth to a later phase | phase 1 |
| P18 | Phase 1 starting values | sentiment on vanilla's gossip scale (`GossipType` weights; villager hurt +25 minor negative, killed +25 major negative, trade +2), shared by ribbit villages and caravan owners; report lifetimes from Veloren (15, 5 and 1.5 in-game days); caravans of 1 trader and 2 llamas (vanilla); edges from Veloren's rule (4 nearest within 400 chunks); the rest TUNABLE and set from a soak at the design gate: ribbit caravan capacity, stay window, buff thresholds, growth threshold, path inefficiency, labour per recipe | Ben sets each by hand; plain placeholders | phase 1 design gate |
| P19 | Sim sentiment and vanilla gossip | the bridge writes village sentiment into nearby villagers' gossip, so vanilla prices and iron golems carry it; vanilla gossip is read back | the sim applies per-player prices itself and leaves gossip and golems alone; read gossip, never write it | phase 1 |
| P20 | Cargo in llama chests | a materialized caravan's llamas carry its sampled cargo in chests (3 × strength slots each); killing a llama drops it; robbery is a transfer with an operation id | cargo stays abstract and a killed trader drops a loot table scaled by cargo; cargo shown only as offers | phase 1 |
| P21 | Currency exchange for players | traders and ribbit merchants at border markets offer emerald and amethyst trades at that market's rate | no player exchange in phase 1; exchange through Numismatics later | phase 1 |
| P22 | When a village joins the sim | when its structure start is generated; caravans run between generated villages | precompute every village inside the world border at world creation, as `/locate` does, so caravans come from unexplored places; only villages a player has visited | phase 1 |
| P23 | Ribbit merchant trades | through the ribbit treasury, like C14 | leave Ribbits' own trades and restock alone | phase 1 |
| P24 | Performance and scale targets | keep §4.1's placeholders (bridge ≤ 1 ms p99; no heap objects beyond the buffers); pool sizes cores − 2, minimum 1; scale bench ranges by orders of magnitude (SCALE.md §10); Ben sets final numbers from phase 0 data | Ben sets them now; relative targets only (no measurable MSPT change against the no-sim baseline) | phase 0 |
| P25 | Regions per province | chosen from the phase 0 bench among 4 × 4, 8 × 8 and 16 × 16 regions | Ben picks now | phase 0 |
| P26 | A player's home, for targeting | the region where the player spent the most time recently (a decaying counter from position observations) | the respawn point; a claims mod | phase 2 |
| P27 | Kaiju trigger | the first nuclear detonation observed in the world | the first device crafted; the first reactor reaching criticality (hooks UNVERIFIED) | kaiju |
| P28 | Dragon damage with griefing off | no block damage; consequences through stock, prices and caravans | bounded engine-placed fire per raid; turn IaF griefing on | phase 2 |
| P29 | Engine pages ahead of the world after a crash | keep the previous page generation until Minecraft's save completes; on load use the newest generation not ahead of the journal | write engine pages only after Minecraft's save completes; accept rare loss | phase 0 |

**P10: the review fixes applied in this plan (2026-10-19).** Already decided: the journal with
operation ids (A9, §4.9), taking a creature leaves the sim (A10, §6.2), and the potato profile (A11,
§2.1). Waiting for Ben: the live puppet registry and the `saveAsPassenger` mixin, ticking-based handoff
and the "edge of the entity-ticking area" wording (§6.1); E and N run first rather than "never
stretched" (§4.6); the corrected 2-core slack argument (§2.1); stretch decisions in the flight recorder
(§4.7); the IO lane in place of a writer thread, four buffers, and no "one native library" phase (§2,
§4.5, §14); buffer sizing, merging, overflow ring and VarHandles (§4.5); encounter generations, one
shared index and cascade cancellation (§4.3); identity after deaths (§4.4); row budgets (§4.8);
attribution as P14; train cargo on the next train (§10.4); Surveyor through Connector (§7); Mercury's
flux (§10.6); the licence notes (§12); unsourced prior-art numbers dropped (§16); and the language
corrections (§3.2).

## Appendix A. Verified facts this plan relies on

Checked on 2026-10-19 against the pinned jars (`mods/deep-time/smoke/mic/server/mods/`), the mapped
NeoForge 1.21.1 source in the Gradle cache, Create's `mc1.21.1/dev` source, the pack files, or the local
source trees. Facts from the 2026-10-19 review or another session say so.

- **Vanilla 1.21.1** (mapped source):
  - Gamerule `doTraderSpawning` (`GameRules.RULE_DO_TRADER_SPAWNING`, default true) gates
    `WanderingTraderSpawner`. It checks every 1,200 ticks, tries every 24,000 ticks at 25% chance rising
    by 25 to 75%, spawns two trader llamas within 4 blocks leashed to the trader, and sets
    `setDespawnDelay(48000)`.
  - `TraderLlama.maybeDespawn` copies its trader's despawn delay minus 1 and discards itself at 0 or
    below; it never despawns when tamed, leashed to anything but a trader, or ridden by one player.
    `Llama` strength is 1 to 5; `getInventoryColumns()` is the strength when chested.
  - `Merchant` declares `getOffers`, `overrideOffers(MerchantOffers)` and `notifyTrade(MerchantOffer)`;
    `AbstractVillager` implements them.
  - `Villager.updateSpecialPrices(Player)` shifts each offer by reputation × price multiplier; Hero of
    the Village lowers prices further. `Villager.getGossips()` is public; `GossipContainer` has
    `add(UUID, GossipType, int)`, `remove` and `getReputation`.
  - `GossipType` (weight, max, decay per day, decay per transfer): major_negative (−5, 100, 10, 10),
    minor_negative (−1, 200, 20, 20), minor_positive (1, 25, 1, 5), major_positive (5, 20, 0, 20),
    trading (1, 25, 2, 20). Villager hurt adds minor_negative 25, killed adds major_negative 25, a trade
    adds trading 2. `DefendVillageTargetGoal` targets a player whose reputation with a nearby villager
    is ≤ −100.
  - `Entity.saveAsPassenger` writes the id and calls `saveWithoutId` without checking `shouldBeSaved`.
  - `ServerLevel.isPositionEntityTicking(BlockPos)` exists; gamerule `spawnChunkRadius` defaults to 2;
    wandering traders, trader llamas and villagers have `clientTrackingRange(10)`.
  - Village structure set: spacing 34 chunks, separation 8.
- **NeoForge 21.1:** `EntityJoinLevelEvent` is cancellable and reports `loadedFromDisk`;
  `EntityLeaveLevelEvent`, `AnimalTameEvent`, `PlayerInteractEvent`, `ChunkEvent`,
  `LivingIncomingDamageEvent` and `LivingDeathEvent` exist. There is no leash event.
- **Carry On 2.2.6.13:** `tschipp.carryon.events.EntityPickupEvent(ServerPlayer player, Entity target)`,
  cancellable, posted on `NeoForge.EVENT_BUS` before an entity pickup. Its blacklist is per entity type
  (review).
- **Ribbits 4.1.6** (NeoForge 1.21.1, LGPLv3, by YUNG; needs YUNG's API and GeckoLib):
  - `RibbitEntity extends AgeableMob implements GeoEntity, Merchant`, with `getOffers()`,
    `overrideOffers(MerchantOffers)`, `restock()`, `getHomePosition()`, and
    `RibbitData(profession, umbrellaType, instrument)` through `setRibbitData`.
  - Professions: nitwit, gardener, sorcerer, fisherman, merchant. Trades priced in amethyst:
    `AmethystForItems`, `ItemsForAmethysts`, `ItemsAndAmethystsToItems`, `EnchantedItemForAmethyst`.
  - Goals: GoHome, Stroll, Fish, WaterCrops, PlayMusic (a band around a `masterRibbit`), ApplyBuff.
    `RibbitApplyBuffGoal` finds players with `Level.getNearbyPlayers` and applies a random effect with
    `Player.addEffect`, with a buff cooldown.
  - `ribbits:ribbit_village`: 14 house templates under `data/ribbits/structure/houses`; structure set
    spacing 25 chunks, separation 8; swamp biomes only; kept 6 chunks from other village sets.
- **Pack:**
  - `options.txt`: `simulationDistance:12`, `renderDistance:12`, `maxFps:120`, `enableVsync:true`.
    Dedicated servers default to simulation distance 10.
  - `disable_villager_trading.js` removes only armorer, librarian, toolsmith and weaponsmith trades,
    through MoreJS 0.16.0. Other professions and wandering traders are untouched.
  - `structurify.json` disables `terralith:fortified_village` and `terralith:fortified_desert_village`;
    vanilla villages remain.
  - `config/incontrol/spawner.json` spawns `iceandfire:fire_dragon` in the overworld (`persecond` 0.1,
    `maxthis` 200) with no biome condition.
  - `config/iceandfire/iaf-common.json`: dragon `griefing` false, `canDespawn` true,
    `targetSearchLength` 128, `wanderFromHomeDistance` 40, `hungerTickRate` 3000.
  - Mods: Carry On 2.2.6.13, Sinytra Connector 2.0.0-beta.17, Numismatics 1.1.0, Surveyor 1.2.4, Antique
    Atlas 3.1.2, C2ME, Distant Horizons 3.3.1, Sodium, Lithium, ModernFix, FerriteCore, ServerCore,
    Create Nuclear 1.3.2-beta.3, Ribbits 4.1.6, YUNG's API 5.1.9, GeckoLib 4.9.3, Ice and Fire CE
    2.1-beta.1, In Control 10.3.0, Serene Seasons, Create 6.0.10, Sable 2.0.5, Create Aeronautics 1.3.2.
  - Serene Seasons ships no config (8-day sub-season default). ServerCore activation range is off.
- **Surveyor 1.2.4:** a Fabric jar whose `neoforge.mods.toml` is a Connector placeholder requiring
  `connector`; LGPL-3.0-or-later. `WorldLandmarks.of(Level)`, `Landmark.global`,
  `LandmarkComponentTypes`. Antique Atlas 3.1.2 draws landmarks.
- **Create 6.0.10:** `GlobalRailwayManager.tick(level)` returns at once outside the overworld, then
  ticks every train with no chunk check; a train stalls only when a player rides it and its lookahead is
  not entity-ticking. `CarriageEntityHandler` discards carriage entities in non-ticking chunks while
  `Carriage` and `Train.storage` stay; station arrival is graph-side; `StationUnloadedCondition` exists.
  `GlobalStation.runMailTransfer` writes to `GlobalPackagePort.offlineBuffer` when the postbox chunk is
  unloaded. `RailwaySavedData` is `create_tracks`, `LogisticsNetworkSavedData` is `create_logistics`.
  `FactoryPanelBehaviour` holds `targetedBy`, `recipeOutput`, `recipeAddress`, `restockerPromises` and
  `network` (target-stock field unchecked). `TableClothBlockEntity` has `owner: UUID` and a `priceTag`.
  `BlockStressValues.getImpact/getCapacity` exist; `KineticNetwork` is runtime-only.
- **Sable 2.0.5 / Create Aeronautics 1.3.2:** a ship is a `ServerSubLevel` in the same `Level`; the spawn
  sequence comes from `SableSpawnCommands.executeSpawnSchematicCommand`; `SubLevelHoldingChunkMap`
  freezes unloaded ships; `SubLevelLoadingTicketType` exists.
- **Ice and Fire CE 2.1-beta.1:** entity classes, NBT keys and `IafDragonFlightManager.setFlightTarget`
  as in §1.3.
- **Numismatics 1.1.0:** coins worth 1, 8, 16, 64, 512 and 4,096 spurs; `GlobalBankManager.BANK` and
  `BankAccount` by UUID; no coin recipe in the jar.
- **TaCZ:** a `LivingEntity` mixin makes every living entity an `IGunOperator`; no mob shooting goal.
- **Destroy (MIC fork, source):** pollution is NeoForge attachments; greenhouse, ozone depletion and acid
  rain per level; smog and radioactivity per chunk; it spreads and decays only in loaded chunks.
- **Deep Time** (Deep Time session and source): finite Petroff-Guyou worlds, 13,312 × 6,656 (16k) or
  26,112 × 13,056 (32k); pure-Java mmap reader `WorldData`; API `DeepTimeClimate` (API_VERSION 4) and
  `DeepTimeSites`; event log writer ids 2 and up reserved, with pre-play civ history in negative years as
  writer 2 (X2); JNI via `jni =0.22.4` and rayon; 8 platform builds plus `x86-64-v3` variants picked by
  `cpuprobe`; about 100 MB of terrain data per 16k planet with the timelapse dropped; a background bake
  takes 30 to 45 min on a player's machine and peaks at 3.2 to 4.2 GB; not in the MIC pack yet.
- **JVM:** JNA 5.14.0 ships with Minecraft (review). FFM is final in Java 22 and preview in 21. The
  Vector API is incubating through JDK 25 and needs a launch flag. Valhalla value classes target a JDK 28
  preview.
- **Economy inputs** (economy research): emerald ore spawns only in mountain biomes in vanilla; budding
  amethyst regrows clusters; trader llama movement attribute 0.175.
