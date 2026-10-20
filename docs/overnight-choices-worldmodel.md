# Overnight choices: the world model (2026-10-20)

**Overnight agent 23.** Reversible choices made while Ben was asleep, one line each with the reason,
numbered X2-#. None is in DECISIONS.md, PLAN.md or OPEN-QUESTIONS.md; each can be undone or put to
Ben. Branch `overnight/worldmodel`. The work: the engine's first knowledge of terrain. A habitat
sampler in the core (`neoforge/core`, package `habitat`) reports 64-block cells as HABITAT records;
a new crate, `rtsim-space`, holds them as the world model behind a provider trait; the herds read
their climate and their founding from it, the ships their water mask; the native engine saves it in
its own page tables. Numbers are PLACEHOLDER unless marked measured.

**HABITAT is a boundary addition made overnight for Ben's review** (docs/boundary.md §10). Choice Y4
in docs/overnight-choices-herdsmc.md flagged such a record as his call; it was built as Y4
recommended.

## Vanilla facts checked in the 1.21.1 sources (2026-10-20)

Checked in NeoForm's decompiled 1.21.1 sources with NeoForge 21.1.249's patches
(`sourcesAndCompiledWithNeoForge_*_output.jar` in the Gradle cache).

- `ChunkAccess.getHeight(Heightmap.Types, x, z)` returns `getFirstAvailable(x & 15, z & 15) - 1`:
  the top block's y. `MOTION_BLOCKING` counts a block that blocks motion or holds a fluid;
  `MOTION_BLOCKING_NO_LEAVES` is that without leaves; `OCEAN_FLOOR` counts only motion-blocking
  material (`Heightmap.Types`).
- `ChunkAccess.getNoiseBiome(x, y, z)` takes quart coordinates and clamps y to the chunk's height.
- `ServerChunkCache.getChunkNow(x, z)` answers only on the server thread and only a chunk that
  reached FULL (else null); it never loads one.
- NeoForge posts `ChunkEvent.Load(levelChunk, newChunk)` in `ChunkStatusTasks.full`, on the server
  thread, after `LevelChunk.runPostLoad`, which adds the proto chunk's entities through
  `ServerLevel.addWorldGenChunkEntities`; `PersistentEntitySectionManager.addEntity(entity,
  worldGenSpawned)` posts `EntityJoinLevelEvent(entity, level, worldGenSpawned)`, so chunk-generation
  entities join with `loadedFromDisk` true, just before their chunk's Load.
- `NaturalSpawner.spawnMobsForChunkGeneration` calls `finalizeSpawn` with
  `MobSpawnType.CHUNK_GENERATION`; NeoForge's `Mob` keeps it (`getSpawnType()`, saved as the NBT
  `neoforge:spawn_type`).
- Vanilla biome values (`OverworldBiomes`): plains 0.8 and 0.4 (snowy plains 0.0 and 0.5), desert
  and savanna 2.0 and 0.0, forest 0.7 and 0.8 (birch 0.6 and 0.6), taiga 0.25 (snowy taiga -0.5),
  windswept hills 0.2, jungle 0.95, stony peaks 1.0, snowy slopes -0.3, grove -0.2, frozen and jagged
  peaks -0.7, river 0.5 (frozen river 0.0).
- Vanilla creature spawns: `BiomeDefaultFeatures.farmAnimals` is sheep 12, pig 10, chicken 10, cow 8;
  `plainsSpawns` adds horse 5 and donkey 1 (46 in all: sheep 260 per mille); savanna adds horse 1 and
  donkey 1 to the farm animals; frozen peaks, jagged peaks and snowy slopes spawn goats (weight 5);
  meadows sheep. `MobSpawnSettings.getMobs(MobCategory)` gives the weighted list, `SpawnerData.type`
  the entity type.
- `BlockTags.ICE` (`minecraft:ice`) and `FluidTags.WATER` exist; `BiomeTags` holds the vanilla
  `is_ocean` to `is_end`; NeoForge's `Tags.Biomes` holds the `c:` tags of docs/boundary.md §10.3.
- The GameTest server's world is the FLAT preset (`GameTestServer`: `WorldPresets.FLAT`, seed 0, no
  structures): superflat plains.

## The record and the sampler (Java)

| # | Choice | Why |
|---|---|---|
| X2-1 | **A new crate, `crates/rtsim-space`**: the `WorldModel` trait, the habitat cell and its codec (HABITAT, HABITAT_KNOWN), `SampledWorld`, its pages, and a test painter. rtsim-ecology and rtsim-ships depend on it and keep their adapters (`HabitatClimate`, `HabitatWater`) with them. | PLAN.md §5.1 names `rtsim-space` as the home of "the world-model provider trait (sampled vanilla, Deep Time reader)". Adapters next to the module they serve keep the trait free of module knowledge. |
| X2-2 | **Cells of 64 blocks** (SCALE.md §2), **one point per chunk**, at its centre column: 16 a cell. A cell is sampled only when all 16 of its chunks are loaded in full, so every HABITAT is a whole cell. | SCALE.md's field cell; one point per chunk is "sparse, a few points" and still resolves a 32-block water mask (four points). Whole cells keep latest-wins exact: no record ever carries half a cell over a full one. |
| X2-3 | **Sent once, then on change, through the engine's list**: at every session start the engine sends HABITAT_KNOWN (each held cell's fingerprint, per region) and END; Java samples nothing until END and sends a cell only when its fingerprint differs (or chunk-generation animals were counted). Java keeps no file of its own. | The engine's pages are the authority: a cell a crash took from the engine (pages older than the world save) is sent again at its next load, which a Java-side "sent" file could not know. Cost per session start: one record per region held (544 bytes at most). |
| X2-4 | **Kind ranges** 0x00A0 to 0x00AF (latest-wins), 0x0900 to 0x09FF (journaled, reserved, none yet), 0x8900 to 0x89FF (commands); **dictionary kind 0x30** for biome names. | Next to the conflict's 0x0090, 0x0800, 0x8800 (merged tonight) and its dictionary 0x20; clear of every range in docs/boundary.md and docs/industry-boundary.md. |
| X2-18 | **A cell is sampled at most once per load of its chunks**: an unload makes it eligible again; a new chunk in it resets it. The sampler's own `resample` (GameTests) forces one. | "Updates only on change": players' building shows at the next load without polling loaded chunks; an unchanged cell costs a sample (measured below) and sends nothing. |
| X2-19 | **The budget**: at most 8 cells and 0.25 ms of sampling a tick (PLACEHOLDERS), at the end of the tick (the core's `SessionPlugin.endOfTick`); a cell whose chunks are not all loaded leaves the queue at once. | A burst of chunk generation (a player flying) spreads over ticks; the queue holds cell keys only. |
| X2-12 | **Species come from each biome's own spawn data**: the record carries the herd species' share of the biome's `CREATURE` spawn weight (per mille, averaged over the land points), not a table of tags. | "Species chosen by biome from data": the data is the world's own (vanilla, datapacks, modded biomes such as Terralith's), so a pack's biomes need no table. |
| X2-13 | **Y20's adoption, as counting**: animals vanilla spawned at chunk generation (`MobSpawnType.CHUNK_GENERATION`) of the five herd species are counted per cell and sent; founding weighs them (0.5 habitat cells each) and floors the herd's size at 4 sim animals each. **The bodies are left to vanilla**, not taken into the herd. | Taking the bodies would mean materializing a herd group around existing vanilla-saved entities and swapping their ownership mid-tick: a new record and a capture path, not cheap. Counting is cheap (one event, one array) and makes generation spawns the founders' evidence. |
| X2-14 | **Latest wins per cell; generation counts only rise** (the engine keeps the highest it was sent). | A later session's resample counts no generation animals (none generate in an old chunk), and must not erase what was seen. |
| X2-7 | **A climate hook in the core**: `ClimateSource` (`season(server)`, `temperature(level, pos)`), registered with `RtsimApi.registerClimate`; vanilla otherwise (no degrees, no seasons). Its two methods have the shapes of the industry plugins' `IndustryPlugin.season` and `temperature` (branch `overnight/industry-plugins`, IND_CONDITIONS), so one plugin class can answer both. No source is implemented tonight. | The pack's sources (mic-climate's `Climate.celsius`, Serene Seasons) belong in a plugin (D4); `mic_rtsim_climate` is being scaffolded by the industry-plugins agent tonight, so this branch adds only the core hook. |
| X2-21 | **The stub engine keeps no habitat**: it answers SESSION_START with HABITAT_KNOWN END alone. | The stub simulates nothing; the bridge's sampler must still start. Its unit test now expects the extra command. |

## The engine side (rtsim-space, rtsim-ecology, rtsim-ships, rtsim-jni)

| # | Choice | Why |
|---|---|---|
| X2-5 | **Unknown ground is avoided**: a herd's 128-block forage cell is passable only when every point in it is known; a ship's 32-block cell is water only when all four of its points are known open water; founding waits for enough of a region. | The brief's rule for lazy regions: nothing plans across ground nobody has seen; the sampled area grows with players, so the world opens as they explore. |
| X2-6 | **A dimension the world model knows nothing of keeps the old behaviour**: the herds' climate answers as Y3's uniform grassland and the ships' mask as U11's all-water. | Tests and tools that found herds or ports by order (HERD_FOUND, SHIP_COMMAND, the protocol tests) run unchanged, and a dimension nobody sampled behaves as before tonight. Once one cell of a dimension is known, X2-5 holds there. |
| X2-8 | **Degrees on mic-climate's scale**: vanilla's temperature t is -20 + 30.4 (t + 0.5) °C, clamped to -0.5 to 2.0 (Project Atmosphere's mapping, as mic-climate 0.2.0's `BiomeSeasonFallback`): plains 19.5 °C, snowy plains -4.8 °C, desert and savanna 56 °C. A plugin's degrees replace it, less the season they were taken in; a dimension where a plugin reported seasons adds mic-climate's default offsets (spring -2, summer +4, autumn -1, winter -8 °C) by sub-season. | One scale for the sim and the pack's climate mod. Consequence: herds find deserts and savannas far too hot (horses' comfort band ends at 28 °C, H2-6), so savanna horse herds drift out where they can. |
| X2-9 | **Forage from the ground**: a forage cell's capacity is 2,000 (Y3's) × its land share × the mean vegetation index of its land (by tags: open grassland 1, forest 0.6, taiga 0.5, jungle and mushroom 0.4, slopes 0.35, snowy 0.2, peaks and shores 0.1, desert and badlands 0.05, water 0; sparse vegetation × 0.6, dense × 0.85); growth 0.1 a day at 8 to 30 °C, ramping to nothing at 0 °C and at 45 °C; frost decays forage up to 0.02 a day. TUNABLE. | The herds' `Climate` trait asks for forage dynamics per cell and sub-season (H2-8); these follow the biome and the temperature, the two things the record carries. |
| X2-10 | **A herd's terrain**: a forage cell with more than half its points water, or any lava or void, is closed; otherwise its cost is 1 + 4 × water share + 1 × ice share + relief / 48 blocks. TUNABLE. | "A herd never plans across water": the herds' Dijkstra never enters a closed cell nor cuts a corner past one (H2-11), so the sea is a wall; a river ford or a frozen lake costs more but stays crossable. |
| X2-11 | **Founding**: a region (512 blocks) is judged once at least a quarter of its points are known (its "first sample"); one herd when the herd species' habitat there adds up to 2 cells, the species drawn by habitat weight with a draw keyed by the region, placed at the centre of the passable forage cell with the most of the species' habitat; size = density × the species' habitat cells (sheep 24, cow 10, pig 12, horse 6, goat 8 a cell; 8 to 4,000). The region is marked in its page when decided (founded or barren) and never judged again; a region not ready is judged again when it next changes. Only while `Module.ECOLOGY` is on; 64 regions a step. A herd already named for the region (pages lost) counts as founded. TUNABLE. | Y4's recommendation ("one herd per suitable region on its first sample, species by biome"); a quarter is the least that can say whether a region is grassland or sea. The name guard keeps a crash that lost the mark from founding twice. |
| X2-20 | **The founding seed is a constant** (PLACEHOLDER), as HERD_SEED. | The bridge does not send the world's seed (as the dragons' S14, the ships' U1). |
| X2-15 | **Pages**: table 0x5700 holds the meta page (the biome and body name tables, the seasonal dimensions, the revision and the state hash), table 0x5701 one page per region (its marks and cells). A save writes the meta page and the regions changed since the last save, every region after a save failure. A load recomputes the state hash from the pages and logs a difference; pages this build cannot read are deleted at start and the world model starts empty (Java resends as chunks load). | The persist pattern (magic, version, state hash, loud drop); a save costs what players explored since the last one, not the world. Deleting at start keeps a stray region page from failing every later load. |
| X2-16 | **The climate is not periodic-pure while the model grows**: the herds' forage closed form assumes a climate that repeats every year. Unknown ground is closed, so it holds no forage row and turns known harmlessly; a known cell changes only when players rebuild its ground, and a grazed row then rereads its past sub-seasons with the new rates. | Re-anchoring rows on every change would need a change feed from the model into the forage field; the error is bounded by one grazed cell's regrowth. |
| X2-17 | **The ships' water mask**: 32-block cells (the ships' own, T9), water only when all four points are known open water; ice is not water. | A lane must never cross land (the brief's test); four points a cell is what one point per chunk gives at 32 blocks. |
| X2-22 | **The world model is shared through one `WorldHandle`** (a provider behind a lock): the step writes HABITAT at its observe point and every reader runs inside the same step, so the lock is never contended. A Deep Time provider replaces the sampled one behind the same handle; `WorldModel::sampled` then answers `None` and HABITAT is counted and ignored. | SCALE.md §12 and PLAN.md §11: the provider is a trait so a Deep Time plugin can take over; modules ask by block rectangle (SCALE.md §2's "queries, never storage") and never see cells. |
| X2-23 | **`nHabitatInfo`** (JNI, `Engine::habitat_info`): a cell's masks, surface and fingerprint, the counts, and the region's founded herd with its position; `NativeEngine.habitatInfo`. | The GameTests read the engine's own world model (as `nHerdInfo`, Y22). |

## Results

To be filled in below as runs finish.

## Not done

To be filled in.
