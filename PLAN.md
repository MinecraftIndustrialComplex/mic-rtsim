# mic-rtsim: a simulated world for MIC

> **Baseline, 2026-10-19.** This is the plan as first written. Ben has since redirected it, and those
> decisions in DECISIONS.md take precedence where they differ: caravans first, no bespoke entities, no
> dependence on pack mods in the core, Ribbits first-class, Cobblemon out, scale planning first. A
> restructured plan replaces this one in the following commits.

**Status:** design only, 2026-10-19. Nothing is built. Written for Ben.

**What it is:** a world simulation in the spirit of Veloren's rtsim, built as a core engine plus content
mods. It runs the whole map whether or not anyone is there. That covers herds of tens of thousands
migrating with the seasons, villages that trade, grow and remember who wronged them, tribes that raid
the player because their refinery is choking the forest, dragons that hunt herds and burn barns,
merchant ships and pirates at sea, the player's own distant factories while they are unloaded, and
later whole planets.

**Ben's decisions (2026-10-19):**
- **Split:** a core library plus content mods, in one monorepo. Each mod is its own jar and mod id.
- **Licence:** AGPL-3.0; it is a MIC mod. Anything shared with Deep Time goes into a common library
  that we negotiate with Deep Time. Deep Time may later run on top of the core engine.
- **World:** a finite map. History (nations, evolution) should end up integrated with Deep Time.
- **Core language: Rust.** Ben asked for the comparison to be judged *only* on late-stage runtime
  performance ("buttery smooth even on a potato PC"); distribution and writing effort do not count.
  That comparison is §2, and Ben chose its recommendation. Phase 0 measurements can still reopen it.
- **Pack-wide optimization:** Ben plans to have agents optimize every mod in the pack: merging mods,
  more shared libraries, and rewriting mods and parts of Minecraft in Rust. The rtsim must fit that
  effort and help it. §1a says how.
- **One native host for the whole pack:** `mic_native` (§1a.1) hosts every native mod in the pack,
  including the optimization effort's rewrites.
- **Licence of the host and shared crates: MPL-2.0** (§8). The rtsim engine and content stay AGPL.

**Choices I made (all reversible before release):**
1. **Names:** the working name is `mic-rtsim`. Mod ids are `mic_rtsim` (core bridge) and
   `mic_rtsim_<module>`.
2. **Potato reference profile** (used by every performance gate):
   - x86-64 baseline natives (no AVX2).
   - The integrated server: client and server in one JVM.
   - One sim worker thread at the lowest OS priority.
   - The laptop pinned to 2 cores with `taskset`.
3. **Calendar:** the sim calendar is the Serene Seasons calendar. The pack ships no SS config, so the
   defaults apply: an 8-day sub-season and a 96-day year. In Deep Time worlds it is mic-climate's
   `YearClock`.
4. **The engine owns the truth.** A Minecraft entity that stands for a sim agent is a **puppet**.
   Vanilla never saves puppets; the engine captures their state when they leave.
5. **Bodies:**
   - Civilians are Cobblemon `NPCEntity` puppets (dialogue, MoLang variables).
   - Fighters (bandits, marines, pirates, guards) use one custom humanoid entity.
   - Goblins are a custom GeckoLib entity.
6. **Raids:** we write our own raid director rather than steering vanilla `Raid`.
7. **Dragons:** the dragons module switches off the existing In Control desert fire-dragon spawner, so
   the sim owns the dragon population.
8. **Factories:** the unit the sim abstracts is a Create 6 logistics network (stock links plus factory
   gauges). Remote cargo moves as Create packages.
9. **Storage:** agents live in `bevy_ecs` behind our own facade, used for storage and queries only.
   Its own executor and task pools stay off, because the rtsim owns no threads (§1a). Fields and
   networks live in custom grids and graphs. A phase-0 benchmark confirms or replaces this.
10. **Map output:** Surveyor landmarks, which the pack's Antique Atlas already draws.
11. **When history runs:** at world creation, after Deep Time's bake when one is present.
12. **This file:** it lives here until the monorepo exists, then moves to `mods/mic-rtsim/PLAN.md`.
13. **No private native runtime.** The rtsim loads as a plugin of `mic_native`, which owns native
    loading, the worker pools, profiling and memory accounting (§1a). It starts from a copy of Deep
    Time's natives tooling; Deep Time switches to it after its P1-19 gate (agreed, §8).
14. **A patch ledger and contract tests from day one.** Every mixin, and every reach into another
    mod's internals, is listed with the GameTests that pin it, so optimization agents can rewrite those
    mods safely (§1a.3).

---

## 0. Summary

| Module (mod id) | Simulates | What the player sees | Pack mods it drives |
|---|---|---|---|
| Native host (`mic_native`, pack-wide, §1a) | nothing; it loads, schedules, profiles and accounts for every native mod | `/micperf` | Deep Time, the rtsim, future Rust rewrites |
| Core (`mic_rtsim`) | clock, scheduler, storage, saves, LOD tiers, puppets, reports and sentiment | nothing directly; `/rtsim` commands, atlas markers | NeoForge, Surveyor + Antique Atlas |
| Ecology (`mic_rtsim_ecology`) | forage fields, herds with age cohorts, migration routes, predation | herds that move with the seasons; fewer animals where you over-hunt or pollute | vanilla animals, Cobblemon `PokemonEntity` |
| Civ (`mic_rtsim_civ`) | cultures, factions, settlements, notable people, memory, growth, history | villages with named people who remember you, grow, wall up, or become ruins | Cobblemon `NPCEntity` + dialogue, vanilla villages, Lootr |
| Trade (`mic_rtsim_trade`) | goods, markets, prices, caravans, treaties | prices that move, caravans on roads, NPC shops, NPC towns buying your train cargo | Numismatics, Create 6 table cloth shops, Create trains + packages |
| Conflict (`mic_rtsim_conflict`) | grievance, escalation, war bands, battles | envoys with demands, war-band markers on the atlas, night raids, sieges | Destroy pollution, TaCZ, L2 Hostility levels |
| Dragons (`mic_rtsim_dragons`) | dragon life, hunger, territory, raids, hoards, grudges | dragons that hunt herds, raid barns in winter, come back for you | Ice and Fire CE |
| Naval (`mic_rtsim_naval`) | sea lanes, merchant, pirate and navy ships, encounters | ships on radar, pirate attacks, escorts, privateering | Sable + Create Aeronautics, Create Big Cannons, Create Radars, TaCZ |
| Industry (`mic_rtsim_industry`) | remote factories as production processes, cargo at unloaded stations | distant outposts that keep producing while you are away | Create 6 logistics, trains, packages, PowerGrid, Destroy |
| Space (`mic_rtsim_space`) | bodies, orbits, interplanetary routes, megaprojects | later: a swarm eating Mercury, power beamed home | needs a space mod (none in the pack) |

---

## 1. The shape

```
 Minecraft server thread (Java)                  rtsim engine (native, its own threads)
 ┌────────────────────────────────────┐          ┌─────────────────────────────────────────┐
 │ mic_rtsim bridge                   │  obs     │ kernel: sim clock, cadence scheduler,   │
 │   observers (players, chunks,      │ ───────► │   event wheel, storage, saves, RNG,     │
 │   damage, pollution, Create state) │          │   profiler                              │
 │   materializer / puppet registry   │ ◄─────── │ modules: ecology civ trade conflict     │
 │   content adapters (IaF, Sable...) │  cmds    │   dragons naval industry space          │
 │   KubeJS events, datapack loading  │          │ knowledge: reports, sentiment, gossip   │
 └────────────────────────────────────┘          └─────────────────────────────────────────┘
          one hand-over per tick through two reused shared buffers, never per entity
```

- **The engine** knows nothing about Minecraft. It owns every simulated thing and runs on its own
  threads.
- **The bridge** is the only Java that runs on the server thread. It writes what happened this tick
  (observations) and applies what the engine decided (commands), under a fixed per-tick quota.
- **Content mods** have two halves:
  - a Rust crate of rules compiled into the engine;
  - a Java half that adapts the engine's commands to real entities and blocks (an IaF dragon, a Sable
    ship, a Cobblemon NPC), plus datapack definitions.
- **The engine owns no threads.** It runs as a plugin of the pack-wide native host (§1a).

---

## 1a. Fitting the pack-wide optimization effort

Ben plans to have agents optimize the whole pack: merging mods, adding shared libraries, and
rewriting mods and parts of Minecraft in Rust. Deep Time is already native, and the rtsim will be
the first native mod designed with that effort in mind. If every native mod brings its own runtime,
the pack ends up with islands that fight over the same cores. Six rules prevent that.

### 1a.1 One native host for every native mod in the pack

**The problem.** A native mod that brings its own thread pool sizes it to the core count. On a 4-core
PC, Deep Time's rayon pool, the rtsim's pool and a future native lighting or pathfinding rewrite
would each start about 4 threads. Those threads would fight each other, the render thread, and the
threads the pack already runs: C2ME's chunk workers and Distant Horizons' builders. Each native mod
would also bring its own loader, its own CPU detection and its own profiler.

**The fix.** The pack gets one host mod, `mic_native` (repo `mods/mic-native`). It owns:

| Part | What it does |
|---|---|
| Loader | Finds, checks and loads every native library, with one `natives.json` scheme (Deep Time's schema 2). `cpuprobe` picks the CPU variant once for all of them. The host starts from a copy of Deep Time's `tools/natives/` pipeline (§8). |
| Foreground pool | Normal OS priority. For native work that someone waits on:<br>• a game thread, e.g. a rewritten pathfinder or lighting engine called from the server tick;<br>• a player on a loading screen: Deep Time's world-creation bake, a dedicated server's first-start bake (no player can join until it ends), and the rtsim's history generation.<br>During play the governor caps it at cores − 2 (minimum 1). On a loading screen it may use every core. |
| Background pool | The lowest *normal* OS priority: nice 19 on Linux, `THREAD_PRIORITY_LOWEST` on Windows, utility QoS on macOS. For work nobody waits on: the rtsim during play, prefetching, previews. Sized to cores − 2, minimum 1, so one thread on the potato profile. It is deliberately not idle-class (`SCHED_IDLE`): there, a browser or Discord would win too, and an idle thread holding a lock could stall a game thread for as long as no core is free (Deep Time session, 2026-10-19). |
| Governor | CPU and memory accounting per plugin. Tells background plugins when they are over budget, so they stretch their cadence (§3.5). It replaces or feeds a plugin's own memory cap (Deep Time has one per session) and never runs alongside it. |
| Profiler | Named zones from every plugin. Tracy in dev builds, plus a ring buffer that `/micperf` reads in game. |
| Service registry | Native plugins publish versioned C-ABI services by name and look up each other's. Native code then calls native code without going through Java. |
| GPU device (later) | One wgpu device per process, shared by plugins that compute on the GPU (Deep Time's climate path does today). |

**Rules for every plugin:**
- **No threads or pools of its own.** Each plugin submits its work to the host's two pools.
- **Synchronous calls run on the caller's thread.** Some JNI calls are made directly from worldgen or
  render-support threads, for example Deep Time's play-time terrain columns on C2ME's workers and its
  far-terrain sections on Distant Horizons' threads. They stay on those threads and are never queued
  into a pool, because chunk loading waits on them.
- **Per-job context hooks.** The pools run enter and exit hooks registered by a plugin around each of
  its jobs. That keeps per-session thread-locals working. Today Deep Time sets its compute mode, log
  ring and allocator block cache with rayon's start and exit handlers.
- **No priority inversion.**
  - Foreground work never waits on background work.
  - Background work never holds a lock that a game thread takes. The allocator uses per-thread
    caches, background threads take no Java monitors, and only one writer appends to an event log at a
    time.
- **Plugins talk only through the C ABI.** Rust's own ABI is not stable between libraries built
  separately. So each plugin is its own cdylib and links a small `mic-native-rt` crate that wraps the
  host's function table.
- **Each mod still works alone.** Without the host (Deep Time used outside MIC), `mic-native-rt` falls
  back to a private pool with the same priorities.

**Why separate libraries instead of one big one:** each mod keeps its own release cycle, licence and
build. The cost is an indirect call at coarse boundaries, which is negligible at the rates in §3.

**What this means on a 2-core potato.**
- In singleplayer the render thread and the server thread already occupy both cores.
- **When the sim thread runs:**
  - Mostly when one of the game threads sleeps. The server thread sleeps between ticks whenever a tick
    takes less than 50 ms. The render thread waits for the GPU or vsync.
  - When there is no sleep, Linux still gives a nice-19 thread a small share of a contended core:
    weight 15 against 1,024 for a normal thread, in the kernel's weight table.
  - On Windows it runs only when no higher-priority thread is ready, apart from the scheduler's
    anti-starvation boosts.
- **The sim lives in that slack.** When there is none, it falls behind its schedule (§3.5) and never
  takes a frame.

### 1a.2 Native code can feed the sim directly

- **Observations are Rust traits** in `rtsim-space` and `rtsim-knowledge` (§5.2). Today a single
  implementation decodes the Java bridge's buffer.
- **Rewrites can feed the engine natively.** When an optimization agent moves part of Minecraft or a
  mod into Rust (chunk storage, entity tracking, Create's railway graph), it can publish a host service
  that the engine reads directly. Nothing crosses into Java.
- **It works the other way too.** The rtsim's coarse pathfinding and region index are host services
  that other native code may use.

### 1a.3 Everything the rtsim touches in other mods is listed

Optimization agents will merge and rewrite the very mods the rtsim adapts to. Three things make that
safe:
- **A patch ledger.** Each target mod gets one adapter class that holds every mixin and every access to
  that mod's internals. All of them are listed in `mods/mic-rtsim/PATCHES.md`, with the target mod,
  the version range, the class and member, and the reason.
  - Proposal: every MIC mod keeps one, and the optimization effort reads them all before touching a
    mod.
  - Two mods patching the same method at different mixin priorities is a trap we have already hit (the
    mic_nuclear reviews).
- **Contract tests.** Each adapter has GameTests that pin the behaviour it needs. Examples: a puppet is
  never saved; a package delivered to an unloaded station reaches the offline buffer; a dragon accepts a
  flight target. An agent rewriting Create runs them.
- **Adapters target behaviour, not classes.**
  - The puppet interface is spawn, capture and steer. No `Entity` appears in the engine.
  - A body can therefore later become something cheaper, such as a client-only ambient animal with no
    server AI, if the optimization effort builds one (§3.7).

### 1a.4 Few, cheap hooks into Minecraft

- **One listener per hot event for the whole rtsim**, in the core bridge. Hot events are damage, block
  break and place, and chunk load and unload.
  - The listener rejects most events with a lookup in a primitive set before any module sees them.
  - Modules subscribe inside the engine, not on the NeoForge bus.
- **Mixins only where no event exists.** There are two so far: `Entity.shouldBeSaved` and
  `IRadar.getTracks`.
- **The split into mod ids costs almost nothing at runtime.** Each content mod's Java side is a handful
  of adapters. If the optimization effort finds per-mod overhead that matters, bundling them is a build
  change (jar-in-jar or one combined jar), not a code change.

### 1a.5 Shared crates, not copies

Building blocks that the rtsim needs, and that any rewrite will also need, go in the host's shared
workspace instead of in the rtsim:
- generational arenas;
- counter-based RNG;
- the timing wheel;
- the region-keyed spatial index;
- the LZ4 + CRC32 page codec (from Deep Time's tile packs);
- the bench harness.

Their licence is a question for Ben (§11).

### 1a.6 One performance harness for the whole pack

- **The potato profile is defined once, at pack level** (choice 2), not separately per mod.
- **One scenario runner measures the whole pack:**
  - a fixed world and seed, and a scripted route;
  - server tick time (MSPT) at p99;
  - frame time at p99;
  - GC pause count and total, from JFR;
  - native zone times, from the host.
  The candidate runner is mc-runtime-test, which runs the real client headlessly.
- **The rtsim's perf gates (§3.1) are scenarios in that harness.** Each mod's server-thread cost is
  tracked separately, and the rtsim's 1 ms is its share.
- **Every optimization change is measured the same way.** A rewrite that speeds itself up by eating
  another system's slack then shows up.

---

## 2. Core language, judged only on late-stage performance

### 2.1 What makes a modpack feel bad on a weak PC

In singleplayer the server and the client are one JVM process. They share one heap, one garbage
collector and the same 2-4 cores. Raw throughput is not what players notice. These three things are:

1. **Work on the server thread.** TPS drops and rubber-banding.
2. **GC pauses.** A stop-the-world pause stops the render thread too, because it is the same process.
   The more live objects and garbage the process has, the more often and the longer it pauses.
3. **Background threads taking the render thread's core.** On 2-4 cores, a busy worker at normal
   priority competes with rendering and with C2ME/chunk building.

A simulation of a million things must cost (1) nothing beyond a fixed boundary, (2) nothing at all,
and (3) only what the OS can spare. All three options run the simulation off the server thread, so
they tie on (1). They differ on (2), (3) and on how much work each simulated thing costs.

### 2.2 The options

- **A. Java core.** Data-oriented Java: primitive column arrays, no Minecraft imports, addon rules in
  Java.
- **B. Rust core.** A native engine owns all simulation state and every per-thing loop. Java is only
  the Minecraft boundary.
- **C. Java now, Rust kernels later.** Java core first, and proven hot kernels move to Rust behind the
  same API.

### 2.3 Criterion by criterion

| Criterion | A. Java core | B. Rust core | C. Java + Rust kernels |
|---|---|---|---|
| Server-thread cost | boundary only | boundary only | boundary only |
| GC impact on frame pacing | **Sim state lives on the shared heap.** Primitive arrays are cheap for the GC (no references to trace; big ones are never copied). Sparse data (memories, relations, path caches, inventories), every addon written in ordinary Java, and every temporary object either become traced object graphs or garbage. The sim raises GC frequency for the whole process, client included. It stays near zero only if every line, addons included, stays allocation-free forever. | **None.** No sim objects on the heap. The GC sees two reused boundary buffers. | Same as A for everything not yet moved. Data shared with a kernel must live off-heap from day one, or be copied every step. |
| Heap pressure | Everything counts against `-Xmx`, which on an 8 GB PC the pack already mostly fills. A fuller heap means more frequent GC. | Native memory counts against RAM but not against `-Xmx`, so it does not make GC more frequent. Rows are exactly the size of their fields. | Mixed. |
| Thread priority (render thread first) | `Thread.setPriority` maps to OS priority on Windows. HotSpot ignores it on Linux by default. | The lowest normal OS priority on every platform: nice 19 on Linux, `THREAD_PRIORITY_LOWEST` on Windows, utility QoS on macOS. The OS gives the render thread its core first. | As A. |
| Work per simulated thing | C2 JIT targets the running CPU and auto-vectorizes simple loops. Explicit SIMD (Vector API) is incubating through JDK 25 and needs `--add-modules jdk.incubator.vector` at JVM launch, which a mod cannot add. Java 21 has no value types (Valhalla's JEP 401 targets a JDK 28 preview), so every record is either an object (header plus pointer chase) or split by hand into parallel arrays. Bounds checks are hoisted only in simple loops. | Ahead-of-time LLVM, structs and tagged enums by value, arenas. SIMD picked per CPU at load: Deep Time already ships baseline and `x86-64-v3` builds and picks one with `deeptime_cpuprobe`. | As A until moved, then as B. |
| Warm-up | Interpreted, then C1, then C2. The first minutes after world load, and the history fast-forward at world creation, run partly on cold code, while 190 mods JIT on the same cores. | Full speed from the first call. | As A. |
| Boundary cost | none | One or two JNI calls per tick handing over shared buffers. The cost grows with the number of *events* (spawns, deaths, trades), not with the number of simulated things. Per-entity JNI calls are forbidden by design. | Every kernel call crosses JNI with data. If state is on the Java heap it is copied each step. This is where C loses most. |
| Extension cost | An addon's Java rule runs inside the hot loop at JIT speed, and brings its own garbage and its own slow paths into it. | Addons extend through data (definitions compiled into engine tables), Rust rule crates compiled into the engine, and Java hooks on events (low frequency). Nobody can put per-entity Java into the hot loop, so the worst an addon can do is react slowly to an event. | As A. |
| Parallel safety at scale | Any race is a silent desync, found late. To be safe, fewer systems run in parallel. | Ownership rules make data races between parallel systems a compile error, so more of the sim can run in parallel safely. | As A. |
| Shared with Deep Time and future Rust rewrites | JVM sim threads sit outside the native host's pools and governor (§1a), so they compete with Deep Time and with every native rewrite instead of being scheduled with them. Native rewrites can reach the sim only through JNI. | Joins the pack-wide native host (§1a): one loader, one pair of pools and one profiler, shared with Deep Time and every native rewrite. On 2-4 cores that stops pools fighting. Native rewrites of Minecraft or mods can feed the engine without crossing into Java. Deep Time can even become an engine module, which Ben said he is open to. | As A, plus copying. |
| Visible in Spark | yes | No. The engine needs its own profiler (`/rtsim perf`, Tracy in dev builds). | partly |
| Failure mode on a potato | **Frame stutter.** GC pauses grow as memories, histories and caches grow over a long save. It is worst after hours of play, which is exactly when the sim is biggest. | **The sim falls behind its schedule.** Far things update less often, but frame pacing and TPS are untouched. | Both, plus copying. |

### 2.4 What the numbers say, and what still has to be measured

- **Cadence, not language, decides most of the cost** (§3.3). A million things updated once per
  in-game day is 1,000,000 / 24,000 ticks ≈ 42 rows per tick. Either language does that for free. So
  the gap between A and B is not about the bulk tiers.
- **The gap is the three potato threats.** GC, priority and warm-up all favour B, and none of them can
  be fixed in A by working harder.
- **Evidence on Java's ECS speed** (vendor benchmark, M1 Pro, JDK 17, one thread): Dominion reads one
  component over 10 M entities at about 0.7 ms per million. That is fine, but it is a best case on a
  fast core with no GC pressure from anything else.
- **Phase 0 measures, on the potato profile:**
  - the JNI hand-over;
  - the same herd kernel in both languages;
  - GC pause counts and lengths of a Java prototype after a long simulated run.
  If a measurement contradicts this section, the section changes.

### 2.5 Why C is the worst of the three

The expensive part of "move to Rust later" is moving the *state*, not the kernel. A kernel only wins
if the data already lives where it runs. C therefore either copies across JNI every step or keeps
state off-heap from day one. Keeping state off-heap is option B with the logic written in Java.

### 2.6 Recommendation

**B, a Rust core.** My first recommendation (Java) weighed addon ease and writing effort, which Ben has
now ruled out. On pure late-stage smoothness, B wins every criterion that a player on a weak PC can
feel. It loses only on Spark visibility, which our own profiler replaces.

Ben's plan to move more of the pack into Rust adds one more reason. A native sim shares one host, one
pair of pools and one profiler with the rest of the pack's native code, and it can exchange data with
that code directly (§1a). A Java sim would be the one large background system outside that
scheduling.

### 2.7 What Java would have changed (kept in case phase 0 reopens the choice)

These parts of the plan would change:
- The crates become Gradle modules with the same boundaries.
- Every hot table becomes primitive arrays or a fastutil map. Minecraft already ships fastutil.
- An allocation budget of zero bytes per tick in steady state, checked in CI with JFR allocation
  profiling, becomes a hard gate.
- Rules may be Java.

These parts stay the same: the module split, the tiers, the puppet protocol, the bridge, and every
content design in §7.

---

## 3. How it stays cheap: the performance architecture

The goal is that a player on a weak PC cannot tell the sim is running, even with a million simulated
animals and decades of history.

### 3.1 Budgets (targets we test against, on the potato profile)

| Budget | Target | Where it comes from |
|---|---|---|
| Server-thread cost of the bridge | ≤ 1 ms per tick at p99 | 2% of the 50 ms tick |
| Sim objects on the Java heap | none beyond two fixed boundary buffers | §2.1 |
| Sim threads | none of its own; it runs on the host's background pool at lowest OS priority: 1 thread on the potato profile, cores − 2 (minimum 1) elsewhere | §1a.1, §2.1 |
| Spawns and despawns per tick | a fixed quota (config); the rest wait in a queue | spreads materialization bursts |
| Blocks placed per tick (construction, ruins, damage) | a fixed quota (config) | as above |
| Materialized entities per player | a cap per module (config) | client frame time, server entity ticking |
| Save hitch | one copy of dirty pages at a tick boundary; compression and IO on a writer thread | §3.6 |

### 3.2 Five fidelity tiers

Every simulated thing lives in exactly one tier at a time. The tier decides how much detail exists and
how often it is touched.

| Tier | Who is in it | How it moves | When it decides | Cost grows with |
|---|---|---|---|---|
| **E: Embodied** | agents with a puppet in loaded, entity-ticking chunks | Minecraft moves the puppet; the engine shadows it | Minecraft AI, steered by engine intent (a goal: go here, attack that) | puppets × change events |
| **N: Near** | agents inside a player's interest radius but not in loaded chunks, or about to enter | path legs at block resolution | at decision points | legs |
| **F: Far** | every other individual agent: caravans, ships, dragons, war bands, herds as groups, notable people | legs on coarse graphs (roads, sea lanes, flight corridors, migration routes) with closed-form position | at decision points | legs and events |
| **A: Aggregate** | populations: herd members, settlement households, garrisons, factory processes, forage | counts and rates on grid cells, sites and processes; piecewise-linear between events | on a cadence (hourly, daily) | cells ÷ cadence |
| **S: Strategic** | factions, markets, nations, planets | n/a | monthly or seasonally | factions |

**Moving between tiers:**
- **Promotion** (A to F to N to E) happens when relevance rises: a player approaches, an event
  involves the thing, or it becomes notable.
- **Demotion** happens when relevance falls, after a delay so that edge-walking does not thrash.
- **Quantities are conserved across moves.** Demoting folds an individual back into its aggregate
  (count, mean hunger, wounds). Promoting draws an individual consistent with the aggregate. A herd
  of 20,000 that the player sees part of is still 20,000 after they leave, minus what they killed.
- **Entry and exit rates must match** (Sunshine-Hill and Badler, §12). Otherwise a player finds a full
  area on arrival that then empties, or a suspiciously constant headcount.

### 3.3 Time: events and closed forms, not ticks

- **Travelling things are not ticked.** A caravan on a road leg has a start time, a path and a speed.
  Its position at any time is a formula. The only cost is the **event** at the end of the leg
  (arrival), plus any encounter scheduled on the way.
- **Encounters are scheduled, not searched for.** When an agent starts a leg, the engine looks up the
  other agents booked on the same graph edges or cells in the same time window. It schedules a
  meeting event at the computed time. That replaces an all-pairs check per tick (the line-of-sight
  trap that killed Dwarf Fortress frame rates) with a lookup per leg start.
- **Periodic work is staggered.** A system with cadence C touches each row once every C ticks, chosen
  by a hash of the row id. Each tick does 1/C of the work and nothing ever spikes. Veloren
  (`(seed+tick)%10`), MCA and Dwarf Fortress all do this.
- **Logic is independent of the timestep.** Chances are hazard rates, `1-(1-p)^dt`, so a far agent
  stepped once an hour gets the same odds as a near one stepped every tick. Rates integrate in closed
  form between events. This is the Veloren `ctx.chance` idiom, and Unloaded Activity's catch-up.
- **Time jumps are normal.** `serversleep` skipping the night is just a large dt, handled the same way.
- **Timed events** (arrival, a birth, a harvest, a process running out of input) sit on a hierarchical
  timing wheel: constant time to schedule and to pop.

### 3.4 Lazy individuals: the sheep's hunger

Ben's example, how hungry one sheep is, shows the rule:

- A herd row holds counts per age cohort and the cohort's mean and spread of body condition. Members
  are **latent**: they have no row at all.
- When a member must exist (it is about to be materialized near a player), it is **derived**. Its
  identity, colour and hunger come from `hash(herd seed, member index)` plus the cohort statistics. If
  the player walks away and back, the same sheep with the same markings comes back.
- A member becomes **notable** and gets its own row only when something makes it matter: a player
  names it, tames it, breeds it or hurts it. Every other change folds back into the cohort.
- People work the same way. A settlement's households are cohorts. Only notable people (merchants,
  leaders, anyone a player has dealt with, historical figures) are rows. Unimportant rows are culled
  on a cadence, the way Dwarf Fortress culls unimportant historical figures and RimWorld collects
  unreferenced world pawns.
- **The question "how hungry is that sheep" costs nothing until someone looks.**

### 3.5 Threads and the tick boundary

```
 tick t   server thread: [ Minecraft tick ......... ][ bridge: apply cmds(<t), write obs(t), signal ]
          sim threads:                                                     [ step to t: ingest obs(t),
                                                                             run due systems + events,
                                                                             write cmds ]
 tick t+1 server thread: [ Minecraft tick ......... ][ bridge: apply cmds(t) ... ]
```

- **Two shared buffers per direction, reused forever.** They are fixed binary records with no format
  version before release. The engine allocates them; Java sees them as direct `ByteBuffer`s.
- **Hand-over:** sequence numbers are written with release and read with acquire ordering. There is
  one JNI call per tick to wake the sim thread.
- **The bridge never waits.** If the sim has not finished, the bridge applies nothing new that tick.
  Commands are at least one tick late, which is invisible for everything the sim decides (a raid, a
  spawn, a price).
- **Where immediacy matters, Java answers from cached state and reports afterwards.** Examples: a
  puppet's dialogue line, or a shop price shown in a tooltip.
- **Falling behind degrades far fidelity only.** The engine has a CPU budget per wall-clock second. When
  it is over budget, it stretches the cadence of S, A and F work, lowest relevance first. Closed forms
  keep stretched work exact, so nothing drifts; it just happens later. E and N work is never stretched.
- **Systems run in parallel by declared data access** (read and write sets), in fixed order where
  order matters. They run on the host's background pool (§1a.1); `bevy_ecs`'s own executor is not
  used. This is the CK3 and DSP pattern: parallel phases that do not depend on order, then a serial
  apply.
- **The governor's over-budget signal** comes from the host, so the sim also backs off when Deep Time
  or another background plugin needs the pool.

### 3.6 Memory and saves

- **Layout:** columns in arenas, with generational ids (index plus generation) everywhere. Any id may
  be dead, so every lookup checks (the Veloren rule).
- **What a row costs.** These are layout budgets we design to, worked as arithmetic:

| Thing | Row budget | Example count | Memory |
|---|---|---|---|
| Herd (group agent) | 64 B | 10,000 herds | 640 KB |
| Latent herd member | 0 B | any | 0 |
| Notable person, with 16 memory slots of 8 B | 96 B + 128 B | 100,000 | 22.4 MB |
| Report (an event someone knows about) | 32 B | 1,000,000 | 32 MB |
| Forage cell, 64-block cells, Deep Time 16k planet (13,312 × 6,656 blocks → 208 × 104) | 8 B | 21,632 | 173 KB |
| Same at chunk resolution (832 × 416) | 8 B | 346,112 | 2.8 MB |

- **Every memory structure has a cap and an eviction rule.** Examples: reports per person, sentiments
  per person and per faction (Veloren caps these at 128 and 1,024), historical figures, and path
  caches. Unbounded history is the second trap in §12.
- **Saves** go to `<world>/rtsim/`:
  - A manifest is written last, the same crash-safe order Deep Time uses.
  - Global tables (factions, settlements, notables, relations, reports) are separate from per-region
    pages (fields, local agents).
  - Each page is LZ4-compressed with a CRC32. That codec already exists in Deep Time's tile packs, so
    it is a shared-library candidate.
- **Save sequence:** at Minecraft's save event the engine copies only the dirty pages at a tick
  boundary (a memcpy), and a writer thread compresses and writes them. Veloren clones all of its Data
  on the tick thread every 60 s; that is the thing to avoid.
- **Reboot rules:**
  - Only state and small deltas persist. Names, looks and latent members derive from seeds.
  - AI plans restart on reboot, as Veloren's brains do.
  - Before release, a format change is just a version bump that wipes the save. There is no migration
    (standing rule for unreleased MIC mods).

### 3.7 Client cost

- **Clients only ever see puppets** (vanilla entity sync) and **map landmarks**. No simulation state
  is streamed to clients.
- **Puppets are capped per player, per module.** A herd of 20,000 near a player shows as a density of
  animals in loaded chunks under the cap, with the rest just over the hill. That matches how Assassin's
  Creed Unity shows 10,000-person crowds with 40 full AIs.
- **Later, a potato-friendly option:** draw far herds, armies and fleets as Distant Horizons custom
  render groups (`IDhApiCustomRenderObjectFactory`). These are boxes in the LOD view, not entities.
  Whether this looks good is unverified (phase 7+).
- **Later, if the optimization effort builds one:** a client-only ambient body. These would be
  animals drawn and animated on the client from a compact packet, with no server entity, AI or
  collision. Puppet adapters are not tied to `Entity` (§1a.3), so herds could switch to it without any
  engine change.

### 3.8 Measuring it

- **The potato profile** (choice 2) is the reference for every perf gate. It is defined at pack level
  and shared with the optimization effort's harness (§1a.6). `/rtsim perf` is the rtsim's page of
  `/micperf`.
- **The headless CLI** (`rtsim-cli`):
  - `run` fast-forwards a world;
  - `bench` runs fixed scenarios (a million latent sheep, 10,000 herds, 100,000 notables) and reports
    µs per system per sim day;
  - `history` generates a world's history;
  - `replay` re-runs a recorded session.
- **Flight recorder:** the bridge can log every observation buffer to a file. `rtsim-cli replay`
  re-runs a real play session deterministically, so perf and logic bugs found in game reproduce
  outside it.
- **Determinism gate:** after N sim days, the state hash with 1 worker must equal the hash with
  8 workers. RNG is counter-based and keyed by (world seed, system, entity, tick), so results never
  depend on thread timing.
- **Per-rule cost is measured continuously** (`/rtsim perf`, CI bench deltas). In CK3, one scripted
  check ran 850,000 times a month before anyone noticed.

---

## 4. The engine

### 4.1 Crates (a Cargo workspace, mirroring Deep Time's layout)

| Crate | Role |
|---|---|
| `rtsim-kernel` | sim clock, cadence scheduler, timing wheel, storage facade (`bevy_ecs` for agents), arenas, saves, counter RNG, profiler. No Minecraft, no JNI. |
| `rtsim-space` | address space and spatial indexes, world-model provider trait (Deep Time reader, sampled vanilla), coarse graphs, hierarchical pathfinding |
| `rtsim-knowledge` | reports, witnesses, gossip, sentiment, settlement noticeboards |
| `rtsim-ecology`, `rtsim-civ`, `rtsim-trade`, `rtsim-conflict`, `rtsim-dragons`, `rtsim-naval`, `rtsim-industry`, `rtsim-cosmos` | content rules; each one is a module (§4.4) |
| `rtsim-jni` | the cdylib, loaded by the `mic_native` host as a plugin; the only crate that uses `jni` (Deep Time pins `jni =0.22.4`); links `mic-native-rt` for pools, profiler and services |
| `rtsim-cli` | run, bench, history, replay, inspect |
| `rtsim-viewer` | HTML maps of histories, herds and trade, in the same style as Deep Time's viewer |

The Java side is a Gradle multi-project:
- `neoforge/core` builds `mic_rtsim`.
- `neoforge/<module>` builds `mic_rtsim_<module>`.

The generic building blocks (arenas, counter RNG, timing wheel, region index, page codec, bench
harness) come from the host's shared workspace (§1a.5), not from these crates.

### 4.2 Address space (built for planets from day one)

- **Frames:** universe → star system → body (planet, moon, station) → dimension (one Minecraft
  `Level`) → region (512 × 512 blocks, the same as a Minecraft region file) → chunk → block.
- **Positions** are (frame id, local coordinates).
- **Interest and paging both work per region.** That matches Minecraft's own loading unit.
- **Bodies carry orbital elements.** Travel times between bodies come from orbital mechanics, not
  tables (§7.8).
- **Retrofitting this later is the classic failure,** so the strategic tier is multi-body even while
  only the overworld exists.

### 4.3 Ids that Minecraft sees

- Engine ids are 64-bit (index plus generation).
- Anything that needs a UUID derives a stable UUID from (world seed, engine id): Numismatics
  `BankAccount`, the table cloth `owner`, and Cobblemon NPCs.

### 4.4 The module API (how content plugs in)

Each module registers:
- components and tables;
- systems, each with a cadence and its read and write sets;
- event handlers;
- **definition schemas**: what its datapack JSON looks like, compiled into engine tables;
- **body kinds**: which puppet adapters the Java side must provide.

This is Veloren's `Rule` binding typed events, with the host deciding what data each event may
touch. Ours adds declared access, so the scheduler can run modules in parallel.

**The four ways to extend, from most to least common:**
1. **Datapack definitions.** Species, cultures, goods, building templates, unit types, ship classes and
   dragon kinds. They are reloadable. Most content, including third-party content, is data.
2. **Behaviour as data.** Utility curves (input to score) and action plans (legs, waits, trades,
   fights). These are compiled into flat tables that the engine scores in batches across all agents
   due that tick. There is no general-purpose script interpreter in the hot loop.
3. **Rust rule crates** compiled into the engine. First-party MIC modules work this way. All of them
   are statically linked into one native library. A system stays dormant until its Java mod
   registers.
4. **Java and KubeJS hooks on events.** The engine emits batched, low-frequency events (raid
   declared, settlement founded, dragon killed) to `RtsimEvents` in Java and KubeJS. Scripts can
   answer with commands. Nothing per-entity, nothing per-tick.

### 4.5 Agent AI

- **Decisions happen at decision points** (a leg ends, something interrupts, a schedule fires), never
  per tick.
- **At a decision point:**
  1. The agent scores a short list of candidate plans with data-defined utility curves. Inputs include
     hunger, wealth, threat, sentiment, distance and orders from the faction.
  2. It commits to the best plan.
  3. The plan's first leg schedules the next event.
- **Priorities are Veloren's `choose` levels** (urgent, important, casual). Only a strictly higher
  priority interrupts, and the interrupted plan gets a cancel hook.
- **Combat:**
  - Fights nobody watches are resolved with Lanchester laws: square law for ranged, linear for melee,
    with unit stats from data and a morale break point.
  - Fights near a player are real Minecraft combat between puppets. Casualties are reported back.
  - **Rule: abstract results must never beat observed results.** X4's abstract combat is stronger than
    its real combat, so players learned to fight from another sector. We tune abstract resolution to be
    slightly worse for whoever has the stronger side, and test it against real fights in GameTests.

### 4.6 Knowledge: reports, gossip and memory

- **A report** is a fact: who did what to whom, where, when, and how severe. It expires by kind.
  Following Veloren, reports hold facts, never opinions.
- **Witnesses** are puppets within a data-defined radius, plus the settlement's own awareness when the
  event happens inside it.
- **Memory:**
  - Each notable person has fixed memory slots (report id, salience) and a sparse sentiment map
    (target to i8).
  - Settlements and factions keep aggregate sentiment per actor. Players are actors.
  - Caps follow §3.6. Decay is stochastic and slower for strong feelings, as in Veloren.
- **Gossip carries news physically:**
  - At home, people merge reports with the settlement noticeboard (Veloren's site `known_reports`).
  - Between settlements, news travels with caravans, ships and travellers, so word of what you did
    spreads at the speed of trade.
  - A town nobody trades with learns nothing.

---

## 5. The Minecraft bridge (Java)

### 5.1 Puppets without duplication

This is the protocol that makes "the engine is the truth" safe:

1. **Tagging.** A puppet carries the attachment `mic_rtsim:agent`, holding the engine id and a
   materialization epoch.
2. **Puppets are never saved by vanilla.** A mixin on `Entity.shouldBeSaved` returns false for tagged
   entities. Our own entities simply override it.
3. **Capture before removal.** When a puppet's chunk unloads, or the puppet is removed, its adapter
   captures the state the engine cares about: health, wounds, carried items, sheared, tamed, cargo.
   It sends the state back as a `Dematerialized(id, epoch, state)` record. Veloren loses inventory and
   mid-action state on this round trip; we capture through a per-body adapter so nothing that matters
   is lost.
4. **A stale puppet is discarded on sight.** One with an old epoch (for example, saved by another mod)
   never wins.
5. **Crash safety comes free.** Puppets never reach disk, and the engine's last save is consistent with
   itself.

**Adapters.** Each body kind has one Java adapter with three operations:
- `spawn(record) → Entity`;
- `capture(Entity) → record`;
- `steer(Entity, intent)`: sets a move target or an attack target.

**Persistence while materialized.** Vanilla despawns non-persistent mobs beyond 128 blocks, and IaF
dragons have `canDespawn = true` in the pack config. So every puppet is marked persistence-required
for as long as it is materialized.

### 5.2 What the bridge observes (and how often)

There is one NeoForge listener per event for the whole rtsim (§1a.4). Each observation below is
also a Rust trait, so a native rewrite of its source can feed the engine directly (§1a.2).

| Observation | Source | Cadence |
|---|---|---|
| Players: position, dimension | server player list | every 20 ticks |
| Chunks loaded and unloaded, forced tickets | `ChunkEvent.Load/Unload`, ticket controllers (mic-grid-loader, create-power-loader) | on event |
| Damage and deaths involving puppets or tracked animals | `LivingIncomingDamageEvent`, `LivingDeathEvent` | on event |
| Blocks broken or placed inside tracked areas (territory, plots) | `BlockEvent.BreakEvent`, `EntityPlaceEvent`, filtered by area first | on event |
| Destroy pollution | `PollutionHelper`: per chunk (smog, radioactivity) and per level (greenhouse, ozone depletion, acid rain) | per loaded chunk in tracked areas, staggered |
| Create trains | `RailwaySavedData` (`create_tracks`): positions, `ScheduleRuntime` state | every 20 ticks |
| Create logistics networks in districts | `LogisticsNetworkSavedData`, `FactoryPanelBehaviour` | while loaded, staggered |
| Climate | mic-climate `Climate.celsius(Level, BlockPos)`, `YearClock`; Deep Time `DeepTimeClimate.at(pos)` | on demand for cells |

### 5.3 What the bridge does on command

- Spawn, despawn and steer puppets, through adapters, under the per-tick quota.
- Place structures and build steps progressively, under the block quota. It never overwrites
  player-placed blocks (§7.2).
- Write Surveyor landmarks: `WorldLandmarks.of(level).put(Landmark.global(...))`. Antique Atlas 3.1.2
  draws them.
- Write Numismatics balances: `GlobalBankManager.BANK`, accounts keyed by UUID.
- Fill and drain Create package offline buffers: `GlobalPackagePort.offlineBuffer`.
- Fire KubeJS events.

### 5.4 Commands for Ben

- `/rtsim perf` shows each system's µs per tick and the tier counts.
- `/rtsim inspect <agent|settlement|herd>` describes one thing.
- `/rtsim near` lists what is simulated around you.
- `/rtsim jump <days>` fast-forwards time.
- `/rtsim record start|stop` drives the flight recorder.

---

## 6. Reuse table (need → pack mod → verified hook)

Verified means a research agent saw the class, method, config or doc in the pinned jar or source on
2026-10-19 (Appendix A). Anything else is marked.

| Need | Pack mod (version) | Hook |
|---|---|---|
| Talking civilians with memory | Cobblemon 1.8.1 | `entity.npc.NPCEntity`, datapack `NPCClass` (names, variations, `config` MoLang variables, behaviours, `canDespawn`), `api.dialogue.Dialogue` / `DialoguePredicate` |
| Herd bodies | vanilla animals; Cobblemon | `PokemonProperties.createEntity(Level)`. Cobblemon herds cap at 9 per leader group, so the engine does grouping. |
| Dragons | Ice and Fire CE 2.1-beta.1 | `iceandfire:fire_dragon` / `ice_dragon` / `lightning_dragon`; `DragonBaseEntity` NBT (`AgeTicks`, `Hunger`, `Gender`, `Variant`, `HomeAreaX/Y/Z`, ...); `IafDragonFlightManager.setFlightTarget(Vec3)`; roost and cave structures (enabled in `structurify.json`) |
| Guns for NPCs | TaCZ | every `LivingEntity` is an `IGunOperator` (mixin), with `LivingEntityShoot` and `LivingEntityAmmoCheck`. No mob AI goal exists, so we write one. |
| Troop scaling | L2 Hostility | NBT `l2hostility:level` |
| Ships | Sable 2.0.5 + Create Aeronautics 1.3.2 | `SchematicLoader.loadSchematic` → `container.allocateNewSubLevel(Pose3d)` → `plot.newEmptyChunk` → `template.placeInWorld(plot.getEmbeddedLevelAccessor(), ...)` → `updateLastPose()`. Ships freeze when unloaded (`SubLevelHoldingChunkMap`). |
| Ship guns | Create Big Cannons | `PitchOrientedContraptionEntity.tryFiringShot` on an assembled, loaded cannon |
| Radar contacts | Create Radars | `RadarScanningBlockBehavior` already tracks loaded entities and ships. Far sim ships need a mixin on `IRadar.getTracks` (no public API). |
| Money | Numismatics 1.1.0 | `Coin` worth 1, 8, 16, 64, 512 and 4,096 spurs; `GlobalBankManager.BANK` of `BankAccount` by UUID |
| NPC shops | Create 6.0.10 | `TableClothBlockEntity` (`owner` UUID, `priceTag` filter; any item, so coins work). A non-player owner is untraced. |
| Trains while unloaded | Create 6.0.10 | `GlobalRailwayManager` ticks every overworld train every tick with no chunk check; `RailwaySavedData` `create_tracks`. Not in other dimensions (early return). |
| Cargo at unloaded stations | Create 6.0.10 | only mail: `GlobalStation.runMailTransfer` → `GlobalPackagePort.offlineBuffer` |
| Declared production | Create 6.0.10 | `FactoryPanelBehaviour` (inputs with amounts, `recipeOutput`, address, promises); `LogisticsNetworkSavedData` `create_logistics`. Valid only while loaded, so we snapshot it. |
| Machine power estimate | Create 6.0.10 | `BlockStressValues.getImpact/getCapacity` × RPM; no per-network saved total |
| Industrial footprint | Destroy (MIC fork) | `PollutionHelper`, `PollutionEvents`. Smog and radioactivity per chunk; greenhouse, ozone depletion and acid rain per level. It only changes in loaded chunks. |
| Temperature and season | mic-climate, Serene Seasons | `Climate.celsius`, `YearClock.now`, `SeasonHelper` |
| World data (Deep Time worlds) | Deep Time (not in pack yet) | `WorldData.open/raster/height/waterLevel`, `Rivers`; API `DeepTimeClimate.at(pos)` (monthly °C, Köppen), `DeepTimeSites` (resource deposits) |
| Map markers | Surveyor 1.2.4 + Antique Atlas 3.1.2 | `WorldLandmarks.of(Level).put(...)`, `Landmark.global(id, builder)` with `POS`, `NAME`, `LORE`, `COLOR`, `TIME` |
| Remote full-fidelity bases | mic-grid-loader, create-power-loader | NeoForge `TicketController`, power-gated |
| Computer access | CC: Tweaked 1.120.2 | `PeripheralCapability` through `RegisterCapabilitiesEvent`, for a sim peripheral (market prices, news) |
| Ruins loot | Lootr | per-player chests in ruin structures |
| Radio warnings (later) | Simple Radio 4.5.7.9 | `SimpleRadioApi` frequencies and sources; the audio format is unverified |
| Existing raid code | pack KubeJS `zombies.js` | per-player zombie raid (`/startraid`). It becomes the undead faction, or is retired. |

**No mod in the pack covers these** (verified against the full mod list): goblins, orcs, bandits or
pirates as factions; a dragon behaviour that raids villages; a mob AI that fires TaCZ guns; settlement
growth; herds beyond 9 per group; any simulation of unloaded areas; space.

---

## 7. Content modules

Each module lists its model, its Minecraft side, and a walkthrough of what a player actually does and
sees.

### 7.1 Ecology (`mic_rtsim_ecology`)

**Model.**
- **Species** (datapack `data/<ns>/rtsim/species/*.json`):
  - body (an entity type, or a Cobblemon species plus aspects);
  - diet, comfort band in °C, preferred Köppen classes (biome tags as the fallback);
  - herd size range, travel speed;
  - breeding sub-seasons, litter, lifespan, predators.
- **The forage field** is a grid over the map. It regrows from monthly temperature and precipitation:
  Deep Time's L1/L2 climate in Deep Time worlds, otherwise biome climate plus Serene Seasons. It is
  lowered by grazing, Destroy acid rain and smog, and player land use seen while loaded. A grid kernel
  updates it once per in-game day, staggered.
- **A herd** is one F-tier agent: cohort counts (young, adult, old), body condition per cohort, a
  route and a memory of traditional routes. History generation builds those routes (§8).
- **Migration:**
  - Each sub-season the herd picks its next target from a flow field of forage within its comfort
    band.
  - Legs are closed-form. Births and deaths are hazard rates applied at events.
  - When a herd's puppets keep failing to pass a place (a wall the player built, a river of acid),
    the bridge reports the obstacle. The route edge's cost rises and the herd detours.
- **Predation:** the player, wolves, dragons and NPC hunters all subtract through reports.

**Minecraft side.**
- When a herd's footprint overlaps loaded chunks near a player, the bridge spawns representative
  animals there. The number is the count times the fraction of the footprint that is loaded, capped
  per player.
- Each animal is a derived member (§3.4), so the same ewe with the same colour comes back.
- Puppets run vanilla grazing AI and get a steer goal toward the herd's next waypoint.

**Walkthrough.**
1. It is Late Spring (Serene Seasons). Your Antique Atlas shows a marker, *Greywool flock*, labelled
   with its current count and drifting north-east along the valley.
2. You ride up the valley. As the flock's footprint enters your loaded chunks, sheep and lambs appear
   at the far edge and graze their way past, all moving the same way.
3. You kill six and shear ten. The flock's count drops by six. "Sheared" folds into the adult cohort
   and regrows over time.
4. You go home. The puppets dematerialize, and the flock keeps walking: one event per leg, no ticks.
5. You fence off the pass with a wall. In autumn the returning flock's puppets fail to get through. The
   route cost rises and next year the flock takes the long way round.
6. You build a Destroy plant upstream. Acid rain lowers the meadow's forage. The flock's body condition
   drops, lambing fails that spring, and the count on the marker falls. The wolves and the dragon that
   follow it go hungry too.

### 7.2 Civ (`mic_rtsim_civ`)

**Model.**
- **Entities:**
  - **cultures** (data: building style, tolerances, values, tech level);
  - **factions** (tribe, nation, bandit clan, city league), each with a leader, territory and
    relations;
  - **settlements** with tier (camp → hamlet → village → town → city), household cohorts, stocks,
    plots and buildings;
  - **notable people** (rows);
  - **roads** (a graph).
- **Settlements in today's pack.** The world is Terralith/Tectonic, and `structurify.json` disables
  Terralith's villages. The module adopts each vanilla village as a settlement the first time its
  structure start is seen (`StructureTags.VILLAGE`, `StructureManager.getStructureWithPieceAt`). Its
  history is generated locally from its surroundings.
- **Settlements in finite-map worlds.** The engine decides where settlements are before chunks
  generate. Worldgen places them through our own structure placement (§8).
- **Growth.**
  - Population follows food, which is fields, herds and trade, through a logistic curve with a
    carrying capacity.
  - Wealth funds building projects from the culture's templates (structure NBT per tier).
  - Threat funds walls. Trade with an industrial player unlocks better materials, up to a town that
    builds its own Create windmill.
  - A settlement that is sacked or starved becomes a **ruin**: the same structure, damaged, with Lootr
    chests.
- **Building without griefing the player.**
  - Settlements own **plots**. Construction only happens on plots.
  - If player-placed blocks are found in a plot, construction waits, and the settlement records
    encroachment as a grievance.
  - Unloaded settlements complete projects in sim time. On the next load, the bridge places finished
    buildings under the block quota, in chunks no player has modified inside the plot.
  - Loaded settlements build progressively: builder puppets walk to the site while blocks appear a few
    per tick.

**Minecraft side.**
- Civilians are Cobblemon `NPCEntity` puppets. NPC classes are per culture and role (baker, guard,
  merchant, elder).
- At materialization the bridge writes the person's sentiment toward each nearby player, and the top
  known reports, into the NPC's MoLang variables. Cobblemon `DialoguePredicate`s pick lines from them.
- What still needs checking: whether mods can register new Cobblemon `BehaviourConfig` types (§11).

**Walkthrough: you slight someone.**
1. Brindle is a vanilla village the sim adopted. Marta the baker stands at her stall: a Cobblemon NPC
   with a generated name and her own look.
2. You punch Marta. The bridge sees `LivingIncomingDamageEvent` on a puppet and sends a report:
   *Assault*, you → Marta, severity from the damage.
3. Marta's sentiment toward you drops. The two villagers within the witness radius get the report too.
4. Talk to her and the dialogue opens cold. The predicate reads the sentiment variable the bridge set.
5. At dusk Marta goes home. The report merges onto Brindle's noticeboard, Brindle's opinion of you
   drops, and guard puppets now watch you when you are in town.
6. Two days later a caravan leaves for Oakford carrying the report. Oakford only learns about it when
   the caravan arrives.
7. Weeks later the feeling has faded a little. Strong feelings fade slowly. A gift of bread, or paying
   the fine the elder asks for, repairs it faster.

### 7.3 Trade (`mic_rtsim_trade`)

**Model.**
- **Goods** are groups of item tags.
- **Production** comes from each settlement's land and culture. In Deep Time worlds it also comes from
  `DeepTimeSites` resource deposits.
- **Markets:** each settlement has stocks, demand and a price per good. Every price and stock is
  clamped: Veloren's economy needed hand clamps because it "isn't economying", so ours are designed in.
- **Caravans and merchant ships** run arbitrage along roads and sea lanes, paying tolls to the factions
  they cross.
- **Treaties** raise sentiment and lower tolls. War brings embargo.
- **News** rides with every caravan (§4.6).
- **Money:**
  - Numismatics. NPCs and settlements have `BankAccount`s keyed by derived UUIDs.
  - Prices are quoted in spurs. The coins are worth 1, 8, 16, 64, 512 and 4,096 spurs.

**Minecraft side.**
- **NPC stalls** are Create 6 table cloth shops whose `owner` is the merchant's UUID, with the price in
  Numismatics coins. Whether a non-player owner works end to end is untraced; the fallback is selling
  through Cobblemon dialogue.
- **Per-player prices** (a settlement that dislikes you charges you more) go through the merchant's
  dialogue offer, because a table cloth has one price for everyone.
- **Towns that trade by train** get a Create station in their town plan. They also get a postbox
  addressed by the town's name.

**Walkthrough: selling iron to a town by train.**
1. Oakford, a town, has a station and its market buys iron and coal.
2. You give your freight train a schedule to Oakford with a package postbox carriage. Your Stock Keeper
   sends packages of iron ingots addressed `Oakford Market`.
3. The train runs to Oakford while you sit at home. Create ticks trains whether or not their chunks
   are loaded. At the station, Create's own mail transfer drops your packages into Oakford's postbox
   offline buffer.
4. The engine's market reads the buffer and prices the iron. It pays your Numismatics account and puts
   packages of the grain you ordered, addressed to you, into the same buffer.
5. The train comes home with grain. Your bank balance has changed. Oakford, with steady iron, starts a
   smithy project, and a year later its skyline has one.

### 7.4 Conflict (`mic_rtsim_conflict`)

**Grievance.**
- Each faction keeps a ledger per player (or team) per signal kind. Its culture's tolerances turn
  signals into grievance.
- **Signals the bridge can actually see:**
  - Destroy smog and radioactivity in chunks inside the faction's territory;
  - level greenhouse, ozone depletion and acid rain;
  - logs broken inside territory (`BlockEvent.BreakEvent`, `#minecraft:logs`);
  - ore veins drained by Create Ore Excavation drills inside territory (`OreDataAttachment` per
    chunk);
  - members killed (reports);
  - player blocks on their plots;
  - emissions from the player's abstract districts (§7.7).
  The last one closes the gap where Destroy pollution only accumulates in loaded chunks.
- **Escalation:** notice → envoy with a demand → scouts and sabotage → raid → invasion → siege.
- **De-escalation:** pay the tribute, cut the emissions (a Destroy catalytic converter lowers smog),
  gifts and trade, or kill the war chief.

**Armies.**
- A war band is an F-tier agent with unit stacks.
- It musters at a settlement or camp, marches on the road graph, and shows on the Antique Atlas while
  scouted.
- **Raids on player bases only resolve while a player of that base is online.** Otherwise the band
  camps near the base, visible on the map, and waits (config). Raids on NPC settlements resolve
  abstractly.
- **Raid director:**
  - picks a spawn arc at the edge of the loaded area on the band's side;
  - spawns waves under the spawn quota, with troops scaled through `l2hostility:level`;
  - sends survivors home when morale breaks;
  - reports casualties to the band.

**Bodies.**
- **Goblins** are a custom GeckoLib entity with melee, bows and torches. Chieftains carry a TaCZ gun.
- **Bandits** use the custom humanoid with TaCZ guns.
- Both need our TaCZ-firing AI goal, which calls `IGunOperator`.
- Existing mobs can join as allies or monsters, for example Born in Chaos's `MissionaryRaiderEntity`
  as cultists.

**Walkthrough: the Ashfang goblins and your refinery.**
1. You build a Destroy refinery on the river below the Ashfang forest. Its vats and open-ended pipes
   put smog into the chunk.
2. The Ashfang culture is intolerant of smog and of logging. Their grievance against you rises with the
   smog the bridge samples in their territory.
3. *Notice:* goblin scouts appear at the forest edge. They watch and flee when approached.
4. *Envoy:* a goblin envoy walks to your base. Its dialogue demands a monthly tribute in Numismatics
   coins, or that the smoke stops. Pay, and grievance pauses. Put catalytic converters on the line,
   and smog falls and grievance decays.
5. Ignore it, and a war band musters at the Ashfang camp. An atlas marker shows it marching down the
   forest trail over the next days.
6. You are online when it arrives. At night, waves spawn at the edge of your loaded area. You hold
   them with TaCZ rifles and a Create Big Cannons battery. The chieftain has a shotgun.
7. Outcome: the band breaks and the survivors go home. Ashfang's grievance becomes fear. The human town
   downriver, which hates the goblins, thinks better of you, and its next caravan brings a gift.
8. Had you been offline, you would have found the band camped below your walls when you logged in.

### 7.5 Dragons (`mic_rtsim_dragons`)

**Model.**
- A dragon is an F-tier agent with species (fire, ice, lightning), age, hunger, a lair, a territory, a
  hoard, and sentiment toward players.
- **Lairs** are Ice and Fire's roost and cave structures. They are enabled in the pack, and the engine
  finds them by structure lookup.
- **Hunger** drives hunting trips into the dragon's territory:
  - first herds (ecology);
  - then settlement livestock when the herds have migrated away (winter);
  - then a player's farm if it holds a grudge.
- **Ice dragons shift range with the cold.**

**Minecraft side.**
- The dragon materializes as an IaF entity whose NBT comes from engine state (`AgeTicks`, `Hunger`,
  `Variant`, `Gender`, `HomeAreaX/Y/Z`). It is persistence-required.
- `IafDragonFlightManager.setFlightTarget(Vec3)` steers it.
- A custom goal picks livestock targets using IaF's `dragon_targets` tag. Whether IaF dragons carry
  prey off is untraced.
- The pack has dragon griefing off. Fire damage during a raid is therefore placed by the engine,
  bounded per raid, and recorded in settlement state.
- **Ownership:** the module disables `config/incontrol/spawner.json`'s desert fire-dragon spawner, so
  the sim owns the dragon population and a killed dragon is not silently replaced.

**Walkthrough.**
1. Winter. The Greywool flock went south. Cinderjaw, a fire dragon at stage 3 in the roost east of
   Brindle, is hungry.
2. The engine picks Brindle's livestock.
   - If nobody is there, the raid resolves abstractly: Brindle loses animals and a barn, and a report
     goes out. Brindle's elder posts a bounty in Numismatics coins. You hear it in dialogue in any town
     the news has reached.
   - If you are in Brindle, Cinderjaw materializes beyond render distance and flies in. It seizes
     sheep and sets two roofs alight.
3. You drive it off wounded. Its health is captured when it leaves. It remembers you, and next hungry
   season your farm is on its list.
4. You kill it. Its hoard sits in the roost, and Brindle pays the bounty. No replacement dragon spawns
   from thin air; a young dragon may claim the roost years later.

### 7.6 Naval (`mic_rtsim_naval`)

**Model.**
- **Sea lanes:** a graph built from coast and ocean data (Deep Time water and elevation rasters, or
  sampled ocean biome tags), with ports at coastal settlements.
- **Ship classes** (datapack): schematic, speed, cargo, crew, guns.
- **Ships:**
  - merchants follow trade (§7.3);
  - pirates hunt lanes from cove bases, using encounter scheduling;
  - navies patrol, escort, and hunt pirates by following reports.
- **Fights nobody watches:** pursuit by speed, then Lanchester resolution by guns and crew (§4.5).

**Minecraft side.**
- Near a player, a ship materializes as a Sable sub-level from its schematic (the five-call recipe in
  §6). Its crew are humanoid puppets with TaCZ guns. Its CBC cannon is fired by our crew AI through
  `tryFiringShot`.
- **Damage persists.** On dematerialize, the adapter snapshots the ship's plot blocks back into a
  template, so a holed hull stays holed. Saving a sub-level plot to a template is unverified (§11).
- **Radar:** materialized ships show on Create Radars for free. Far sim ships need the `getTracks`
  mixin.
- Sable freezes unloaded ships, so the engine owns every NPC voyage.

**Walkthrough.**
1. You are sailing a Create Aeronautics vessel along the coast. Your Create Radar shows a contact well
   beyond view: a sim ship through the mixin.
2. It closes. At materialization range a pirate sloop appears, built from its schematic, with crew on
   deck and a cannon that fires.
3. You sink it, or board it and take the cargo. The pirate faction's sentiment toward you crashes. The
   coastal nation's navy offers you a letter of marque through a harbour master's dialogue.
4. Had you fled after holing it, the sloop would have limped back to its cove with the hole in its
   hull, and been patched over the following days.

### 7.7 Industry (`mic_rtsim_industry`)

**Unit: a district.** A district is one Create 6 logistics network (`create_logistics`): the vaults
behind its stock links are its stock, and its factory gauges declare its production. This is Create's
own idiom; nothing new to place.

**While loaded:**
- the engine snapshots the gauge graph (inputs with amounts, outputs, targets, addresses);
- measures each gauge's real throughput over time;
- estimates kinetic demand from `BlockStressValues` impact × RPM (PowerGrid separately);
- records emission rates from Destroy's chunk pollution.

**While unloaded:**
- Each gauge is a process with its measured rate. Rates are limited by inputs in the stock snapshot,
  and a process stops at its target stock, which is Create's own semantics.
- Integration is piecewise-linear between events (an input runs out, a target is reached). It is
  exact and cheap.
- Trains keep running, because Create already does that.
- At district stations, packages move through the postbox offline buffers, which the engine fills from
  and empties into the abstract stock.
- Plain item transfer at stations does not happen while unloaded (belts and funnels need loaded
  chunks), so remote districts trade in packages.
- Emissions keep feeding grievance (§7.4), and are written into Destroy's chunk attachments when the
  chunks load.

**On reload:**
- The engine reconciles stock deltas into the linked vaults under the per-tick quota, capped by
  capacity.
- Create then takes over. Items on belts are never touched.

**Other dimensions:** Create does not tick trains outside the overworld (`GlobalRailwayManager.tick`
returns early). Districts in other dimensions therefore need engine-run trains or a Create mixin.
Decided later.

**Full fidelity:** mic-grid-loader stays the option for a base that must really run.

**Walkthrough.**
1. Your iron outpost on the far coast has crushers, washers, blaze-burner smelting and factory gauges
   keeping a target of ingots in stock. Its stock links put three vaults on the network *Iron Coast*.
2. While you are there, the engine learns the gauge graph and measures each gauge's rate.
3. You go home and the outpost unloads. The engine turns ore into ingots at the measured rates until
   the ingot target is met or the ore runs out. The outpost's smog keeps counting against the Ashfang
   goblins.
4. Your freight train calls. Create moves it. At the outpost's station, the offline buffer receives the
   engine's packages of ingots addressed to your base.
5. Weeks later you return. As the chunks load, the vaults hold less ore and more ingots, and the
   machines pick up where the numbers left off.

### 7.8 Space and megaprojects (`mic_rtsim_space`, later)

The pack has no space mod; adding one is Ben's decision when the time comes. The engine is ready
anyway (§4.2):

- **Bodies** have mass, orbit and composition. Transfer times between bodies come from orbital
  mechanics (Hohmann transfers).
- **Body economies and interplanetary factions** are the strategic tier: a few rows, decided monthly.
- **A megaproject is a strategic-tier process on a body,** for example a self-replicating swarm
  turning Mercury into collectors:
  - **Its state:** replicator mass, mined mass and collector area.
  - **Growth** is limited by three things:
    - mass: Mercury is 3.30 × 10²³ kg;
    - energy: the solar constant of 1,361 W/m² at 1 AU, divided by 0.387², gives about 9,090 W/m² at
      Mercury's orbit, times collector area and efficiency;
    - the replication time of the design the player researched.
  - **Output:** collector power beamed to a receiver block on a PowerGrid network at home.
  - Every number comes from physics or from the player's design.
- **Districts on other bodies** use the same process model as §7.7. The connections between them are
  interplanetary routes instead of rails.

---

## 8. History generation and Deep Time

**History is the same engine run fast.** At world creation the S and A tiers of every module run with
large time steps over N years. Individual tiers are off, and only notable people exist. This is how
Dwarf Fortress builds its history, a year every couple of seconds with about 20,000 people. Because
it is the same rules, the world the player walks into is in the state those rules produce, not in a
separate generator's idea of it.

**Inputs: a world-model provider.**
- **Deep Time worlds:**
  - terrain, water and rivers (`WorldData`, `Rivers`);
  - monthly temperature and precipitation, and Köppen (L1/L2);
  - resource deposits (`DeepTimeSites`).
  The world is finite: 13,312 × 6,656 blocks at 16k Petroff-Guyou.
- **Other worlds:** the area inside the world border, sampled on a coarse grid from the biome source
  and the generator's base height.

**Outputs:** settlements, factions and borders, roads, ruins, dragon lairs, herd routes, notable
figures, and a chronicle.

**Worldgen reads the outputs.**
- A custom `StructurePlacement` puts sim settlements and ruins where the sim says.
- A per-chunk road feature paves the road segments that cross that chunk. This is deterministic and
  chunk-local.
- Lairs reuse IaF structures where possible.

**Deep Time integration, already planned on Deep Time's side.**
- Deep Time's event log reserves writer ids 2 to 65535 for later writers, e.g. `civ:history`. Its plan
  §4.2 names a future `deeptime-civ` crate that reads sites and appends segments.
- The rtsim history writes there, so the chronicle shows in Deep Time's own viewer and timelapse.
- **Time convention (agreed with the Deep Time session, 2026-10-19).**
  - `t` is in years relative to world creation (`deeptime-format/src/events.rs`). Its sign means
    before or after creation; it does not separate geology from civ.
  - Pre-play civ history gets negative years on the same axis. In-play civ events are positive.
  - Writer id 2 and the `civ:` kind namespace keep civ events apart from geology. No consumer reads the
    sign as that distinction.
  - Deep Time is fixing two passages that say otherwise: a doc comment and its plan §5.
- **Evolution, later.** Ben wants histories that include evolution. A coarse evolution model across
  Deep Time's geologic keyframes would generate species definitions adapted to each climate zone.
  Each species maps to a Minecraft body by tags. The species format (§7.1) is designed now so that
  generated species fit it.

**History runs on the foreground pool** (§1a.1). The player is waiting on the Create World screen,
or the dedicated server admits nobody until it finishes. Deep Time's own target for its bake there is
900 s for a full 16k planet on an average gaming rig (its P1-19 gate). History time adds to that same
wait, so phase 6 gates it.

**The shared library (agreed with the Deep Time session, 2026-10-19).**
- **Format.** `deeptime-format` covers world tiles, the event log and the LZ4 + CRC32 codec. It is
  LGPL and already exists. The rtsim reads it and appends to it.
- **Natives: copy now, share later.**
  - The pack-wide host, `mods/mic-native` (§1a.1), starts from a copy of Deep Time's native
    toolchain. That toolchain is the 8 platforms via cargo-zigbuild and cargo-xwin, the `x86-64-v3`
    variants with `cpuprobe`, `natives.json` schema 2, and the Java `NativeLoader`.
  - To that copy the host adds the two pools, the governor, the profiler and the service registry.
  - Deep Time is changing that pipeline right now (PGO and a shipped allocator). It switches to
    depending on the host after its P1-19 timing gate closes.
  - Deep Time also ships outside MIC, so its jar keeps a fallback loader and probe. "Sharing" means
    depending on the host as a library (jar-in-jar plus a crate), not deleting Deep Time's copy.
- **Sharing threads.**
  - Deep Time and the rtsim share threads through the host's pools rather than by becoming one
    library. Deep Time keeps its own release cycle and licence. It can still become an rtsim module
    later, as Ben suggested.
  - **Deep Time's side needs a refactor.** Its pools carry per-session thread-locals, and six sim
    stages build small private pools. Moving them onto the host's per-job hooks (§1a.1) takes real work
    on Deep Time's side.
  - **Thread count needs no change.** Deep Time's results are identical at any pool size (its tests
    check 1, 3 and 8 threads), so the host may choose the size.
- **Licence:**
  - The rtsim engine and content are AGPL, and Deep Time is LGPL.
  - The host and the shared crates are what optimized forks of other people's mods will link. An AGPL
    host would make every such fork AGPL. That blocks GPL-2.0-only code. It also blocks giving
    optimizations back upstream under the upstream licence (Create is MIT).
  - Ben's decision (2026-10-19): MPL-2.0 for the host and shared crates. Its copyleft is per file,
    so it works with Rust's static linking, where LGPL's relinking terms are awkward. The rtsim engine
    and content stay AGPL.
  - MPL-2.0 crates can be linked into Deep Time's LGPL-3.0-or-later cdylib. Deep Time's `cargo deny`
    licence allowlist needs MPL-2.0 added.
  - Ben owns both Deep Time and the rtsim, so any choice works for those two.

---

## 9. Custom-roll ledger

Sizes are relative: S < M < L < XL.

| Item | Size | What it is |
|---|---|---|
| Pack-wide native host (`mic_native`) | L | loader, `cpuprobe` and natives tooling copied from Deep Time; foreground and background pools at OS priorities; governor; profiler and `/micperf`; service registry; `mic-native-rt` with a standalone fallback |
| Patch ledger + contract tests | S | `PATCHES.md`, plus one GameTest set per adapter (§1a.3) |
| Engine kernel | XL | clock, scheduler, timing wheel, storage facade, saves, RNG, profiler, determinism tests |
| Space and pathfinding | L | frames, world-model providers, coarse graphs (roads, lanes, routes), hierarchical paths |
| Knowledge | M | reports, witnesses, noticeboards, gossip by carrier, sentiment with caps and decay |
| JNI boundary and bridge core | L | buffers, hand-over, observers, quotas, puppet protocol, `shouldBeSaved` mixin, commands |
| CLI, bench, replay, viewer | M | headless runs, potato benches, flight recorder, HTML maps |
| Ecology | L | species data, forage kernel, herds, migration, materialized members |
| Civ | XL | cultures, factions, settlements, notables, plots, growth, ruins, Cobblemon adapters and dialogue |
| Trade | L | goods, markets with clamps, caravans, treaties, Numismatics and table cloth adapters, postbox market |
| Conflict | L | grievance ledger, escalation, war bands, raid director, Lanchester resolution |
| Goblin entity + custom humanoid + TaCZ goal | L | GeckoLib models and animations, AI goals; the art is the bulk |
| Dragons | M | dragon agent, hunting, IaF adapter and goals, bounded fire, In Control config |
| Naval | L | lanes, ship classes and schematics, Sable adapter, crew AI with CBC, hull snapshot, radar mixin |
| Industry | L | gauge snapshot, rate measurement, process sim, offline-buffer exchange, vault reconciliation |
| History generation + worldgen placement + road feature | XL | fast-forward driver, structure placement, templates per culture |
| Deep Time provider + event-log writer | M | after the shared library is agreed |
| Space | XL | after a space mod is chosen |

**Art and data** are the hidden bulk: building templates per culture and tier, ship schematics,
goblin models, Cobblemon NPC classes and dialogue, and species definitions.

---

## 10. Phases

Each phase ends with something Ben can see in game, and with the perf gates from §3.1 green on the
potato profile.

| Phase | Builds | Exit test |
|---|---|---|
| **0. Spikes** | JNI hand-over bench; `bevy_ecs` vs plain columns at 10⁵ and 10⁶ rows; the same herd kernel in Java and Rust on the potato profile; a GameTest of the puppet protocol (no duplicates across save, reload and crash); a shared-library agreement with Deep Time; the first `mic_native` host built from a copy of Deep Time's natives pipeline; the pack-level potato harness with one baseline scenario | numbers written into this plan; the Rust choice holds up against them (or §2 is reopened); a baseline of the whole pack recorded before any sim exists |
| **1. Kernel + ecology slice** | kernel, bridge, saves, `/rtsim` commands, Surveyor markers, herds of vanilla sheep migrating with the seasons | a herd of tens of thousands crosses the map over a season; walking in and out shows consistent animals; the 1-worker and N-worker state hashes match; bridge p99 ≤ 1 ms |
| **2. Civ core** | settlements adopted from vanilla villages, Cobblemon civilians, reports, sentiment, noticeboards, dialogue that reflects memory | the §7.2 walkthrough works in game |
| **3. Trade** | goods, markets, caravans carrying news, Numismatics accounts, table cloth stalls, the postbox market | the §7.3 walkthrough works |
| **4. Conflict** | grievance from Destroy pollution and logging, envoys, war bands, raid director, goblins, bandits, TaCZ goal | the §7.4 walkthrough works |
| **5. Dragons** | dragon agents, IaF adapter, hunting herds and livestock, bounties | the §7.5 walkthrough works |
| **6. History + growth** | history fast-forward, sim-placed settlements, roads, ruins, building projects and plots | a new world starts with a generated history, browsable in `rtsim-viewer`; villages visibly grow over a play week |
| **7. Naval** | lanes, ship classes, Sable adapter, crew, cannons, radar mixin | the §7.6 walkthrough works |
| **8. Industry** | districts, gauge snapshots, process sim, offline-buffer cargo, reconciliation | the §7.7 walkthrough works |
| **9. Deep Time** | Deep Time provider, event-log writer, shared natives, one native library | a Deep Time planet's history appears in Deep Time's viewer |
| **10. Space** | after a space mod is chosen | |
| **11. Evolution** | with Deep Time | |

**Tests at every phase:**
- `cargo test`, with property tests that check counts, goods and money are conserved across tier
  moves;
- deterministic replay;
- GameTests for adapters;
- one smoke test on the real pack server in the style of mic-climate's `smoke/`.

Builds and tests run on the laptop, niced, one heavy job at a time.

---

## 11. Risks and open questions

**Risks:**
1. **Puppet bookkeeping.** Duplicates or losses at chunk edges, crashes or mod interference. Phase 0
   writes the GameTest first.
2. **Abstract and observed results diverge** (X4). Abstract resolution is tuned against real fights in
   GameTests (§4.5).
3. **Economy drift** (Veloren). Clamps everywhere, and property tests.
4. **Cobblemon NPCs may not accept new behaviour types from mods.** If not, civilians use the custom
   humanoid with our own dialogue screen.
5. **Create internals are not an API.** `FactoryPanelBehaviour` fields, the offline buffer and the
   table cloth owner can change with Create versions. The adapters are kept small and version-checked.
6. **Sable sub-level to template capture** is unverified. If it fails, ship damage persists as a
   hull-integrity number instead of holes.
7. **Deep Time's format is exact-match and pre-release.** The provider breaks whenever Deep Time
   changes. It sits behind the provider trait.
8. **History generation time on a weak PC.** Deep Time's full 16k bake already takes about 21 minutes
   on a 32-vCPU VM. History adds to world creation. The history bench belongs in phase 6's gates.
9. **Optimization agents change the mods the adapters depend on.** The patch ledger and contract
   tests (§1a.3) catch this.
10. **No slack on the weakest PCs.** At the lowest priority, a PC whose two cores are always busy gives
    the sim only a small share, so it falls further and further behind. `/micperf` shows the lag. The
    pack's minimum spec must leave some slack.

**Answered by Ben (2026-10-19):** Rust core; `mic_native` as the pack-wide host; MPL-2.0 for the
host and shared crates.

**Agreed with the Deep Time session (2026-10-19):**
- bakes go on the foreground pool;
- the host starts from a copy of Deep Time's natives, and Deep Time switches after its P1-19 gate;
- pre-play history uses negative years (§8).

**Still open:** which space mod to use, when phase 10 comes.

---

## 12. What the prior art taught (research 2026-10-19)

Sources were read where possible; community claims are marked in the research notes.

| Lesson | Source | Where this plan applies it |
|---|---|---|
| Persist ids and small deltas; derive identity from a seed | Veloren `actor.rs`, MCA `FamilyTree` | §3.4, §3.6 |
| Flat tables with generational ids; any id can vanish | Veloren `lib.rs` | §3.6 |
| Rule plugins bind typed events; the host chooses each event's data access | Veloren `rule/mod.rs`, `event.rs` | §4.4 |
| Stagger per-entity work by hash; never tick a whole population | Veloren, MCA `Village`, Dwarf Fortress | §3.3 |
| Make logic dt-independent with hazard rates and closed-form catch-up | Veloren `ai/mod.rs`, Unloaded Activity | §3.3 |
| Entities are projections of sim state; keep the sim object alive across unload | Veloren `tick.rs`, Create `Train` / `Carriage` | §5.1 |
| Sleep and wake with deterministic merged wake lists; idle with intent plus prediction | Factorio FFF-67/364/421 | §3.3 timing wheel, §7.7 |
| Parallel phases that do not depend on order, then a serial apply | CK3 dev diary, Dyson Sphere Program dev log | §3.5 |
| Cap and prune every memory structure; drop unreferenced agents | Veloren `sentiment.rs`, DF cull, RimWorld | §3.6, §4.6 |
| Teleport abstract flows; make them physical only when observed | Dwarf Fortress world activities | tiers, §3.2 |
| Generate detail lazily but consistently; match entry and exit rates | Sunshine-Hill and Badler 2010 | §3.2, §3.4 |
| Abstract results must never beat observed ones; clamp every runaway loop | X4 community, Veloren economy | §4.5, §7.3 |

**Traps that killed similar projects, and our answer:**
- **Whole-population scans per tick** (Veloren rules, DF line of sight, Bannerlord hourly ticks). We
  use events, staggering and scheduled encounters.
- **Unbounded history** (DF figures, RimWorld pawns). We cap everything and cull.
- **Lossy handoff** (Veloren loses inventories). We use per-body capture adapters.
- **Village life tied to loaded chunks** (MineColonies and Millénaire freeze and break at chunk
  edges). Our engine does not care what is loaded.
- **Drifting economies.** Clamps and conservation tests.
- **Unmeasured per-rule cost** (CK3's 850,000 checks a month). `/rtsim perf` and CI bench deltas.

**Veloren's numbers, for scale:**
- Veloren has no official rtsim benchmark.
- Its simulated brains run every 10th tick. Its handoff and several rules scan every actor every tick,
  and it clones all of its data for each save on the tick thread.
- One third-party fork measured about 0.2 ms of rtsim per 1,000 NPCs. That is a single, noisy
  measurement.
- Our design avoids each of those per-tick whole-population costs.

---

## Appendix A. Verified facts this plan relies on (2026-10-19)

All were checked against the pinned jars (`mods/deep-time/smoke/mic/server/mods/`, the Prism
instance), Create's `mc1.21.1/dev` source, or the local source trees. Everything else in the plan is
marked unverified where it is used.

- **Create 6.0.10**
  - `GlobalRailwayManager.tick(level)` returns at once for any dimension other than the overworld. It
    then ticks every train with no chunk check.
  - A train stalls (`carriageWaitingForChunks`) only when a player rides it and the 16-block lookahead
    is not entity-ticking.
  - `CarriageEntityHandler` discards carriage entities in non-ticking chunks; `Carriage` and
    `Train.storage` stay.
  - Station arrival is graph-side. `StationUnloadedCondition` exists.
  - `GlobalStation.runMailTransfer` writes to `GlobalPackagePort.offlineBuffer` when the postbox chunk
    is unloaded.
  - `RailwaySavedData` is `create_tracks`. `LogisticsNetworkSavedData` is `create_logistics`.
  - `LogisticallyLinkedBehaviour.isValidLink` requires a loaded block entity.
  - `FactoryPanelBehaviour` holds `targetedBy` (inputs with amounts), `recipeOutput`, `recipeAddress`,
    `restockerPromises` and `network`. The target-stock field name is unchecked.
  - `TableClothBlockEntity` has `owner: UUID` and a `priceTag` filter.
  - `KineticNetwork` is runtime-only. `BlockStressValues.getImpact/getCapacity` exist.
- **Sable 2.0.5 / Aeronautics 1.3.2**
  - A ship is a `ServerSubLevel` in the same `Level`.
  - The spawn sequence comes from `SableSpawnCommands.executeSpawnSchematicCommand`.
  - `SubLevelHoldingChunkMap` freezes unloaded ships. `SubLevelLoadingTicketType` exists.
- **Ice and Fire CE 2.1-beta.1**
  - Entity classes, NBT keys and `IafDragonFlightManager.setFlightTarget(Vec3)` as listed in §6.
  - Pack config `iaf-common.json`: dragon griefing off, `canDespawn` true, `targetSearchLength` 128,
    `wanderFromHomeDistance` 40, `hungerTickRate` 3000.
  - In Control already spawns desert fire dragons.
- **Cobblemon 1.8.1**
  - `NPCEntity` with `VariableStruct` data. `NPCClass` fields as listed in §6. The dialogue API
    classes.
  - Herd leader groups cap at 9.
- **TaCZ:** a `LivingEntity` mixin makes every living entity an `IGunOperator`. There is no mob
  shooting goal.
- **Numismatics 1.1.0:** coin values 1, 8, 16, 64, 512 and 4,096 spurs. `GlobalBankManager.BANK` and
  `BankAccount` by UUID.
- **Surveyor 1.2.4:** `WorldLandmarks.of(Level)`, `Landmark.global`, `LandmarkComponentTypes`. Antique
  Atlas 3.1.2 draws landmarks.
- **Destroy (MIC fork, source):**
  - Pollution is stored as NeoForge attachments.
  - Per level: greenhouse, ozone depletion, acid rain. Per chunk: smog, radioactivity.
  - It spreads and decays only in loaded chunks.
  - PowerGrid has no emission signal.
- **Deep Time:**
  - finite Petroff-Guyou worlds, 13,312 × 6,656 (16k) or 26,112 × 13,056 (32k);
  - pure-Java mmap reader `WorldData`;
  - public API `DeepTimeClimate` (API_VERSION 4) and `DeepTimeSites`;
  - event log writer ids 2+ reserved for civ;
  - JNI via `jni =0.22.4` and rayon;
  - 8 platform builds plus `x86-64-v3` variants picked by `cpuprobe`;
  - not in the MIC pack (kept on a pack branch by decision of 2026-09-28).
- **Pack:**
  - `structurify.json` disables Terralith villages (vanilla villages remain);
  - `disable_villager_trading.js` removes villager trades;
  - Serene Seasons ships no config (8-day sub-season default);
  - ServerCore activation range is off;
  - `rctmod-server.toml` caps trainers at 60.
- **JVM:**
  - FFM is final in Java 22 and preview in 21.
  - The Vector API is incubating through JDK 25 and needs a launch flag.
  - Valhalla value classes target a JDK 28 preview.
