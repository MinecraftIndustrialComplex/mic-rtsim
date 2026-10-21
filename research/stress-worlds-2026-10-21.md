# Stress-test worlds survey (DECISIONS.md row ST1)

2026-10-21. Research only. No world was booted or upgraded, nothing here is committed
except this report and the audit tooling and derived numbers (see section 9). The
worlds themselves live in `~/mic-stress-worlds/<name>/` (outside every git tree) and are
for private local testing, never redistributed.

Anything not checked against a file, a jar or a live page is marked UNVERIFIED.

## 1. Bottom line

1. **sbeev's world is not obtainable.** His retired 4000-day "Create'a Colony" world is
   offered only as a paid Patreon download. Patreon and Planet Minecraft both answer
   with Cloudflare 403 to scripted fetches. Whether to buy a membership is Ben's call
   (money), see section 8.
2. **Almost every big YouTuber world is gated** behind Patreon or Discord (Stam1o, FoxyNoTail
   and the Just Create SMP seasons, disruptive builds, Dejojotheawsome, ImagineerDave,
   LifeOre, and so on). The publicly reachable set is small, and **no public world is a
   massive Create factory.** The biggest public thing is a metro city, not a factory.
3. Seven worlds were downloaded and audited (one only partially, Google Drive quota).
   Two stand out:
   - **Create StressC train-collision world**: 180 trains, 806 carriages, 181 stations,
     already Create 6.0.9 on 1.21.1 NeoForge, vanilla plus Create only. Zero migration.
   - **The Swedish Panda "StressA" metro city** (705 MB): 57 trains, 150 stations, 1,023
     signals, 5,691 graph nodes, 130.7 km of track, 243,611 block entities. 1.19.2 Forge,
     Create 0.5.1.i, 45 mods. Needs a real upgrade and loses a lot (section 5).
4. **Recommendation:** upgrade and test StressC first (no upgrade needed, it is a test
   today), Swedish Panda second. Use StressB ("This is Create", already 1.21.1, the only
   dense logistics factory in the set) as a free third, also no upgrade needed. For
   the "thousands of machines" half of ST1, no downloadable world exists, so the
   realistic route is a generated one (section 7).

## 2. Ranked table

Ranked by stress value for rtsim (trains, stations, signals, block entities) weighed
against how cheaply the world can be put on the pack's 1.21.1 / NeoForge 21.1.249 /
Create 6.0.10.

| # | World | Source | MC / loader / Create | World size | Trains / carriages | Stations / signals | Track | Create BE / all BE | Upgrade cost |
|---|-------|--------|----------------------|-----------|--------------------|--------------------|-------|--------------------|--------------|
| 1 | Create StressC train-collision test | GitHub PR attachment | 1.21.1 / NeoForge 21.1.200 / 6.0.9 | 42 MB | 180 / 806 | 181 / 0 | 15.3 km | 1,107 / 1,259 | none |
| 2 | Swedish Panda StressA metro (MCCMC) | Dropbox, YouTube description | 1.19.2 / Forge 43.4.0 (Paper hybrid) / 0.5.1.i | 705 MB | 57 / 190 | 150 / 1,023 | 130.7 km | 43,183 / 243,611 | heavy, lossy |
| 3 | StressB "This is Create" (2025) | Mediafire, YouTube description | 1.21.1 / NeoForge 21.1.133 / 6.0.4 | 59 MB | 3 / 4 | 6 / 0 | 1.5 km | 5,488 / 10,790 | none |
| 4 | Greggi "Milano Centrale Create" | Mediafire | 1.19.2 / Forge or Paper / Create 0.5.x (build not recorded) | 203 MB | 18 / 148 | 9 in graph (23 station BEs) / 0 | 24.6 km | 498 / 1,611 | DFU only (clean) |
| 5 | Warsaw Public Transport (Create issue 6679) | GitHub issue attachment | 1.20.1 / Forge 47.2.32 / 0.5.1.f, 89 mods | 168 MB | 3 / 3 | 10 / 12 | 21.0 km | 3,172 / 4,689 | heavy, all rails lost |
| 6 | Create issue 7596 train-routing test | GitHub issue attachment | 1.21.1 / NeoForge 21.1.127 / 6.0.1 | 28 MB | 1 / 1 | 4 / 0 | 0.3 km | 142 / 443 | none |
| 7 | RagePlaysGames "1000 Days" (partial) | Google Drive folder | 1.19.2 / Forge / Create 0.5.x | 19 MB of unknown total | 3 / 5 (tracks file only) | 8 / 0 | 3.3 km | not audited | DFU only |

Rank 1 beats rank 2 on train count and zero migration; rank 2 beats everything on
network size, signals and block-entity volume. Rank 3 is listed above Greggi because it
is the only world with high-logistics Create 6 content (see section 4).

## 3. Search results, and what could not be obtained

Searched: YouTube (yt-dlp search and descriptions, auto-captions), Create's GitHub
issues and PRs (attachments), CurseForge and Modrinth, Dropbox, Mediafire and Google
Drive links in video descriptions. Not reachable: Planet Minecraft (Cloudflare 403),
Patreon (403 and paywalled), reddit.com (blocked for the search tool), Discord invites
(gated by design, not bypassed), GitHub code search (needs auth).

| Creator / world | Why not downloaded |
|-----------------|--------------------|
| sbeev, "Create'a Colony" 4000-day world | Patreon only. Final-episode auto-captions say the world was about 15 FPS and "almost unplayable" (caption-derived, numbers UNVERIFIED). The modpack (Createa Colony) itself is public on CurseForge, the world is not. |
| Stam1o (Giant Train Network, 1000 Days, Just Create SMP 2) | World downloads are in his Discord, Patreon behind that |
| FoxyNoTail, Just Create SMP 1 and 2 | Modpack public on CurseForge, worlds not |
| disruptive builds (1000 Days, iron factory) | Patreon |
| Dejojotheawsome, ImagineerDave, LifeOre | Patreon or Discord |
| Simi Cats "This is Create 2022" (Drive file) | Google Drive quota refused every attempt today (a showcase map, small) |
| Skeej_Inc Modular Railways (Drive file, 1.19.2) | Same Drive quota block |
| RagePlaysGames overworld region files | Same Drive quota block, only the Nether regions, entities and data came down |
| Create issue 7633 spark profile (27 trains, about 40 km) | Only a profiler dump was attached, no world |

Lead not followed: "Infinite Rail Project" (Modrinth, 1.20.1 Fabric, a multi-YouTuber
Create railway collab). UNVERIFIED whether a world download exists.

Retry later: the Drive quota resets daily. `~/mic-stress-scripts/gd_folder3.py` (priority
order: level.dat, data, region, entities) resumes the Rage download, `gd_file.py <id> <out>`
fetches the Simi Cats and Skeej files.

## 4. Per-world audit

All numbers come from `audit_world.py` (a skipping NBT reader over the .mca files, 0
chunk read errors on every world) and are stored as JSON in the data directory (section
9). "BE" is block entity.

### 4.1 Create StressC train-collision test (rank 1)

- **Source**: https://github.com/user-attachments/files/23432365/world.zip, attached to
  https://github.com/Creators-of-Create/Create/pull/9513 ("Implements a cache for train
  collision checks for better performance", author stacode123, 2025-11-08, closed, not merged).
- **Terms**: none stated. Attached to a public PR as a performance reproduction, used here
  privately only (UNVERIFIED that the author intends reuse).
- **Version**: DataVersion 3955 (1.21.1), NeoForge 21.1.200, Create 6.0.9, Flywheel 1.0.5,
  Ponder 1.0.66. Mods: architectury, ftblibrary, ftbteams, ftbchunks, computercraft
  (CC: Tweaked), jei, commonnetworking, spark, curios, vanillin, journeymap.
- **Size**: 5.3 MB zip, 42.3 MB world, 20 regions (a 4 by 5 block of regions), 9,877 chunks, creative mode.
- **Scale**: 3 track graphs (500, 25 and 4 nodes), 529 nodes, 526 edges, 15,295 blocks of
  chord length, 181 stations, 0 signals, 180 trains with 806 carriages (all small
  bogeys, 1,080 bogeys), no schedules, 1 train moving and 2 derailed at save. 1,259 BEs:
  181 each of track station, redstone link, creative motor, deployer and sequenced
  gearshift, 154 fake tracks. 7,446 track blocks. 57 super-glue entities. No contraptions.
- **Reading**: a lattice of 181 one-station mini lines with one deployer-driven train each. It is a train
  collision and routing load, not a factory.
- **Upgrade loss**: none (blocks are vanilla and Create only). The pack runs Create 6.0.10,
  one patch above; open it as a Create-only world.

### 4.2 Swedish Panda, StressA metro city "MCCMC" (rank 2)

- **Source**: Dropbox links in the description of the YouTube video "Minecraft Create Mod
  Metro System | Metro Line M5 Showcase | WORLD DOWNLOAD" (video id qNb0pHY131E, 2025-12-31):
  `https://www.dropbox.com/scl/fi/wi81qqxm7z08yb91f1l57/MCCMC-world.zip` (621 MB) and
  `.../vxz4j2vfc4i1sz8uiluyt/MCCMC-mods.zip` (78 MB, the matching mods). Earlier video
  u-wp1XFNkgo (362k views) promises the same world.
- **Terms**: none stated, offered as a public download by the creator (The Swedish Panda).
  Private local use only.
- **Version**: level name "WorldPaperCity", DataVersion 3120 (1.19.2), server brands Paper
  plus forge (a hybrid server), Forge 43.4.0, Create 0.5.1.i, Flywheel 0.6.11.
  45 mods: create, createdeco 1.3.3, railways (Steam 'n' Rails) 1.6.6, createrailwaysnavigator
  beta 0.8.4, createindustry (TFMG) 0.9.3, create_new_age 1.1.2, interiors 0.5.6,
  extendedgears, createtrackmap, framedblocks 6.8.6, twigs, the Macaw's family (windows,
  doors, stairs, bridges, roofs, paths, fences, lights, furnitures, trapdoors, paintings),
  trafficcraft, car, automobility, simpleplanes, immersive_aircraft, immersive_paintings,
  saros_road_signs, coloredbricks, worldedit, plus client and library mods.
- **Size**: 705 MB world, 105 region files, 80,469 chunks (79,994 at 3120 and 475 at 3117),
  13,988 chunks inhabited more than 1,200 ticks, 3 playerdata files, 1.1 MB `create_tracks.dat`.
- **Scale (trains)**: 74 track graphs, the largest has 4,164 nodes, 4,365 edges, 101 stations
  and 804 signals (90.4 km), the second 906 nodes, 27 stations, 188 signals. Totals: 5,691
  nodes, 5,851 edges, 130,723 blocks of chord length, 150 stations, 1,023 signals
  (5,409 signal blocks), 57 trains, 190 carriages, 18 trains with schedules (170 entries),
  3 moving and 1 derailed at save, 2 observers. Bogeys: 249 small, 86 large platform
  double-axle, 16 double-axle, 14 invisible, 2 triple-axle (the last four types are Steam 'n' Rails).
  Edge materials: 2,499 Create andesite, 2,323 `railways:phantom`, 1,029 `railways:tieless`.
- **Scale (blocks)**: 243,611 BEs: framedblocks 154,067, create 43,183, minecraft 34,017,
  mcwfurnitures 3,581, car 2,222, createrailwaysnavigator 2,009, railways 1,707, trafficcraft
  1,477, createindustry 1,289. Create BE types: fake_track 23,893, sliding_door 5,327,
  fluid_pipe 3,215, track 3,043, copycat 2,481, track_signal 1,155, schematic_table 800,
  nixie_tube 624, fluid_tank 607, belt 469, track_station 157, display_link 190.
  The Create BE count is track and door heavy; the machine count (belts, tanks, shafts)
  is a few thousand, so this is a city, not a factory. Hot spot: region (-1,-1) alone has
  62,818 BEs, 12,781 of them Create. No contraption entities (carriages live in the train data).
  Entities: 1,634 vanilla, 320 immersive_paintings, 153 automobility cars, 118 `car:car`, 54 super glue.
- **Upgrade loss**: section 5.2.

### 4.3 StressB "This is Create" (rank 3)

- **Source**: https://www.mediafire.com/file/zs0cuou79t8qoex/This_is_Create.zip/file, linked
  from the YouTube "Behind the Scene - This Is Create" description (2025-06-05), the
  companion to the 1.78M-view "This is Create. (2025)" video. Creator StressB.
- **Terms**: none stated, public link. Private local use only.
- **Version**: DataVersion 3955, NeoForge 21.1.133, Create 6.0.4 ("World Download (Create 6.0.4
  for Minecraft 1.21.1)" per the description), Flywheel 1.0.2, Ponder 1.0.46, plus sodium, iris,
  jei, worldedit. Pure vanilla plus Create content. Creative mode.
- **Size**: 43 MB zip, 58.8 MB world, 15 regions (x -2..1, z -2..1), 7,582 chunks.
- **Scale**: 10,790 BEs (5,488 Create, all in one region), 3 trains (4 carriages), 6 stations,
  83 nodes, 1.5 km track, no signals, 7 stationary contraptions (185 blocks). Create 6
  logistics content: 468 item vaults, 66 packagers, 60 package frogports, 43 factory panels,
  36 chain conveyors, 632 belts, 491 simple kinetics, 265 fluid tanks, 57 blaze heaters, 54 steam
  engines, 47 mechanical crafters, 191 redstone links. 1,794 blast furnaces
  (a smelter hall), 127 bees, `create_logistics.dat` present.
- **Upgrade loss**: none.

### 4.4 Greggi "Milano Centrale Create" (rank 4)

- **Source**: https://www.mediafire.com/file/tnrni0bgk4fxh9f/Milano_Centrale_Create.zip/file
  (a "Milano_Centrale_Vanilla.zip" twin exists, not downloaded). Creator "Greggi" (name
  as seen in a video description, UNVERIFIED).
- **Terms**: none stated.
- **Version**: DataVersion 3120 (1.19.2), brands fabric, Paper, forge (hybrid). level.dat carries no mod
  list, so the exact Create build is unrecorded (0.5.x by era). Registry content is vanilla
  plus Create only (0 non-pack namespaces).
- **Size**: 178 MB zip, 203 MB world, 32 regions, 24,843 chunks, 2,573 chunks inhabited.
- **Scale**: 18 trains, 148 carriages (296 small bogeys), 16 graphs (largest 756 nodes,
  22.9 km), 829 nodes, 24,598 blocks of chord length, 9 graph stations (23 station BEs), no
  signals, 11,649 track blocks. 1,611 BEs total. 2,367 vanilla animals.
- **Use**: a clean single-step datafixer test: only Create and vanilla, so any loss is the
  1.19.2 to 1.21.1 hop plus Create's own migration, with no missing-mod noise.

### 4.5 Warsaw Public Transport (rank 5)

- **Source**: https://github.com/user-attachments/files/16176004/Warszawski.Transport.Publiczny.zip,
  attached to Create issue 6679 (Chisel and Bits on trains render lag, LimeShep, 2024-07-11).
- **Version**: DataVersion 3465 (1.20.1), Forge 47.2.32, Create 0.5.1.f, 89 mods (Steam 'n' Rails
  1.6.4, TFMG, Create Connected 0.8.2, Copycats+, Create Unlimited, Create Additions,
  Big Cannons, Chisels and Bits, Framed Blocks, Quark, BOP and more).
- **Size**: 20.7 MB zip, 168 MB world, 61 regions, 40,275 chunks.
- **Scale**: 2 graphs (571 nodes, 625 edges, 10 stations, 12 signals, 21.0 km), 3 trains and
  3 carriages, 4,689 BEs. Every one of the 625 edges is `railways:phantom` track.
- **Upgrade loss**: all rails are Steam 'n' Rails phantom track (5,880 blocks, `railways`
  namespace 5,948). Not recommended, the whole railway turns into ghost graph.

### 4.6 Create issue 7596 train-routing test (rank 6)

- **Source**: https://github.com/user-attachments/files/19045532/New.World.zip, attached to
  Create issue 7596 ("Train routing fails despite route being available", SabrCat, 2025-03-03).
- **Version**: 1.21.1, NeoForge 21.1.127, Create 6.0.1, Ponder 1.0.39, Flywheel 1.0.1.
- **Scale**: 28 MB, 4,053 chunks, 1 train, 4 stations, 15 nodes, 443 BEs including a small
  high-logistics line (4 packagers, 4 postboxes, a stock ticker, 13 vaults). Too small to stress anything.
- **Upgrade loss**: none. Kept only as a Create 6.0.1 save to compare against 6.0.10.

### 4.7 RagePlaysGames "I Survived 1000 Days with the Create Mod in Hardcore" (rank 7, partial)

- **Source**: Drive folder
  https://drive.google.com/drive/folders/1biaIAx2x0si5IUo1geJe-Ie0-w500Skn in the description of
  the video "I Survived 1000 Days with the Create Mod in Hardcore Minecraft [FULL MOVIE]"
  (IPJwHIcmbb4, 1.66M views, 2024-06-06). Creator RagePlaysGames.
- **Version**: Create on 1.19.2 (video description), mods Create, JEI, Xaero's Minimap, Create
  Goggles, Create Enchantment Industry. `create_tracks.dat` DataVersion 3120.
- **Obtained**: only the Nether region files (15 MB), entities, data and the player advancements.
  level.dat and the overworld regions failed on Drive quota, so the world cannot be opened or
  fully audited. From `create_tracks.dat`: 3 trains, 5 carriages, 4 graphs, 172 nodes, 8
  stations, 3.3 km of Create andesite track, 1 observer.
- **Verdict**: a survival world with a tiny rail network. Not worth a retry for stress purposes.

## 5. What an upgrade to 1.21.1 / NeoForge / the MIC pack loses

### 5.1 What was checked in the jar, not guessed

Create 6.0.10 (`create-1.21.1-6.0.10.jar`, the pack's own build, disassembled with javap):

- **Track graph file**: `RailwaySavedData` loads the keys `Trains`, `RailGraphs` and `SignalBlocks`
  from `create_tracks`, with no version gate. The 0.5 layout is read by the same code.
- **Node locations**: `TrackNodeLocation.read` accepts both the Create 6 form (`Pos` int array)
  and the old form (`X`, `Y`, `Z` ints, which is what every 0.5.x world here uses).
  This was the main open worry from the audit and it is settled: old graphs load.
- **Unknown track material** (for example `railways:phantom`): `TrackMaterial.deserialize` logs an
  error and falls back to Create andesite. No crash.
- **Unknown bogey style** (every `railways:*` bogey): `CarriageBogey.getStyle` falls back to
  `AllBogeyStyles.STANDARD`. No crash.

### 5.2 Per world

| World | Survives | Lost or degraded |
|-------|----------|------------------|
| StressC, StressB, 7596 | everything | nothing (already on the pack's version family) |
| Greggi | all blocks and BEs (Create plus vanilla) | only what the DFU hop and Create's 0.5 to 6 migration break (5.3) |
| Swedish Panda | trains, graph, stations, signals (57 trains, 150 stations, 1,023 signals), Create track and doors, createdeco, vanilla | 166,369 of 243,611 BEs (68%): framedblocks 154,067, mcwfurnitures 3,581, car 2,222, railways navigator 2,009, railways 1,707, trafficcraft 1,477, TFMG 1,289, new age 12, automobility 5. 10.74M blocks in lost namespaces become air (twigs 8.17M, of which 7.7M are `twigs:rhyolite` natural stone, TFMG 1.68M natural ore and oil, create_new_age 0.53M ore, framedblocks 154k, trafficcraft 47k, car 38k, Macaw's about 88k, railways 21.9k). 153 automobility cars, 118 car entities and 320 paintings. 3,352 of 5,851 rail edges (57%) are Steam 'n' Rails track and fall back to andesite while their `railways:` blocks vanish (a ghost graph). 118 Steam 'n' Rails bogeys become standard bogeys. |
| Warsaw | little | all 625 edges are phantom track, 77,121 blocks and 1,418 BEs in lost namespaces. Not worth upgrading. |

Pack gaps (from `pack/mods/*.pw.toml`): the pack has Create, Create Deco, Create: Connected,
Copycats+, Create: Interiors, Sophisticated Backpacks, CC: Tweaked and Architectury. It does not have
Steam 'n' Rails, Railways Navigator, Framed Blocks, Twigs, any Macaw's mod, TFMG, Create New Age, Create
Track Map, TrafficCraft, Car, Automobility, Immersive Paintings or Aircraft, Simple Planes, Chisels and Bits.
(The pack ships Create Tracks, which is a different mod from Steam 'n' Rails.)

Missing blocks load as air in vanilla (standard behaviour, not tested here). The bigger risk
is the survival of the world as a stress input: Swedish Panda keeps its train traffic but loses
two thirds of its block entities.

**Option B for Swedish Panda.** Build a test-only instance (not the pack) that adds 1.21.1
NeoForge ports of the missing mods. Modrinth lists 1.21.1 NeoForge builds today for Framed
Blocks 10.6.3, Twigs 3.1.2, Macaw's windows/doors/furniture/roofs/bridges/stairs/trapdoors/lights/paths/fences,
Railways Navigator 1.21.1 beta 0.10.0 (Create 6), Create New Age 1.2.0, TrafficCraft 1.2.0, Automobility 0.5.0,
Immersive Paintings and Aircraft, Simple Planes, "Steam 'n' Rails Neoforge" (a separate project slug,
create-steam-n-rails-1.21.1) and TFMG (`create-tfmg` 1.2.0, plus a Community Edition). Namespaces and block
ids across such ports are UNVERIFIED. One is known to differ: TFMG is `createindustry` in the
Swedish Panda world (1.19.2) but `tfmg` in the Warsaw world (1.20.1), so its blocks need a remap table.
This keeps the world rich but stops measuring the pack; it is a fallback if option A (pack only) is too empty.

### 5.3 The two version hops and Create's own migration (mostly UNVERIFIED)

- **Vanilla DataFixer**: DataVersion 3120 to 3955 is inside DFU's supported range. Run it as a
  full world upgrade (`--forceUpgrade` on a 1.21.1 server, or the Optimize World screen) before
  anything else touches the chunks. Not run here, by instruction. UNVERIFIED that it completes cleanly on 80k chunks.
- **Item format**: since 1.20.5, items use data components. DFU converts items it knows about
  (vanilla containers, vanilla BE ids). Modded BEs it does not know (belts, depots, vaults,
  funnels, deployers) keep their old `Items` NBT, which Create 6 may fail to parse. Expect
  empty belts and vaults and warnings. UNVERIFIED, test on Greggi first.
- **Create track BEs**: the track block entity's saved bezier data and signal and station BEs
  were not inspected against Create 6's readers. Graph loading is verified (5.1), BE
  loading is UNVERIFIED.
- **Create 6 backports**: Create 6.0.x also exists for Forge 1.20.1 (up to 6.0.8 by CurseForge and
  Modrinth listings, and a 6.0.6 jar sits in the DaForge Prism instance). A two-stage route,
  0.5.1 to 6.0.x on 1.20.1, then 1.20.1 to 1.21.1, is possible. Nothing found says it beats the
  direct route. UNVERIFIED either way. Create's 1.21.1 changelog (mc1.21.1/dev) has no world-migration note.
- **Known performance bug in 0.5.1j**: issue 7633 (a 27-train, 40 km server, 150 to 2,000 ms save
  spikes) traced part of it to `create_tracks.dat` accumulating millions of empty compound tags
  (a fix was submitted 2025-01-05 and is not in 0.5.1j). Swedish Panda's file is 1.1 MB so it is
  not affected, but any 0.5.1j world should be checked before blaming Create 6.

## 6. Recommendation: first two worlds to upgrade and test

1. **StressC train-collision world.** Zero migration, it is a Create 6 world today on the
   pack's version family. 180 trains and 181 stations is the heaviest train load found. Run it as
   a plain "does rtsim's train bridge hold at 180 trains" test. Caveats: no signals, no schedules
   (the trains are driven by sequenced gearshifts), and almost no factory content.
2. **Swedish Panda StressA.** The largest real network (5.7k nodes, 150 stations, 1,023 signals, 57
   trains with schedules) and the most block entities. Start with option A (pack only, accept the
   68% BE loss, the train network and signals survive intact) because it is exactly the pack's
   world. Keep option B in reserve. Before booting, do the graph clean-up: rewrite the 3,352
   `railways:*` edges to andesite and either remap `railways:track_*` blocks to `create:track` or
   delete the ghost edges. Whether `railways:` track blocks can be remapped one to one is UNVERIFIED.

Free third: **StressB**, which needs nothing, and is the only world with a real Create 6 logistics
stack (packagers, frogports, factory panels, 468 vaults, a blast-furnace hall). Fourth, for a
clean datafixer test with no missing mods: **Greggi Milano Centrale**.

Tell the later agent before it starts: these are not factory worlds. The factory-scale half of
ST1 (thousands of Create machines) is unmet by anything public. See next section.

## 7. Making up the shortfall: generated stress

- **Railways Untold** (Modrinth `railways-untold`, forge, fabric and neoforge for 1.20.1 and 1.21.1,
  33k downloads): "Procedurally generated Create train track around your world." A generator of
  large track graphs with no download needed. Station, signal and train counts it produces are UNVERIFIED.
- **Create: Threaded Trains** (Modrinth `create-threaded-trains`, 1.19.2 to 1.21.1 NeoForge, 955k
  downloads): runs the railway network calculation off the server thread. Relevant to rtsim's
  Create bridge: if the pack ever ships it, train state is read across threads. Worth a note in the
  bridge design, UNVERIFIED how it interacts with the current hand-over.
- **A synthetic world generator.** The `create_tracks.dat` layout (Trains, RailGraphs with nodes,
  edges, points, SignalBlocks, DimensionPalette) is now documented by `audit_world.py`, so a script can
  emit N stations, M trains and K signals on a flat world, and the mic-rtsim GameTests already
  create synthetic trains and real stations. That reaches any scale and is the only route to the
  "thousands of machines" half of ST1. UNVERIFIED effort, a few hundred lines.
- **Create Optimized Trains** (Modrinth, 1.20.4 to 1.20.6 only): carries 6.0.9 train fixes back,
  irrelevant to 1.21.1.

## 8. Items for Ben

- **Money, yours to decide:** a sbeev Patreon tier would unlock the retired lag world. Not purchased.
  I recommend not buying until the StressC and Swedish Panda runs show the rtsim bridge needs a
  larger real world (it may not, since a generated network scales further).
- **Discord-gated worlds** (Stam1o and others) could be asked for politely, but that is outward
  facing, so I did not. Nothing was joined or requested.

Choices made (all undoable): ranked on stress value weighed against upgrade cost, not on raw size;
audited and kept Warsaw, 7596 and Rage even though small, as cheap comparison points;
committed the audit scripts and derived JSON next to the report so the upgrade agent does not
rebuild them; stopped the Rage Drive retries instead of waiting out the quota.

## 9. Method, tools and files

- Worlds: `~/mic-stress-worlds/<name>/{_archive,world}` for each of stress-world-a,
  stress-world-c, stress-world-b, greggi-milano-centrale,
  create-issue6679-warszawski-transport, create-issue7596-train-routing-newworld, and
  rageplays-1000days-1.19.2 (partial). Each archive is unpacked into an empty directory. About 2.3 GB in total.
  None of it is in git.
- Archive sha256 prefixes: MCCMC-world 23dc960754b5c51a, MCCMC-mods 945839aa234cc93e, StressB
  b83c0f9f9d954b54, StressC c212e123f130f81b, Greggi 1f0e9a4998b74338, Warsaw c0207d0134795105,
  issue 7596 7838a70eda7e2233.
- Tooling (run with `python -I`, scripts kept apart from the data):
  `research/stress-worlds-2026-10-21-data/audit_world.py <world_or_parent> <out.json> [workers]`
  (needs numpy and nbtlib), `summarize.py <out.json>`, and the audit JSON per world under `audits/`.
  A 705 MB, 80k-chunk world audits in about 44 s with 8 workers under nice 10.
- Create 6.0.10 findings (5.1) came from `/home/greencheetah/mic-livesrv/srv-current-base/mods/create-1.21.1-6.0.10.jar`,
  the same build as `pack/mods/create.pw.toml`.
