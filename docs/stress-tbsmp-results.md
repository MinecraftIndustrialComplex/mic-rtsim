# TBSMP Season 2 on its own pack, with and without rtsim: results, 2026-10-21

Ben's ask: "can you not just grab their pack and add in the rtsim mod for it w/ compatability for
any of the create mods they have that we don't". This page is the answer: the TBSMP Season 2 pack
rebuilt at the world's own versions, booted with all 11 dimensions, run with and without mic-rtsim
side by side, and the rtsim compat each Create addon MIC lacks needed. The world is the one found by
`research/stress-worlds-smp-2026-10-21.md` (section 4.1). The world, the pack and every jar stay in
`~/mic-stress-worlds/tbsmp-season2-1.21.1/` (outside git) for private local testing and are never
redistributed. No player data and no player names are in this page or in git (train names hold
player names; only counts are given).

## 1. Bottom line

1. **The pack is rebuilt and boots, with every dimension.** 142 entries at the exact versions in
   `level.dat` (119 Modrinth, 8 CurseForge, 15 jar-in-jar), as a packwiz pack. The original pack
   (NeoForge 21.1.188, Create 6.0.6) boots the world clean. rtsim targets Create 6.0.10, so the test
   instance moves to NeoForge 21.1.249 and Create 6.0.10; 11 jars had to move with it (section 2.2).
   The world loses one mod: Create Egg Production (40 egg collectors, 7 incubators), which has no
   build for Create 6.0.7+.
2. **rtsim runs on it with no rtsim error or warning in the logs.** Four adopt areas (two overworld
   bases, one in `thebrokenscript:day_b`, one in `codmod:entropy`) made 6 districts holding 1,142
   inventories in 3 dimensions. Its cost: about 0.3 ms per tick of inventory sweeps, 7 detections
   (237 ms in all), and one 366 ms glass box compile of the 1,787-node day_b district.
3. **Paired MSPT (on minus off): -1.1 idle, +2.3 busy, +0.7 unloaded, +4.5 reloaded,** every window at
   20 TPS. The A/A control (section 4) says how much of that is two servers differing.
4. **Compat took three small changes, no new block, item or entity (D3):** Create Encased's machines
   get a glass box fingerprint, Garnished's fan recipes are named by their type, and Sophisticated
   Storage's limited barrels and shulker boxes are district stock. Slice and Dice's slicer is the one
   addon machine rtsim still does not model (open question 1).
5. **Trains:** 55 trains and 101 stations in 5 dimensions. rtsim saw the one scheduled train inside a
   scene district (day_b): 12 arrivals (2 simulated while unloaded) and 12 departures.
6. **What broke was the pack's, not rtsim's:** five Create addons call members Create 6.0.7+ removed
   (one crashed the server), Moonlight broke on the newer NeoForge, and two mods bind fixed ports so a
   second server on the machine fails (one of them then hangs the JVM at stop).
7. **DS-10 (only machines seed) works here, with one regression:** adopted seeds fall from 17,789 to
   4,555 and the storage-only district is gone, but the day_b district shrinks from 84 to 69 chunks
   and loses its train station, so rtsim stops seeing that train (section 7, open question 2).
   Alone on the machine, the rtsim server stopped clean in 6 s.

## 2. The pack

### 2.1 Rebuild

- `~/mic-stress-worlds/tbsmp-season2-1.21.1/pack/` is a packwiz pack (`pack.toml`, `index.toml`, 127
  metafiles) at NeoForge 21.1.188 and every mod version `level.dat` records. The 15 jar-in-jar
  libraries come inside their hosts. Built by `tools/stress-worlds/tbsmp_pack.py` from Modrinth and
  CurseForge lookups by name and version string.
- Every jar, with its sha1, project and version ids, source and the reason any was changed, is in
  `research/stress-worlds-2026-10-21-data/tbsmp/pack-manifest.json`. The manifest holds no jar.
- Two lookups needed a correction:
  - alltheleaks 0.1.16 was never published; 0.1.17 is the nearest build.
  - createfood: `level.dat`'s "2.0.0" is the version string of the 2.0.0 betas. Both instances run
    2.0.0-beta-2 (CurseForge file 6607000), not Modrinth's 2.0.0.
- Control boots (no scene, no rtsim): the original pack in 32 s wall (live heap 2,047 MB), the
  bumped pack in 54 s (2,054 MB). Both loaded every dimension and saved clean.

### 2.2 The bump to Create 6.0.10 (test instance only)

Every mod jar was checked against Create 6.0.10, NeoForge 21.1.249, Moonlight 2.29.33 and the
Sophisticated mods with `tools/stress-worlds/linkcheck.py`: it reads each jar's constant pool and
reports every class, field or method the jar calls that the provider jar no longer has. Create 6.0.7+
removed `FluidIngredient`, `Contraption.presentBlockEntities` and `deferInvalidate`, and the Stream
form of `SequencedAssemblyRecipe.getRecipes`; 6.0.10 changed `RecipeApplier.applyRecipeOn` to take a
fourth argument.

| Mod | World ran | Test instance | Why |
|---|---|---|---|
| NeoForge | 21.1.188 | 21.1.249 | MIC's version; rtsim builds against it |
| Create | 6.0.6 | 6.0.10 | rtsim targets 6.0.10 |
| Moonlight | 2.18.13 | 2.29.33 | NeoForge 21.1.215+ changed `PoiStateSet`, which its POI mixin targets |
| Garnished | 2.1.6.1 | 2.1.6.7 | the old `applyRecipeOn`; crashed the first fan holding a freezing recipe |
| Slice and Dice | 4.0.1 | 4.2.3 | the Stream `getRecipes`; 4.2.3 declares Create [6.0.10,7) |
| Create: Framed | 1.6.3 | 1.7 | `presentBlockEntities` |
| Some Assembly Required | 6.0.6 build | 5.2.5 | `FluidIngredient` |
| Sophisticated Core / Storage / Backpacks | 1.3.33 / 1.4.28 / 3.24.9 | 1.3.89 / 1.5.15 / 3.25.13 | the floor the clean Create integrations need |
| Sophisticated Storage / Backpacks Create Integration | 0.1.8 / 0.1.1 | 0.1.13 / 0.1.5 | `presentBlockEntities`, `deferInvalidate`; the first clean builds |

Every intermediate version of the five broken addons was checked too (`linkcheck-candidates`): each
still had the miss until the version chosen. After the bump, every remaining finding outside
Supplementaries is a `changed?` false positive: vanilla `CreativeModeTab.Output` default methods, and
one client-only Moonlight call.

- **Removed:** Create Egg Production. No NeoForge build exists for Create 6.0.7+. The world loses 40
  egg collectors and 7 incubators (their blocks load as air).
- **Known break kept:** Supplementaries 3.1.7 with Moonlight 2.29.33 lacks
  `IScreenProvider.sendOpenGuiPacket` and `HolderReference.getHolderUnsafe`. Only a player opening a
  one-player GUI block, or a recipe display, reaches them; a headless server never does. Kept at the
  world's version.

## 3. Method

The harness of `docs/stress-worlds-results.md` section 2 (`tools/stress-worlds/sw.py`), with:

- **Instance:** the bumped pack (126 server jars), Java 21, G1, `-Xmx5G`, under `nice -n 10`. Arm
  "on" adds `mic_native`, `mic_rtsim`, `mic_rtsim_create` and `mic_rtsim_carryon` (the only plugin
  whose host mod is in the pack), built from this branch at 54bc16c.
- **World:** a `cp --reflink=always` copy of the source world per run; the source is never written.
- **Scene** (`scenes/tbsmp.json`, picked from a per-chunk block entity census of the overworld,
  day_b, entropy and the nether): idle 60 s; forceload four 12 x 12 chunk areas (two overworld
  bases, the densest day_b base, the entropy base); settle 180 s; `rtsim district adopt` at each
  centre with radius 6; busy 300 s; forceload off, unloaded 90 s; forceload on, reloaded 90 s.
- **Pairs:** two servers side by side on their own ports (slots 42 and 44), started 5 s apart.
  `tbsmp-ab` is off then on; `tbsmp-aa` is off and off.
- **Per window:** MSPT mean, p95 and max of the 5 s samples (each sample is NeoForge's mean over the
  last 100 ticks), process CPU per tick, server thread CPU per tick, TPS, heap after a full GC
  (`jcmd`), RSS, `save-all flush` time, and rtsim's `/rtsim district perf`, `list` and `info`.

## 4. With and without rtsim

| Window | A-off mean | p95 | max | B-on mean | p95 | max | on - off |
|---|---|---|---|---|---|---|---|
| idle | 12.68 | 18.45 | 33.84 | 11.57 | 16.12 | 37.10 | -1.11 |
| busy | 42.18 | 48.94 | 50.56 | 44.47 | 52.57 | 53.70 | +2.29 |
| unloaded | 5.60 | 6.00 | 6.00 | 6.30 | 7.25 | 7.25 | +0.70 |
| reloaded | 40.52 | 42.92 | 43.19 | 45.03 | 48.74 | 51.03 | +4.51 |

- TPS 20.0 in every window of both arms. Machine load (1-minute average) about 9 in the busy and
  reloaded windows of both.
- Boot: 82 s wall off, 90 s on (Done reported after 18.3 s and 16.0 s).
- Heap live after GC: 2,474 MB off, 2,491 MB on. RSS 5,945 MB and 6,270 MB.
- `save-all flush`: 1.97 s off, 2.08 s on. rtsim's save directory: 138 kB.

A/A control (`tbsmp-aa`, both arms off, the same scene and slots):

| Window | A-off mean | p95 | max | B-off mean | p95 | max | B - A | TPS A / B |
|---|---|---|---|---|---|---|---|---|
| idle | 25.56 | 35.71 | 131.25 | 27.96 | 30.03 | 153.52 | +2.40 | 20.0 / 20.0 |
| busy | 59.88 | 86.87 | 113.72 | 60.15 | 89.68 | 115.22 | +0.27 | 17.0 / 17.1 |
| unloaded | 14.43 | 21.41 | 21.41 | 10.49 | 16.86 | 16.86 | -3.94 | 20.0 / 19.6 |
| reloaded | 133.24 | 196.73 | 200.63 | 139.11 | 205.91 | 207.49 | +5.88 | 9.6 / 8.8 |

This A/A ran while other agents' builds and GameTests loaded the laptop: a load of about 46 in idle,
18 in busy and unloaded and 34 in reloaded, against about 9 for the A/B. Boot took 400 s, the busy
window ran at 17 TPS and the reloaded one at 9. So it bounds the noise between two identical servers
under load (up to 6 MSPT on a window mean), not the noise at the A/B's load.

AA2_TABLE

## 5. rtsim on TBSMP

### 5.1 Districts

| Adopt centre | Dimension | Block entities adopted | Districts | Chunks | Inventories |
|---|---|---|---|---|---|
| 3216, 1232 | overworld | 5,305 | 2 | 22 + 6 | 176 + 55 |
| 3552, -1904 | overworld | 4,532 | 2 | 32 + 1 | 239 + 1 |
| -1968, -1104 | thebrokenscript:day_b | 6,323 | 1 | 84 | 635 (1 station) |
| -960, 3200 | codmod:entropy | 1,629 | 1 | 29 | 36 |

(The adopt counts are the scene's own; districts per centre are read from the order of `list`.)

### 5.2 Cost (B-on, adopt to the end of the reloaded window, about 11 minutes)

- Engine: 15,558 steps; the server waited on the engine in 21 ticks.
- Detection: 7 runs, 237 ms in all.
- Inventory sweeps: 576 sweeps read 109,632 inventories and 77.7 million slots in 3.92 s, about
  0.3 ms per tick. The slowest single inventory read took 35 ms (most likely a large Create vault).
- Glass box compiles (last compile of each district): the day_b district, 1,787 nodes, 366 ms;
  the larger overworld districts, 1,209 nodes in 26 ms and 890 nodes in 2 ms; entropy, 273 nodes in
  0.5 ms. The two small overworld districts (56 nodes and 1 node) finished their 6,000-tick window
  with a pass. The three larger ones recompiled after the reload and were 100 to 300 ticks into a
  new window at the end. The day_b district never went live: 2 of its 84 chunks were not ticking
  (outside the forceloaded area), so it stayed partly loaded. Each district has its residual
  black-box node (GLASSBOX.md section 5.1), here feeding emeralds and bone meal from machines rtsim
  does not model (Easy Villagers' traders and farms among them).

### 5.3 Trains and stations

From the source world's `create_tracks.dat` (`tools/stress-worlds/train_census.py`):

| Dimension | Trains | Scheduled | Stations |
|---|---|---|---|
| overworld | 49 | 4 | 91 |
| thebrokenscript:day_b | 1 | 1 | 3 |
| thebrokenscript:nightmare | 2 | 0 | 4 |
| the_nether | 2 | 0 | 2 |
| codmod:backrooms | 1 | 0 | 1 |
| codmod:entropy | 0 | 0 | 0 |

97 track graphs. The one scheduled train in a scene district is day_b's: rtsim saw it make 12
arrivals at the district's station (2 of them simulated while the district was unloaded) and 12
departures. rtsim reported no train at the overworld and entropy districts.

## 6. Create addon compat

What each addon MIC lacks needed, checked for seeding, grouping, modelling, recipe types reaching the
glass box, and storage not taken for production. Counts are block entities in the census chunks
(whole world) and inside the four adopt areas.

| Addon | In the world | In the adopt areas | Finding | Change |
|---|---|---|---|---|
| Create Encased (createcasing) | 12,098 (7,098 encased shafts, 2,428 gearboxes, 391 depots, 176 presses, 105 mixers) | 1,448 | Its press, mixer and depot extend Create's classes but had no glass box fingerprint, so they were modelled as unknown | 19aaf5c: a block entity built on a Create class takes that class's fingerprint (GameTest) |
| Garnished | fan recipes; 26 decorative block entities | 0 | Its fan recipe types (freezing, dye blowing) were named as blasting | 084c3de: a fan recipe is named by its type (GameTest) |
| Sophisticated Storage | 3,737 chests, 1,035 barrels, 129 limited barrels | 611 chests and barrels, 5 limited barrels | Chests and barrels are stock through `#c:chests` and `#c:barrels`; limited barrels and shulker boxes were not | 54bc16c: both in `#mic_rtsim:district_stock` |
| Sophisticated Storage controller, link, I/O, input, output | 31 / 6 / 4 / 20 / 14 | 23 | Proxies onto other storage: not read as inventories today (only stock and plugin inventories are). Under DS-10 they would seed a district | for DS-10: `#mic_rtsim:district_not_seed` (section 7) |
| Slice and Dice | 36 slicers, 18 sprinklers | 5 slicers | The slicer is claimed as a kinetic block; items under it pass through the model unprocessed | none yet (open question 1) |
| Some Assembly Required, Create Food | 300 placed sandwiches | 0 | Their recipes use Create's own recipe types, which the glass box already reads | none |
| Create: Framed | 145 sliding doors | 0 | Decorative, no capability | none |
| Rechiseled: Create | 2 mechanical chisels | 0 | Negligible | none |
| Lootr | 27,487 chests | 25 | Stock through `#c:chests`; never a seed | none |
| The Broken Script console | 49 in the day_b area | 49 | No capability: neither a seed nor read | none |
| Easy Villagers (not a Create addon) | iron farms, auto traders, farmers, incubators | 108 | Ticking, with item capabilities: seeds, and their output (emeralds, iron, crops) reaches the glass box only through the residual black-box node | none needed |
| Create Egg Production (removed, section 2.2) | 40 collectors, 7 incubators | 7 | Lost with the bump | none |

GameTests: the `glass_extract_fp` batch (with the two new tests) passed. The one failure in that run,
`aheldreconcileapplieswhenitschunkloads`, is an industry test this branch does not touch.

## 7. DS-10 (only machines seed a district)

DS-10 (`fix/district-seeds`, not yet on main) makes only machines seed a district: never a block in
`#mic_rtsim:district_stock` or `#mic_rtsim:district_not_seed`, else a modded block entity that ticks
and has a capability. Run as one arm on a build of this branch with DS-10 merged in (no commit) and
Sophisticated Storage's controller, storage link, I/O, input, output and decoration table added to
`district_not_seed`:

| | A/B on arm (main's rule) | DS-10 arm |
|---|---|---|
| Seeds adopted (four areas) | 5,305 / 4,532 / 6,323 / 1,629 = 17,789 | 802 / 1,268 / 1,738 / 747 = 4,555 |
| Districts | 6 | 5 (the 1-chunk, 1-inventory overworld district is gone) |
| Overworld districts, chunks and inventories | 22 + 6 + 32 + 1 chunks, 471 inventories | 20 + 6 + 31 chunks, 451 inventories |
| day_b district | 84 chunks, 635 inventories, 1 station | 69 chunks, 442 inventories, 0 stations |
| entropy district | 29 chunks, 36 inventories | 27 chunks, 36 inventories |
| Train arrivals at district stations | 12 (2 simulated), 1 train seen | 0, no train seen |
| Detections | 7, 237 ms | 7, 136 ms |
| Inventory sweeps | 576, 109,632 inventories, 3.92 s | 465, 86,014 inventories, 3.21 s |
| day_b glass box | 1,787 nodes, 366 ms | 1,593 nodes, 270 ms |
| Stop | crashed at stop (CoD mod port, second server) | clean, 6 s, exit 0 |

The run was alone on the machine (load about 6 to 10), so its MSPT is not paired: busy 50.8,
unloaded 5.3, reloaded 42.7 (TPS 18.8, 20, 20). Boot 36 s wall.

- Storage no longer seeds: the one district that held only an inventory dissolved, and the
  overworld districts lost 4 chunks (that one among them) and 20 inventories. Sophisticated
  Storage's chests and barrels (district stock) and its proxies (`district_not_seed`, added for this
  run) start no district. Lootr's chests are stock. The per-block breakdown of the seeds is not in
  the report, so this is read from the district shapes, not counted block by block.
- **The regression:** the day_b district's station chunk holds no machine, so under DS-10 it is not a
  member, the district loses its station, and rtsim no longer sees the scheduled train that called
  there 12 times in the A/B. Any station built in its own chunk beside a factory will do the same.
  See open question 2.
- Sophisticated Storage 1.5's wooden storage connectors (`*_storage_connector`) belong in the same tag;
  none are in this world.

## 8. What broke, and the fixes

| What | Cause | Fix |
|---|---|---|
| Server crash on the first fan with a freezing recipe | Garnished 2.1.6.1 calls the 3-argument `applyRecipeOn` | Garnished 2.1.6.7, and the link check over every jar (section 2.2) |
| Moonlight 2.18.13 cannot run on NeoForge 21.1.249 | its POI mixin targets `PoiStateSet`, which NeoForge 21.1.215+ changed | Moonlight 2.29.33 |
| `BindException` in the second server of a pair | Prometheus Exporter binds 0.0.0.0:19565 | 613c319: sw.py gives each slot its own loopback port (26400 + slot) |
| Second server crashes at stop, then never exits | CoD mod's API server binds 0.0.0.0:15017 hard-coded; the server that reaches it second cannot bind it, and its stopping handler then throws a NullPointerException. After any crash, non-daemon threads (LuckPerms scheduler, spark, Netty) keep the JVM alive | 8501edf: sw.py records the crash and ends the server a minute later. It hit B-on in the A/B and A-off in the A/A (B booted first under load); it costs nothing before the stop |
| Link checker missed calls into classes whose super is `java.lang.Object` | treated as unknown | `Object`, `Enum` and `Record` are known roots |

## 9. Viewer

`rtsim-cli view <world> --port 8745` runs on the B-on world
(`~/mic-stress-worlds/tbsmp-season2-1.21.1/runs/tbsmp-ab-B-on/world`) at http://127.0.0.1:8745/,
loopback only.

## 10. Choices made (all undoable)

- The test instance runs NeoForge 21.1.249 and Create 6.0.10; the packwiz pack keeps the world's own
  versions. The 11 moved jars are the oldest versions the link check passes, not the newest.
- Egg Production dropped; Supplementaries 3.1.7 kept with its known GUI break.
- alltheleaks 0.1.17 for the unpublished 0.1.16.
- The scene's four areas and their dimensions (overworld twice, day_b, entropy); the nether's Create
  is too thin to matter.
- The hung B-on server of the first A/B was ended with SIGTERM (it was this session's own process).
- Compat as tags and fingerprints in the existing plugin, no new plugin.

## 11. Open questions (recommended option first)

1. **Slice and Dice's slicer:** (a) leave it to the residual black box (5 in the adopt areas, 36 in
   the world); (b) a small fingerprint that models it as Create's deployer applying a cutting recipe;
   (c) a plugin. Recommend (a) until a pack MIC ships carries it.
2. **DS-10 drops stations from districts (for DS-10's owner, before it lands):** (a) a chunk holding
   a Create station joins an adjacent district but never seeds one, so a lone station on a line
   stays out; (b) Create's station is a seed (`district_seed` from the Create plugin), which makes
   every lone station a district; (c) leave it to operators (`merge`, pins). Recommend (a).
3. **Carry On range on main:** main's plugin requires Carry On [2.2.6,); TBSMP runs 2.2.2.11, and the
   plugin built with `-Pcarryon_version_range=[2.2.2,)` loaded and ran on it with no error. Recommend
   relaxing main to [2.2.2,) (one property, `neoforge/gradle.properties`).
4. **Egg Production:** recommend keeping it out of the test instance (no build exists; the loss is 47
   blocks).
5. **CoD mod's fixed port 15017:** recommend no remap; pairs are symmetric and single runs bind it.
6. **DS-10 tags for Sophisticated Storage:** recommend adding the controller, storage link, I/O,
   input, output and decoration table (and Sophisticated Storage 1.5's wooden storage connectors,
   none in this world) to `district_not_seed` with `required: false` when DS-10 lands.
