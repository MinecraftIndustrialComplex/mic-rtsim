# Could rtsim fix GregTech's late-game lag? (2026-10-21)

**Verdict.** Mostly no: GregTech's famous lag is in the loaded base where the player stands, and rtsim leaves loaded chunks alone.
The top causes are machines re-searching recipes they cannot start, per-block-entity tick overhead, cable and pipe caches wiped on every block update, AE2, and client chunk rebuilds; they are GT's own code, and GT is fixing them with exact sleep-and-wake.
rtsim fits the minority case of bases kept chunk-loaded only so they keep producing (offline players on servers, remote rigs, a base left behind while its owner is in space): a GT glass-box plugin could make those cost about zero and credit exact-in-rate output.
Cost: a GTM plugin about the size of rtsim's Create plugin (16k Java lines) plus an AE2 plugin, aimed at a 1.21.1 GTM that is not released as stable and that no pack runs.
GTNH (1.7.10) would need rtsim's whole Java side rewritten and the GTNH team's consent; rtsim's Rust engine is the only part that carries over unchanged.

Research only: nothing was built or run. Three Sonnet agents read the web and read-only clones under
`/tmp/gt-research/`; claims below were spot-checked against those clones where a file:line is given.
Anything not checked is marked UNVERIFIED.

**Sources and notation.**
- GT5-Unofficial (GTNH) at `77d7f359` (2026-10-21); `G/` is its `src/main/java/`. `PR#n` is a
  GT5-Unofficial pull request, `GTNH#n` an issue in GT-New-Horizons-Modpack.
- GregTech CEu Modern (GTM): branch `1.20.1` at `aae6d4e` (the 8.0.0 snapshot, 2026-10-21), tag
  `gtm-7.5.3` (`91a79b8`), branch `1.21` at `36a6b38` (2026-10-17). `M/` is
  `src/main/java/com/gregtechceu/gtceu/` on the `1.20.1` branch. `GTM#n` is an issue or PR in
  GregTechCEu/GregTech-Modern, `ST#n` in StarT-Dev-Team/Star-Technology.
- **Profile numbers are as quoted in the issue or PR text.** Every spark paste the agents tried either
  returned 404 or was not opened, so no profile was decoded here. The ranking is by strength of
  evidence, not by a measured share of one tick.

---

## 1. Where GregTech's late-game lag comes from

### 1.1 GTNH (1.7.10, GT5-Unofficial)

| Rank | Cause | Evidence | What GTNH already does |
|---|---|---|---|
| 1 | **Recipe checks and input polling** | A maintainer calls recipe checks "the biggest offender" ([PR#4469](https://github.com/GTNewHorizons/GT5-Unofficial/pull/4469)). A parallel recipe check costs about 0.1 ms, with spikes near 1 ms at recipe start ([GTNH#17083](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/17083)). An octo Volcanus on advanced stocking buses costs 100 to 400 µs per tick each, inside `ProcessingLogic.process()` ([GTNH#21251](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/21251)). Iterating proxy hatches was "even more expensive than recipe check for endgame base" ([PR#6893](https://github.com/GTNewHorizons/GT5-Unofficial/pull/6893)). | Event-driven recipe checks replaced the roughly 100-tick polling ([PR#7141](https://github.com/GTNewHorizons/GT5-Unofficial/pull/7141), [PR#7185](https://github.com/GTNewHorizons/GT5-Unofficial/pull/7185)); `recipeCheckFailCooldown` defaults to 0, "the legacy behaviour was roughly equivalent to 100 ticks" (`G/gregtech/common/config/MachineStats.java:110-117`). Recipe trie, a 256-slot XOR-hash cache and `lastRecipe` (`G/gregtech/api/recipe/RecipeMapBackend.java:92,95,473,532`); recipe lock and a per-pattern CRIB cache (`G/gregtech/api/logic/ProcessingLogic.java:385-440`). Batch mode (below). |
| 2 | **Per-tile-entity overhead of blocks with nothing to do** (hatches, wireless hatches, laser pipes) | Every loaded GT tile entity runs `updateEntity()` each tick unless it opts out with `tryDisableTicking()`, which any cover blocks (`G/gregtech/api/metatileentity/CommonBaseMetaTileEntity.java:113-122`). Disabling ticking on auto-taping maintenance hatches, stocking hatches and mufflers took one world from about 45 to 40 mspt, with a spark comparison showing more than 10 mspt saved ([PR#5900](https://github.com/GTNewHorizons/GT5-Unofficial/pull/5900)). 20k laser vacuum pipes cost 6 mspt in Opis ([PR#7682](https://github.com/GTNewHorizons/GT5-Unofficial/pull/7682)). | Tick opt-outs in those three PRs and for wireless hatches ([PR#7409](https://github.com/GTNewHorizons/GT5-Unofficial/pull/7409)). Cables check connections only every 20 ticks (`MTECable.java:264-269`). |
| 3 | **Block-update flood fill and hatch packets** | A structure update visited "100k+" blocks on any change; cut to about 8k for a synchrotron ([PR#4865](https://github.com/GTNewHorizons/GT5-Unofficial/pull/4865), fixing [GTNH#21167](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/21167)). Background structure-check threads piled up after a wand paste ([PR#4588](https://github.com/GTNewHorizons/GT5-Unofficial/pull/4588), [PR#4592](https://github.com/GTNewHorizons/GT5-Unofficial/pull/4592)). Hatch update packets went from about 2,500/s to 10 to 20/s ([PR#5480](https://github.com/GTNewHorizons/GT5-Unofficial/pull/5480)). | Structure checks run only when `mStructureChanged` is set or forced (`G/gregtech/api/metatileentity/implementations/MTEMultiBlockBase.java:566-582`); the flood limits above. |
| 4 | **Chunk loading and simulation distance** | Hodgepodge's simulation distance (`addSimulationDistance_WIP`, off by default; `simulationDistance` 32) caused the 2.8.0 regressions, one user seeing about 200 ms every second ([GTNH#21612](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/21612), [GTNH#21804](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/21804)). A half-loaded AE2 system "will tend to tank the TPS by performing constant chunk load/unload" ([GTNH#15081](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/15081)). How many chunks real bases keep loaded: UNVERIFIED. | ServerUtilities chunk loading is off by default, capped per rank ([wiki](https://wiki.gtnewhorizons.com/wiki/ServerUtilities)). GT's own tickets: `G/gregtech/api/objects/GTChunkManager.java`. |
| 5 | **AE2 and its addons** | A dimension went from 10 to 12 ms to 80 ms per tick after 2.8.1, fixed by removing the AE2FC fluid auto filler ([GTNH#22109](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/22109)). A level emitter on an item a stocking bus uses costs several ms ([GTNH#14978](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/14978)). AE2 copied its item list on every retrieval ([GTNH#14061](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/14061)). | Fixed in AE2-Unofficial PR#363 and AE2FC PR#150; 2.9.0 polls external inventories less and re-sorts terminals less ([changelog](https://github.com/GTNewHorizons/DreamAssemblerXXL/blob/master/releases/changelogs/changelog%20from%202.8.4%20to%202.9.0-RC-2.md)). ME output hatches flush at most every 40 ticks (`MTEHatchOutputMEBase.java:203,486-503`). |
| 6 | **Client rendering and client ticking** | FPS fell from 140 to 20 when machines toggle, because `markBlockForUpdate` rebuilds the chunk mesh ([GTNH#14625](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/14625)). An async-profiler capture flying around an endgame base: `World.updateEntities` from 36.21% to 12.91%, `CommonBaseMetaTileEntity.updateEntity` from 17.44% to 2.29%, by disabling client ticking of GT tile entities that need none ([PR#7563](https://github.com/GTNewHorizons/GT5-Unofficial/pull/7563); `G/gregtech/api/metatileentity/BaseMetaTileEntity.java:282-290`). | That PR; Angelica for chunk meshing. |
| 7 | **Item, fluid and cable networks** | Item pipes holding items flood-fill the network every 10 ticks per pipe (`MTEItemPipe.java:219-255`, `IMetaTileEntityItemPipe.java:54-121`); fluid pipes act every 5 ticks and scan for gas leaks (`MTEFluidPipe.java:317-343`). The graph-based power net cut cable load from 4.48% to 1.39% of CPU (2021, [PR#572](https://github.com/GTNewHorizons/GT5-Unofficial/pull/572)). Old evidence. | The graph power net; wireless EU, a per-team ledger refilled every 2,000 ticks (`G/gregtech/common/misc/WirelessNetworkManager.java:63-72`), which removes cables altogether in the late game. |
| 8 | **Covers** | They tick per side at their tick rate; a pump cover's minimum rate is 1 (`CoverableTileEntity.java:149-195`, `CoverPump.java:121-123`). No profile found. | Players can slow cover ticking ([PR#2307](https://github.com/GTNewHorizons/GT5-Unofficial/pull/2307)). |
| 9 | **Entities and items** | 75k+ tile entities including 101k floodlight phantom lights made `ArrayList.contains` 65% of a profile ([GTNH#22355](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/22355)); 200k entities from a Tesla tower ([GTNH#12623](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/12623)). Not GT's code. | |

**GTNH batch mode.** For recipes under 128 ticks it folds extra parallels into one longer run, capped
at 128 ticks (`G/gregtech/api/util/ParallelHelper.java:25,511-527`). The wiki says it gives "NOT any
sort of time discount" and exists "to reduce lag by lowering the number of recipe checks"
([wiki](https://wiki.gtnewhorizons.com/wiki/Multiblock_Machines); [PR#2174](https://github.com/GTNewHorizons/GT5-Unofficial/pull/2174)).

**GTNH official servers** give each endgame base "a budget of 15ms of load", and laggy setups are
removed "WITHOUT REFUND" ([Official Servers](https://wiki.gtnewhorizons.com/wiki/Official_Servers)).
Whether that budget counts bases of offline owners: UNVERIFIED.

### 1.2 GTM (1.20.1 Forge, the 1.21.1 port, and its packs)

**Version status.**
- 1.20.1 stable is `v7.5.3-1.20.1` (2026-04-12), on LDLib 1.0.40.b. The 1.20.1 branch is the rolling
  "8.0.0 SNAPSHOT": LDLib removed (MUI2 port, [GTM#3298](https://github.com/GregTechCEu/GregTech-Modern/pull/3298),
  merged 2026-06-21), its own sync system, and `MetaMachine` is itself a block entity.
- **1.21.1 NeoForge:** the only tagged releases are `v7.0.0-1.21.1` and `v7.0.2-1.21.1` (2025-07-20).
  Since then, only the rolling "1.21-8.0.0 SNAPSHOT" (2026-10-17). **No stable 1.21.1 release in 15
  months, and no pack found on it** (UNVERIFIED negative).
- Packs are all 1.20.1: Star Technology (its own fork, GTM-StarT-Fork 1.7.1c), Monifactory 0.13.8
  (2026-08-27, Forge 47.4.13), TerraFirmaGreg. Their exact GTM versions beyond that: UNVERIFIED.

| Rank | Cause | Evidence | What GTM already does |
|---|---|---|---|
| 1 | **RecipeLogic re-searching every tick** when inputs match but the recipe cannot start (sub-tier energy hatch, a failing condition); pattern buffers and proxies multiply the search paths | An idle machine unsubscribes only if `lastFailedMatches == null` (`M/api/machine/trait/recipe/RecipeLogic.java:278-286`), so a matched-but-blocked machine searches every tick. Sub-tier voltage causes "massive lag" ([GTM#3700](https://github.com/GregTechCEu/GregTech-Modern/issues/3700); fix [#3735](https://github.com/GregTechCEu/GregTech-Modern/pull/3735) reverted by [#3772](https://github.com/GregTechCEu/GregTech-Modern/pull/3772)). About 1 ms per idle machine on buffers or proxies with a lower-tier hatch ([GTM#3554](https://github.com/GregTechCEu/GregTech-Modern/issues/3554), [GTM#4471](https://github.com/GregTechCEu/GregTech-Modern/issues/4471), both open). Duplicate handlers give 56 search lists instead of 4, 2 to 20 ms/t per machine ([GTM#4726](https://github.com/GregTechCEu/GregTech-Modern/issues/4726); dedup PR #5251 closed). About 1,200 ms in RecipeLogic in a posted profile ([GTM#3218](https://github.com/GregTechCEu/GregTech-Modern/issues/3218)). | Tick subscriptions: workable machines sleep when idle and wake on handler changes (`M/api/machine/MetaMachine.java:270-312`). A stalled Large Combustion Engine stops ticking until notified ([GTM#5356](https://github.com/GregTechCEu/GregTech-Modern/pull/5356), merged 2026-10-11). Last-recipe cache (`RecipeLogic.java:387-400`). A failed-match backoff is planned for 9.0 ([GTM#5434](https://github.com/GregTechCEu/GregTech-Modern/issues/5434)). |
| 2 | **Very short recipes (1 to 3 ticks)**, per-tick recipe I/O and output-merge simulation, worst with ME hatches | Star Technology at 13 TPS; the maintainer's diagnosis was 1 to 3 tick recipes; the server tick went from about 41,000 to 13,640 µs after the player's changes ([ST#253](https://github.com/StarT-Dev-Team/Star-Technology/issues/253)). 48 ms/tick in `ParallelLogic.limitByOutputMerging` with subtick overclock and ME hatches ([GTM#2809](https://github.com/GregTechCEu/GregTech-Modern/issues/2809)). | Batch mode, 100-tick batches ([GTM#3292](https://github.com/GregTechCEu/GregTech-Modern/pull/3292), `M/common/data/GTRecipeModifiers.java:143-162`), sold as improving "server performance for large, late-game bases" ([docs](https://gregtechceu.github.io/GregTech-Modern/1.21/Gameplay/Electricity/Machines/)). Parallel logic work (#3088, #2668, #2684), fast ingredients (#3270). |
| 3 | **EnergyNet cache invalidation** | `NET_DATA.clear()` on every neighbour update and connection change (`M/common/pipelike/cable/EnergyNet.java:41-55`). About 40% of a large server's tick; a third-party hotfix mod brought it to 16% and then 7% of the original ([GTM#4396](https://github.com/GregTechCEu/GregTech-Modern/issues/4396), [hotfix](https://github.com/barkerh4200-spec/gtceu-energynet-hotfix), self-reported). | A pipenet rewrite is underway, unmerged (`gus/pipenet-rewrite`). |
| 4 | **AE2** | Crafting parallelism above about 64k drops TPS to about 1 ([ST#502](https://github.com/StarT-Dev-Team/Star-Technology/issues/502)). The pattern buffer exposes 27 slots as distinct buses (`MEPatternBufferPartMachine.java:436`), feeding cause 1. A pattern buffer rewrite took an ABS tower from TPS 3 to "little lag" (author's claim, [GTM#4910](https://github.com/GregTechCEu/GregTech-Modern/pull/4910), 8.0 snapshot only). | ME buses and hatches sync every 40 ticks (`M/config/ConfigHolder.java:259-263`). Monifactory ships its own AE2 patches (MAE2, MoniLabs mixins; contents not read). |
| 5 | **Per-pipe fluid tickers, conveyor covers on large inventories, computation multiblocks** | Every fluid pipe holds a server-tick subscription and distributes every few ticks (`M/common/blockentity/FluidPipeBlockEntity.java:95-103,155`). Conveyor covers on three Create vaults of about 1,600 stacks: TPS 10 to 15 ([GTM#3759](https://github.com/GregTechCEu/GregTech-Modern/issues/3759), closed as an inventory-API limit). 456 µs/t at about 1,000 CWU/t ([GTM#3639](https://github.com/GregTechCEu/GregTech-Modern/issues/3639)). | Item pipes have no per-pipe tick and use route caches (`ItemPipeNet.java:19-21,68`). |
| 6 | **LDLib sync and the async structure thread (7.x)** | LDLib registers every machine in a per-level 50 ms async thread that updates all non-lazy fields; server cost UNVERIFIED. The 7.x structure thread needed a `getBlockState` mixin measured at 6% slower ([GTM#4727](https://github.com/GregTechCEu/GregTech-Modern/pull/4727)). No profile shows structure checks as a top cost. | 8.0 drops LDLib and makes structure checks event-driven, with one check 2 ticks after load (`M/api/machine/multiblock/MultiblockControllerMachine.java:96-101`). 8.0 still calls `setChanged()` on every machine every tick (`ManagedSyncBlockEntity.java:139-150`). |
| 7 | **Client** | GUI FPS cost fixed by the LDLib removal ([GTM#1957](https://github.com/GregTechCEu/GregTech-Modern/issues/1957)). A client direct-buffer leak proportional to machine activity, cause unknown ([GTM#4571](https://github.com/GregTechCEu/GregTech-Modern/issues/4571), open). | |
| 8 | **Not GTM** | Pack lag reports blamed on Create stress and steam boilers (ST#257, ST#277), TFC spawning and items (TFG#2709, TFG#3004). GTM ships no chunk loader (no `ForgeChunkManager` or `TicketController` in its source); chunk-loading cost in GTM packs has no evidence either way. | |

**Other fixed hot spots:** Large Centrifuge chance code was O(N²) ([GTM#4205](https://github.com/GregTechCEu/GregTech-Modern/issues/4205), fixed by #4210 and #4211); skipping empty handlers cut the max tick from above 5,000 ms to about 500 ms ([GTM#4909](https://github.com/GregTechCEu/GregTech-Modern/pull/4909)). Upstream 1.12.2 had the same class of fixes ([GT12#1374](https://github.com/GregTechCEu/GregTech/pull/1374), [GT12#2487](https://github.com/GregTechCEu/GregTech/pull/2487)).

### 1.3 Across both lines

**The top three, by evidence:**
1. **Recipe search churn.** A machine looks for work it cannot do, every tick or every few ticks, and
   ME stocking buses, pattern buffers, proxies and 1 to 3 tick recipes multiply the cost. This is the
   best-evidenced cause in both lines (GTNH PR#4469, GTNH#21251; GTM#3700, #3554, #4471, #4726,
   ST#253).
2. **Per-block-entity overhead of things that have nothing to do.** In GTNH every GT tile entity ticks
   unless it opts out; GTM machines keep a vanilla ticker even when their logic sleeps (GTNH PR#5900,
   PR#7682; GTM 8.0's per-tick `setChanged()`).
3. **Networks that throw their caches away.** GTM's EnergyNet wipes its route cache on any neighbour
   update (GTM#4396, about 40% of a tick); GTNH's structure floods visited 100k+ blocks per change
   (PR#4865); GTNH item pipes flood-fill every 10 ticks.

**What the three share:** each is work that does not change the result. A machine that cannot start,
a hatch with nothing to move and a cable whose topology did not change all cost the same every tick.
The fix both teams are converging on is exact sleep with a wake on the event that matters, which keeps
results identical: GTNH has largely done it (event-driven recipe checks, tick opt-outs), and GTM is
doing it in 8.0 and 9.0 (#5356, #5434).

**What is not well evidenced:** how much of a typical server's load is chunk-loaded bases whose owners
are elsewhere. That share is the part rtsim could remove, and no profile measures it.

---

## 2. What rtsim does that is relevant

- **The unit is a district,** a group of chunks closed under physical connection and cut at countable
  links and stores (INDUSTRY.md §2.1; DECISIONS.md SEC-1: "sections split at stores ... and each
  section learns, replays and goes stale on its own").
- **Black box (C19, built):** boundary stores are read round-robin under a per-tick inventory and slot
  budget (`neoforge/core/.../industry/Sampler.java:83-113`). Each good's slope must agree between the
  two half-windows within ε before a rate is trusted (`crates/rtsim-industry/src/learn.rs:617-660`).
  Unloaded, a district runs at its learned rates between breakpoints (an input empties, an output
  fills, a learned target is reached), with integer error diffusion (INDUSTRY.md §4,
  `crates/rtsim-kernel/src/conserve.rs:222`), at a cost of O(breakpoints + events), independent of
  elapsed time (INDUSTRY.md §15).
- **Glass box (C28; G1 on main, G2 on `feat/glassbox-g2` branches):** compiles the real machine graph
  with the mod's own formulas and solves it for steady rates, replaying at `clamp(measured, L, U)` and
  never above what was measured (GLASSBOX.md §3.5). Cycle counts come from running the mod's own
  countdown on the JVM at compile time (G-B5). Chance outputs enter at exact expectation
  (`crates/rtsim-industry/src/glass/records.rs:318-332`, G-B11).
- **Reconcile:** on chunk load the deltas go into the boundary stores once, with an epoch kept in a
  chunk attachment so a crash or reload cannot apply them twice
  (`neoforge/core/.../industry/Reconciler.java:34-58`, INDUSTRY.md §5).
- **Loaded chunks are not rtsim's.** A district is live, partial or unloaded by its block-ticking
  chunks (`neoforge/core/.../industry/Districts.java:344-373`), and SCALE.md §7 says it plainly:
  "Loaded district: Create itself; cost: Minecraft's, not the sim's." While loaded, rtsim only samples.
- **Measured overhead** on four real Create worlds: 0.2% to 0.5% of the server thread, about 0.1 to
  0.25 ms per tick (docs/stress-worlds-results.md §1, §3). The worst single rtsim tick was a detection
  pass of 90 to 160 ms every 5 minutes on 39 districts (§7).
- **Chunk loaders become optional:** "Chunk-loading never makes a factory worse. It is worth its cost
  only where real-time behaviour matters" (INDUSTRY.md §9).
- **Size today:** the Create plugin is 16,154 Java lines, 9,311 of them glass-box extraction; core
  industry Java is 9,186 lines; the Rust industry crate is 26,133 lines, 9,627 of them glass box.
  Nothing in rtsim knows GregTech, AE2 or EU, and it has never run on a GT world.

---

## 3. Fit

### 3.1 Unloading a GT base instead of chunk-loading it

**What it saves:** everything the unloaded part costs today. Machines, pipes, covers, hatches and the
AE2 devices in those chunks stop ticking, and the replay costs O(breakpoints + events). For a server
that keeps offline players' bases loaded, that is the whole cost of those bases, and it is the only GT
lag cause where rtsim removes the cost itself rather than going around it.

**What breaks, and what a GT plugin would do:**

| Concern | GT fact | Treatment |
|---|---|---|
| **AE2 requests** | AE2 destroys a grid node when its chunk unloads, in 1.21.1 (`AENetworkedBlockEntity.java:75-78`) and in GTNH's fork (`AENetworkProxy.java:95-105`), so a half-loaded grid splits; GTNH#15081 says half-loaded grids tank TPS. AE2 itself answers unloaded networks with a chunk loader (Spatial Anchor, `ChunkLoadingService.java:35-44`). Players already notice AE2 stopping exports when nobody is online ([GTNH#20142](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/20142)). | **An AE2 grid must be one district, all loaded or all virtual,** which I2's closure gives if the plugin merges every chunk a grid touches. Steady lines and stocking (level emitters, GTNH level maintainers) become learned targets (I6). **On-demand autocrafting while unloaded is not simulated:** nobody is requesting then anyway, and a request from a loaded terminal reaches only loaded nodes. |
| **ME storage as the boundary** | GTM's ME output bus keeps an uncapped `KeyStorage` buffer, saved with the block entity, and flushes it to the grid every 40 ticks (`M/integration/ae2/machine/MEOutputBusPartMachine.java:52,84-96`). GTNH's ME output hatch caches and flushes every 40 ticks or more. | The grid's cached inventory is the boundary stock, read through an AE2 plugin. ME output buses are the ideal reconcile target: unlimited, persisted, and flushed by GT itself on load. |
| **Power buffers** | GTNH multiblocks drain EU every running tick, and a shortfall calls `stopMachine(POWER_LOSS)`, which zeroes progress and **voids the recipe's outputs** (`MTEMultiBlockBase.java:1064-1079,1454-1477`). GTM takes EU per tick and fails the tick on a shortfall (`RecipeLogic.java:431`, `M/api/machine/trait/notifiable/NotifiableEnergyContainer.java:302-327`). GTNH wireless EU is a per-team global ledger (`WirelessNetworkManager.java:63-72`). | EU is a good and battery buffers are stores (INDUSTRY.md §7). Running out of EU is a breakpoint, and in GTNH it must also book the voided in-flight recipe, or the sim beats reality. Wireless EU is a store that can be read and written while unloaded, like rtsim's Ender Transmission channels. |
| **Multiblock state** | GTNH does not save `mMachine` (formed) and waits 100 ticks after load (`mStartUpCheck = 100`, `MTEMultiBlockBase.java:185`) before checking structure and running. GTM saves `isFormed` and re-checks 2 ticks after load (`MultiblockControllerMachine.java:77-80,96-101`). Both freeze recipe progress on unload and save it. | Reconcile writes on the first tick after load (INDUSTRY.md §5), before either GT resumes. In-flight work freezes and resumes, as rtsim already assumes for Create. A structure that fails its re-check on load makes the district stale. |
| **Chance outputs** | GTM: chanced outputs carry a per-machine accumulator with only a random initial offset ("OR Chanced outputs are deterministic", `M/api/recipe/chance/logic/ChanceLogic.java:40-77,299-330`). Its long-run rate is exactly the expectation. GTNH: true RNG, a Gaussian approximation when mean ± 3σ fits in [0, parallels], else one roll per parallel (`G/gregtech/api/util/ParallelHelper.java:593-624`). | rtsim's exact expectation by integer diffusion (G-B11) matches GTM in rate. For GTNH it matches the mean and drops the variance. |
| **Upgrades and settings** | Hatch tiers, coils, parallel hatches, programmed circuits, batch mode, voiding, input separation and cover settings all change rates. They are saved block-entity state. | A fingerprint over machine type, tier and these settings (G-B3's approach). Any change makes the district stale, so it freezes until relearned or recompiled. |
| **Maintenance (GTNH)** | 1 in 6,000 per 1,001 running ticks, per multiblock (`MTEMultiBlockBase.java:1501-1531`). | A counter-RNG hazard draw per slot (SCALE.md §4). Late-game bases use auto-maintenance hatches, so it is often moot. |
| **Pollution (GTNH)** | Mufflers vent pollution into per-chunk values ticked in a world tick handler (`polluteEnvironment`, `MTEMultiBlockBase.java:893-940`; `G/gregtech/common/pollution/Pollution.java:99-270`). Whether pollution spreads into unloaded chunks: UNVERIFIED. | As rtsim's Destroy pollution plugin does (I14): attribute emissions by position while loaded, apply them on load with the epoch. |
| **Things that cannot be learned** | Machine controller covers, wireless redstone and CC-style logic; GTNH research and data sticks; multiblocks with start-up costs (fusion) or variable recipes. | The observed duty cycle, or "not learnable", which freezes the district while unloaded. That is today's behaviour, so the sim is never worse (I5). |

**Where this pays off:**
- multiplayer servers with many bases kept chunk-loaded while their owners are offline;
- remote rigs (oil and fluid drilling, mining outposts);
- a base left loaded while its owner is in another dimension.

None of these has been measured as a share of lag (§1.3), so the win is real but of unknown size.

### 3.2 Sleeping loaded, steady factories

rtsim has no such mode, and building one would be a new design, not a plugin. It would work like this:
1. A section that passes the steady test (or a glass-box verdict) has its block entities taken off the
   level's ticker list. GTNH already does this for idle tiles with `World.func_147457_a`
   (`CommonBaseMetaTileEntity.java:113-122`), and Lithium does it for vanilla block entities.
2. Its boundary stores are credited at the model's rates in batches.
3. Any event (a player interaction, a GUI, a neighbour update, a store reaching a bound, a power change)
   wakes it: it catches up and goes back to real ticking.

**Why it is a poor trade for GT:**
- **The expensive work is not in steady state.** A running GT machine's tick is cheap: progress plus
  one, an EU drain (`RecipeLogic.java:333`, `MTEMultiBlockBase.java:1064-1079`). The cost in §1 is in
  transitions: recipe searches, output merging and ME flushes. Where every tick is a transition (1 to
  3 tick recipes), batch mode already folds the transitions, exactly and with GT's own semantics.
- **A machine stuck searching is steady at zero rate.** Sleeping it until its inputs, power or tier
  change is GTM's planned failed-match backoff (#5434) and #5356, which need no rate model.
- **Frozen inside state shows.** Progress bars, GUIs, Jade/WAILA, active textures, sounds and redstone
  covers that report activity would all freeze. GTNH players read those constantly.
- **Hidden state must be written back on wake,** or GT diverges from the replay: progress, the EU
  buffer, the chance cache, maintenance counters and output buffers. That means invasive mixins per
  machine class into code that 8.0 is renaming.
- **Skipping ticks breaks code keyed to the tick counter.** Aikar's verdict on Spigot's tile tick
  limiter: "skipping ticks actually FULLY BREAKS Some tile entities"
  ([post](https://aikar.co/2015/10/08/spigot-tick-limiter-dont-use-max-tick-time/)). GT is full of
  `aTick % n` and offset-timer logic.
- **AE2 keeps ticking regardless:** the grid is the boundary, and its own tick manager runs.

**What carries over from rtsim is the discipline, not the mechanism:**
- a cost formula per system with CI slopes (SCALE.md §0, §10);
- scheduling the next moment a machine can act (a timing wheel) instead of polling for it;
- running the mod's own formulas for exact counts (G-B5).

Upstream, these are small patches. Through rtsim, they would be a whole subsystem.

### 3.3 Accuracy GT players would accept

- **GT suits a glass box better than Create does.** Every recipe has a fixed duration and EU/t, and
  overclocking and parallels are deterministic integer math, fixed at recipe start (GTNH
  `MTEMultiBlockBase.java:821-883`; GTM `OverclockingLogic.java:198-249`, `GTRecipe.getTotalRuns()`).
  There are no kinetics, belt spacing or arm timing to calibrate. A compiled model can reproduce a GTNH
  flowchart exactly in rate.
- **The black box would not be accepted.** A learned slope inside ±ε, replayed "at or below" what was
  measured (G-B2), is the kind of shortfall the exact-ratio culture notices first. Mysda's Create
  factory needed a 20-minute quiet window before its verdict (docs/stress-worlds-results.md §4.2), and
  long GT recipes would need longer.
- **What players have accepted:** GTNH developers accept lumpy output as long as totals do not change.
  Batch mode is "shackled (aka only performance improvements, no balance impact)" ([GTNH#14002](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/14002)),
  and the official servers already impose per-base load budgets. DSP's SampleAndHoldSim "sacrifices
  short-term accuracy for long-term consistency" and is popular for it. Factorio players took the
  approximate fluids 2.0 because "the fluid system consumed up to 20% of the tick time"
  ([FFF-416 thread](https://forums.factorio.com/viewtopic.php?t=113959&start=60)).
- **The bar, then:** the same totals per hour as a loaded base, chance outputs at expectation, and a
  never-better-than-real rule that GTNH would check against its calculators. Players exploit whichever
  way an abstraction errs (the X4 lesson, INDUSTRY.md §1).

### 3.4 Fit per cause

| Cause | rtsim, unloaded | A sleep mode while loaded | The better fix |
|---|---|---|---|
| Recipe search churn | gone for unloaded sections only | would sleep stuck machines, which needs no rates | event-driven checks and a failed-match backoff (GTNH done; GTM #5356, #5434) |
| Per-tile tick overhead | gone for unloaded sections | gone for slept sections, at the visibility cost of §3.2 | tick opt-outs (GTNH PR#5900, #7409, #7682); dropping GTM 8.0's per-tick `setChanged()` |
| Network cache invalidation, floods | gone for unloaded sections | no effect: an update wakes the section | incremental invalidation; the GTM pipenet rewrite |
| AE2 | its nodes leave the grid, so the grid must be all virtual | no: AE2 keeps ticking | AE2 and pack fixes (AE2-Unofficial #363, MAE2) |
| Chunk-loaded bases | **the direct fit**: no loaders needed, output credited exactly | n/a | none: GT has no catch-up of its own |
| Client rendering | none | could stop texture toggles, but that hides activity | fewer `markBlockForUpdate` calls (GTNH#14625), Angelica |
| Entities and items | none | none | not GT |

---

## 4. Size of the job

### 4.1 A GTM plugin for 1.21.1 NeoForge

**Prerequisite:** a GTM for 1.21.1 that someone plays. Today there is only 7.0.2 (July 2025) and a
rolling 8.0 snapshot, with no pack on it (§1.2), and 8.0 renames much of the API rtsim would hook. The
realistic target is GTM 8.0 once it is released for 1.21.1.

**Hooks that exist:**
- `IRecipeLogicMachine` callbacks (`recipeLogicStatusChanged`, `beforeWorking`, `onWorking`,
  `onWaiting`, `afterWorking`; `M/api/machine/feature/IRecipeLogicMachine.java:76-129`) can be attached
  to existing machines through `ModifyMachineEvent` and `MachineBuilder` (`MachineBuilder.java:141-150,282-296`)
  without mixins. Note that `afterWorking` also fires on interrupts (`RecipeLogic.java:647-648`).
- `RecipeLogic` and `MetaMachine` are not final, and `@SaveField` state is enumerable for
  fingerprints.
- The addon API is `IGTAddon` with `@GTAddon`. There is no event-bus event for "recipe finished".

**Pieces, sized against what rtsim already has:**

| Piece | Size | Notes |
|---|---|---|
| GT boundary stores | S | super and quantum chests and tanks, through capabilities; `NotifiableEnergyContainer` as EU stores |
| AE2 plugin | M | grid storage as boundary stock; reconcile through ME output buses or grid insert and extract; whole-grid district closure; crafting jobs not learnable. Useful to any AE2 pack, not only GT |
| GT glass-box models | L, about the Create glass extraction (9k Java lines) | machine nodes from `RecipeLogic` and the server's `GTRecipeType` lookup; overclock, parallel, batch and coil modifiers evaluated on the JVM through GTM's own `fullModifyRecipe` at compile time (as G-B5 does for Create); multiblocks as one node with their hatches; cables as capacity edges (voltage, amperage, loss); generators as fuel-burning sources; pipes and covers as movers with filters |
| EU as a good, power-loss void, wireless EU stores | S to M | new to the engine's goods dictionary, otherwise as PowerGrid batteries |
| GameTest corpus | M | as G1's 12-factory corpus |
| Distribution outside MIC | M | rtsim core plus the `mic_native` Rust library per OS. D1 rules distribution out as a concern for MIC, but GT packs would ask for it |

**Total:** roughly the Create plugin again (15k to 20k Java lines) plus a few thousand lines of Rust.
The engine (`cluster.rs`, `learn.rs`, reconcile, journal, paging) is reused unchanged. G2 is not on
main yet, so the glass-box replay a GT plugin needs does not exist on main either.

### 4.2 GTNH on 1.7.10

- **What carries over:** the Rust engine and the JNI bridge (JNI works on Java 8).
- **What does not:** the whole Java side (core 32,431 lines, 9,186 of them industry) is NeoForge 21.1
  code:
  - capabilities (`Capabilities.ItemHandler.BLOCK`);
  - data attachments for the reconcile epoch;
  - `FullChunkStatus.BLOCK_TICKING`;
  - ticket controllers;
  - NeoForge events;
  - GameTests.

  1.7.10 has `IInventory` and `ISidedInventory`, Forge's old `IFluidHandler`, GT's own energy
  interfaces and AE2 rv3. There are no data components or attachments: the epoch would go in chunk NBT
  through `ChunkDataEvent`, as GT's own `GTChunkAssociatedData` does (`G/gregtech/api/util/GTChunkAssociatedData.java:488`).
  Whether GT uses `ChunkEvent.Load/Unload` on 1.7.10: UNVERIFIED (GT itself does not). There is no
  GameTest framework. **This is a rewrite of the Java side,** not a port: core industry, a GT5U
  plugin, an AE2-Unofficial plugin and a test harness.
- **Toolchain:** GTNH's buildscript compiles to Java 8 bytecode with multi-release classes for Java 17
  to 25 (jvmDowngrader, `gradle.properties:47-60`), and UniMixins with GTNHMixins is required.
- **Model breadth:** GT++, TecTech, GoodGenerator, BartWorks and KekzTech now live inside GT5U, with
  many bespoke multiblocks (fusion start-up EU, research, the Eye of Harmony, the quantum computer),
  each needing its own model or "not learnable".
- **Honest blockers:**
  1. The Java rewrite and the lack of GameTests.
  2. The GTNH team: mods ship through DreamAssemblerXXL, and the team has refused automation shortcuts
     before ("Tick acceleration will not be added for automation purposes",
     [GTNH#10230](https://github.com/GTNewHorizons/GT-New-Horizons-Modpack/issues/10230)).
  3. Shipping a native library in GTNH: UNVERIFIED whether any GTNH mod besides lwjgl3ify does.
  4. The lag GTNH players complain about is in the base they stand in, which this does not touch.

---

## 5. Prior art

| What | Exact or approximate | Lesson |
|---|---|---|
| **AE2 tick manager** (1.21.1): `TickRateModulation` SLEEP to URGENT (`api/networking/ticking/TickRateModulation.java:26-56`); sleeping and awake sets plus a priority queue per level (`me/service/TickManagerService.java:53-55,101-146`); `alertDevice` wakes (:183-211); devices get `ticksSinceLastCall`, and the Molecular Assembler scales progress and power by it (`MolecularAssemblerBlockEntity.java:378,413`) | exact for linear progress | Sleep plus a complete set of wake events plus an "elapsed ticks" argument. Nothing in AE2 progresses while unloaded. |
| **Lithium block entity sleeping**: swaps the ticker for a no-op, `setChanged()` wakes it (`SleepingBlockEntity.java:10-47`); vanilla types only; NeoForge build official, ports Canary and Radium | exact by design | Works only where "nothing to do" is cheap to state and the wakes can be hooked. GT is not covered. |
| **Spigot `max-tick-time`** for tiles: disabled by Paper ([docs](https://docs.papermc.io/paper/reference/spigot-configuration/)); "skipping ticks actually FULLY BREAKS Some tile entities" (Aikar) | broke results | Never skip ticks without replaying what they would have done. |
| **Vanilla simulation distance, ServerCore, Hodgepodge's simulation distance, Create: Tick Control, Tick Tweaks** | change results (things slow or stop) | Throttling is opt-in for a reason; GTNH's attempt regressed (GTNH#21612). |
| **Create: LazyTick** ([GitHub](https://github.com/duckgun13476/Create-LazyTick)): backoff for idle or blocked machines with event wakes and recipe caches | mostly exact, "visible delay" | The GT-shaped fix, done for Create. |
| **Meanwhile** ([GitHub](https://github.com/KURONAMI333/meanwhile), v0.1.0): on chunk load, ticks a block entity once, reads what changed in its data and repeats that per missed tick; completions run through real logic; no item movement between machines | approximate per block | The closest shipped Minecraft prior art to rtsim's replay; its gap (flow between machines) is what districts and the glass box solve. Not in INDUSTRY.md §1 yet. |
| **Unloaded Activity, The Block Keeps Ticking, Offline Progression** | closed form per mechanic, or real ticks under caps | Already in INDUSTRY.md §1. |
| **GT batch mode** (GTNH, GTM) | exact in totals; one chance roll per batch in places ([PR#8428](https://github.com/GTNewHorizons/GT5-Unofficial/pull/8428)) | GT's own aggregation, accepted because totals do not change. |
| **Mindustry** sectors: production rates kept as a 60-sample window while played (`SectorInfo.java:21-23,260-265,354-359`), and `mean * secondsPassed` capped by storage for sectors not being played (`Universe.java:206-213`) | approximate, nearly free | A shipped rate replay with storage caps; rates must be re-measured whenever the factory changes. |
| **DSP SampleAndHoldSim** (`UpdatePeriod` 10) and **Weaver** (cheaper logic for remote planets, about 3x) | approximate; stocks can go negative | Never overdraw; keep bursts apart from steady flow. Already in INDUSTRY.md §1. |
| **Factorio**: inserter sleep (FFF-67: 6/7 asleep, updates per second almost doubled), deterministic wake order (FFF-364), fluid manager (FFF-271, 6.5x), fluid segments (FFF-416, "a fairly large step back in realism"), robots skipping up to 20 to 60 ticks on predicted paths (FFF-421) | exact except fluids and robot positions | Exact sleep first; approximate only where players can live with it. |
| **Shapez 2**: far clusters update a few times a second and stay exact through a time-delta model, up to 500,000 buildings ([devlog 027](https://steamcommunity.com/games/2162800/announcements/detail/4469355336712585839)) | exact | Low-frequency updates stay exact when the model is in elapsed time. GT recipes already are. |
| **Captain of Industry**: skipping recipe searches when inputs did not change made 1,500 idle machines "7 times better" ([CD#37](https://coigame.com/post/cd-37)); integer simulation math | exact | Exactly GT's top cause, fixed in place; caches "introduce multiple sources of truth". |

**No mod or proposal was found that simulates GregTech machines while unloaded or replaces loaded GT
machines with rates,** in either line (a search negative: GitHub across both orgs, the web and the
wikis; reddit search is poor).

---

## 6. If Ben wants to go further (nothing here is decided)

1. **Cheapest and most useful for GT:** nothing in rtsim. Exact sleep-and-wake patches in GT itself:
   a failed-match backoff for GTM, and route caches that survive a neighbour update.
2. **First rtsim step, if any:** an AE2 plugin, the one piece any AE2 pack would use. It would also
   tell us whether whole-grid districts work at all.
3. **A GT glass box:** only once GTM 8.0 ships for 1.21.1 and G2 is on main. Measure first: profile a
   real GTM server's chunk-loaded bases of offline owners, because that share is the size of the
   prize and nobody has measured it.
