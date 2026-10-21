# Stress worlds: results (DECISIONS.md row ST1), 2026-10-21

mic-rtsim was booted on four real Create worlds, run with and without rtsim side by side, profiled,
saved, reloaded and uninstalled. This page gives the numbers, what broke, what was fixed and what is
left. The worlds are described in `research/stress-worlds-2026-10-21.md`. The downloads and every copy
stay in `~/mic-stress-worlds/` (outside git) and are never redistributed.

## 1. Bottom line

1. **rtsim costs little server time on these worlds.** Spark puts rtsim's own work at 0.2% to 0.5% of
   the server thread (about 0.1 to 0.25 ms per tick) on StressC, StressB and StressA (0.9% on StressA
   before the fingerprint cache). Paired MSPT differences (on minus off) are within the noise between
   two identical servers. See section 3 for the table. Its one visible cost is a 90 to 160 ms
   detection tick every 5 minutes on StressA's 39 districts (section 7).
2. **Four rtsim bugs and one hot spot were found on real worlds and fixed, each with a test first**
   (section 5):
   - an upward fan chute failed the glass box compile of a whole district;
   - Lithium's sleeping block entities crashed the tick-order read;
   - Create's deployers counted as players editing the district, so a factory with deployers never
     finished a glass box compile, rescanned its districts about 7 times a second and ran a threat
     survey every 10 seconds;
   - `/rtsim district perf` said nothing about trains (a plugin line now reports them);
   - the glass box's fingerprint passes recomputed fixed answers (now cached; a third less time).
3. **The big costs on these worlds are Create's, not rtsim's:** train collision checks (Create's
   pairwise `findCollidingTrain`, about 20% of the server thread on StressC's 180 trains) and G1
   garbage collection under Create's allocation.
4. **Station learning works on the one world with scheduled trains,** once a Create upgrade bug is
   undone: Create 6 sets every Create 0.5 station, signal and curve position to 0,0,0 on its first
   load, so on the first StressA run rtsim saw no arrival at all. With the positions converted before
   the upgrade (`remap_tracks.py --positions-only`), 7 metro trains made 67 arrivals and 65
   departures at district stations in 26 minutes, 13 of them simulated while the scene had their
   districts unloaded (section 4.4). StressC's 180 trains and Greggi's 18 have no schedules, and
   StressB's 3 trains sit idle.
5. **The glass box reaches verdicts on real builds:** pass, 0 strikes, on StressB's 2,779-node factory
   after a 20-minute window, and on the 6 StressA districts that hold inventories.
6. **Uninstalling leaves the world clean except for one thing that matters:** the gamerule
   `doTraderSpawning` stays false (section 8, item 1).

## 2. Method

- **Server:** the MIC pack as deployed (NeoForge 21.1.249, 168 mods, Create 6.0.10), copied from the
  te-perf server (pack commit dc49fe7). Java 21, G1, `-Xms2G -Xmx6G` (10G for StressA), view and
  simulation distance 8, `max-tick-time -1`, run under `nice -n 10` on the shared laptop
  (Ryzen, 24 threads, 46 GiB; other agents' builds and GameTests ran at the same time).
- **rtsim:** mic-rtsim and every plugin built from this branch (base cc55edd on main, rebuilt after
  each fix; 7177451 for myl2, al3 and the last StressB boots), with `mic_native` (debug-info build).
  "Off" removes the rtsim and mic_native jars; everything else is the same.
- **Pairs:** two servers on copies of one world, started 5 s apart, run through the same scene at the
  same time on their own ports. Pairs alternate which arm starts first. A scene (`~/mic-stress-worlds/scenes/`) is:
  1. idle (spawn chunks only);
  2. forceload the build's chunks, settle, `rtsim district adopt` at the build's centre (rtsim arm);
  3. busy window;
  4. optional spark profile (`--save-to-file`, never uploaded);
  5. forceload removed, wait, an "unloaded" window, forceload again, wait, a "reloaded" window;
  6. heap and metaspace (`jcmd`), `save-all flush` timing, world and rtsim sizes.
- **Measures per window:** MSPT (the server's own tick times, sampled every 5 s), whole-process CPU per
  tick, server-thread CPU per tick, TPS, and rtsim's `/rtsim district perf`, `list` and `info` output.
- **Tools** (committed in `tools/stress-worlds/`): `sw.py` (the harness), `summarize.py`, `gcrate.py`
  (allocation rate from the GC log), `sparkprof.py` (spark profile reader), `hotspots.py` (where the
  Create blocks are), `remap_tracks.py` (clean-up of 1.19.2 Create worlds:
  Steam 'n' Rails, missing mods, foreign schedule entries, Create 0.5 positions).
- **Noise:** two identical servers differ by up to 2 to 3 ms MSPT on the train worlds, because Create's
  train simulation is bursty (MSPT standard deviation 5 to 45 ms within a window) and the laptop's load
  moved between 7 and 80 during the night. Read single pairs with that in mind; the spark share is
  the firmer number.

## 3. Numbers, off against on

MSPT, whole-process CPU per tick and server-thread CPU per tick, in ms. "d" is on minus off within
one pair; the range is over pairs.

| World | Window | Off MSPT | On MSPT | dMSPT (range) | dCPU/tick (range) | dServer thread/tick (range) | Pairs |
|---|---|---:|---:|---:|---:|---:|---:|
| StressC | idle | 26 to 49 | 26 to 51 | +1.6 (-0.8 to +2.9) | +12.6 (+5.4 to +16.2) | +2.7 (+2.0 to +3.8) | 3 |
| StressC | busy | 28 to 32 | 28 to 34 | +1.2 (+0.6 to +2.0) | +10.7 (-0.6 to +16.8) | +1.0 (+0.7 to +1.2) | 3 |
| StressB | idle | 3.9 to 8.6 | 4.0 to 7.9 | +0.1 (-0.7 to +0.8) | +0.5 (-1.4 to +1.5) | +0.2 (-0.3 to +0.4) | 3 |
| StressB | busy | 17.6 to 21.8 | 18.2 to 21.3 | +0.1 (-0.4 to +0.6) | +2.8 (+1.0 to +4.3) | +0.6 (+0.4 to +1.0) | 3 |
| Greggi | idle | 3.68 | 3.97 | +0.29 | +1.2 | +0.36 | 1 |
| Greggi | busy | 6.26 | 6.29 | +0.04 | +1.0 | +0.13 | 1 |
| StressA | idle | 10.0 to 14.8 | 7.9 to 30.3 | +6.7 (-2.2 to +15.5) | -6.0 (-28.1 to +16.1) | +1.2 (-2.4 to +4.8) | 2 |
| StressA | busy | 29.9 to 66.4 | 30.6 to 61.9 | -1.9 (-4.5 to +0.6) | +1.0 (+0.2 to +1.8) | -0.1 (-0.2 to 0.0) | 2 |

The StressB pairs are my1 and my2 (before the deployer fix) and my4 (after). my3 is left out: the
laptop's load rose to 80 during it and its idle window read 95 against 165 MSPT. The StressA pairs
are al2 (stations at 0,0,0, so no trains in rtsim's view) and al3 (positions fixed).

**Spark (busy, server thread):**

| World | rtsim share | Per tick | Biggest rtsim parts | Biggest costs overall |
|---|---:|---:|---|---|
| StressC | 0.215% | 0.11 ms | sampler 48 ms, glass 36 ms, engine hand-over 24 ms, habitat 8 ms per 60 s | Create trains 32 to 42%, of which `findCollidingTrain` 20.7% self time |
| StressB (my1, before the fixes) | 0.44% | 0.22 ms | detection rescans, glass restarts | Create kinetics and logistics |
| StressB (my4, after) | 0.457% | 0.23 ms | glass box 108 ms, of which fingerprint passes 96 ms; sampler 32 ms; districts (detection) 32 ms; threats 8 ms per 60 s | Create kinetics and logistics |
| StressB (myl2, fingerprint cache) | 0.432% | 0.22 ms | glass box 68 ms, of which fingerprint passes 64 ms; sampler 48 ms; districts (detection) 16 ms; threats 12 ms per 60 s | Create kinetics and logistics |
| StressA (al2, e330ce3) | 0.885% | 0.38 ms | districts 128 ms (a rescan 84 ms), glass box 104 ms (fingerprint passes 68 ms), engine hand-over 88 ms, sampler 24 ms per 60 s | Create trains 33.7%, `findCollidingTrain` 3.6% |
| StressA (al3, 7177451) | 0.523% | 0.25 ms | districts 88 ms (a rescan 56 ms), glass box 68 ms (fingerprint passes 44 ms), engine hand-over 40 ms, threats 36 ms, sampler 16 ms per 60 s | Create trains 14.7%, `findCollidingTrain` 2.1% |

**Memory, save and boot (rtsim arm against off arm):**

| World | Live heap | Metaspace | RSS | rtsim dir | Save flush | Boot (wall, "Done") |
|---|---|---|---|---|---|---|
| StressC | 1.63 to 1.70 GB, same as off | +4 to +5 MB | +70 MB | 15 to 29 kB | 0.46 to 0.67 s, same as off | 64 to 160 s wall, 4.7 to 10 s Done, same as off |
| StressB | 1.77 GB, same as off | +4 MB | +120 MB | 47 kB | 0.58 s (2.0 s in a loaded pair), same as off | 72 to 88 s wall |
| Greggi | 1.80 GB, same as off | +4 MB | +18 MB | 7 kB | 0.56 s | 66 s wall |
| StressA | 1.98 GB, same as off | +4 MB | -6 to +420 MB (committed heap differs by arm) | 50 to 54 kB | 0.79 to 1.14 s (off 0.94 to 1.50 s) | 90 to 247 s wall, 11 to 19 s Done, same as off |

### 3.1 StressC's CPU difference is garbage collection, not rtsim

The first two StressC pairs showed the rtsim arm using 16 ms more CPU per tick, idle and busy, with
zero districts in the idle window. The server thread differed by only 1 to 4 ms. JFR and the GC logs
explain it:

- both arms allocate about 0.3 to 0.5 GB/s, 57% of it `Vec3` from Create's train collision code;
  rtsim's own allocation is 0.2% of the total;
- the live heap is about 1.7 GB of 6 GB, near G1's marking threshold, so one arm often sits in
  back-to-back concurrent marking (old GCs every 4 s) while the other does not. The CPU difference is
  G1's concurrent threads, plus the write-barrier work marking puts on the server thread;
- the third pair flipped it: busy CPU per tick -0.6 ms (rtsim arm lower).

So the CPU-per-tick column on StressC is which arm landed in marking, not rtsim. A larger heap or
`-XX:InitiatingHeapOccupancyPercent` tuning would remove it for both (section 8).

## 4. Per world

### 4.1 Create StressC train-collision world

1.21.1, Create 6.0.9 saved, 180 trains, 806 carriages, 181 stations on a superflat. No upgrade needed.

- **Booted** clean on the pack, off and on. 3 track networks, 529 graph nodes, 180 trains, every one
  "Idle, No Schedule", so no train ever arrives or departs.
- **Districts:** `adopt` at the station field gave 724 seeds and 10 districts, all named after a
  station ("Track Station"), 8 chunks and 20 stations each, no boundary inventories.
  First detection 27 ms in one tick, later ones about 10 ms.
- **Glass box:** 0 nodes (no machines), compile 1.6 ms.
- **Trains:** nothing to learn (no schedules). `TrainWatch` costs nothing measurable.
- **Reload boot** (from the rtsim arm's save, rtsim on): 10 districts restored, boot 112 s wall
  (7.5 s Done), idle 43.7 and busy 30.3 MSPT, no errors from rtsim. No reconcile (no inventories).
- **Uninstall boot** (jars removed): boots in 52 s, idle 37.9 and busy 26.4 MSPT. Residue in section 6.
- **Bottleneck:** Create's train collision check, pairwise over trains, about 20% of the server thread
  in self time (`Train.findCollidingTrain` and `intersectSphere`). Not rtsim's to fix.

### 4.2 StressB "This is Create"

1.21.1, Create 6.0.4 saved, 10,790 block entities, the only dense logistics factory in the set
(packagers, frogports, vaults, mechanical arms, deployers, gantries).

- **Booted** clean, off and on.
- **Districts:** `adopt` gave 5,744 seeds and 7 districts with 180 boundary inventories. District 1:
  39 chunks, 174 inventories, 5 stations; the defence survey counts 5,556 machines and 9,242 dark
  spawnable blocks.
- **Sampler:** 22,680 inventory reads in a busy window, 1.53 million slots, 252 ms in all; the slowest
  single read 15 ms (a 1,620-slot vault group).
- **Glass box, before the fixes:** the compile of district 1 first failed outright (upward chute,
  fixed in 1672a3a), then never finished: 113 restarts in a few minutes, each one a deployer placing
  or breaking a block (fixed in 59c68f4).
- **Glass box, after the fixes (my4):** district 1 compiles to 2,781 nodes, 4,381 edges, 91 to 97
  recipes and 30 domains. The solve converges in 2 rounds; the last solve took 64 ms on the engine's
  worker, off the server thread. 11 compiles and 14 restarts in the first 2.5 minutes after `adopt`,
  every restart a fingerprint change in one cluster of chunks (-18..-14, -15..-13: gantries, an
  elevator, a mechanical bearing and sequenced gearshifts), then it held for the rest of the run.
  - **What it could not model:** 78 block entities the compiler has no model for (the residual,
    G-B13) form one black box node, and 33 of the 40 goods in the checksum pass through it, so their
    predictions are unbounded and never judged. The boundary inventories barely move (nobody feeds
    the factory), so most measured rates are 0. `district info` gives only the residual's size;
    the compile log now names its blocks (7177451, below).
  - **No verdict in a 10-minute window:** every recompile resets the 6,000-tick checksum window, so
    at the end of the busy window it stood at 3,500 of 6,000 ticks; the unload then discarded it
    ("not live"), and it restarted from 0 on reload. A verdict needs 5 quiet minutes after the last
    recompile with the district loaded throughout.
- **Detection:** before the deployer fix, 1,051 rescans in 15 minutes (mean 12.3 ms, max 35 ms), all
  caused by deployers. After it: 3 detections in 12 minutes (the first at `adopt`, 74 ms in one tick;
  the others 18 and 42 ms), 134 ms in all.
- **A 20-minute busy window (myl2, jars at 7177451):** district 1 recompiled 9 times in the first 3
  minutes after `adopt` (21 restarts, all fingerprint changes in the same contraption chunks), then
  held. The checksum then ran 3 full windows of 6,000 ticks: **verdict pass, 0 strikes**. The goods
  through the black box stay unjudged. The residual is now named in the compile log: 81 block
  entities, `minecraft:crafter` 33, `create:deployer` 28, `create:portable_storage_interface` 5,
  `create:track_station` 5, decorated pot, dispenser and dropper 3 each, `create:stock_ticker` 1.
- **Reload and reconcile:** after 300 s unloaded (19 of district 1's 39 chunks stayed loaded through
  the spawn area, so it was "partly loaded") and a reload, all 7 districts came back with their
  inventories; district 1 recompiled once on reload (compile 10, same graph less one node) and its
  checksum window started again. **No reconcile was applied, correctly:** nothing crosses the
  boundary of an idle factory, so the engine had no change to hand back. No world in this set
  exercises reconcile; the GameTests do.
- **Bottleneck inside rtsim:** after the fixes, the glass box's fingerprint passes: 96 of rtsim's
  184 ms per busy minute in `Industry.endOfTick` (my4). Both of its hot calls had fixed answers, so
  7177451 caches them (SW26): in myl2 the passes took 64 ms (`hash` 84 to 44 ms, `CreateGlass.fingerprint`
  56 to 16 ms), on a machine twice as loaded and with the server at nice 19 (SW31), so the real
  saving is likely larger. rtsim in all: 0.432% of the server thread in myl2.

### 4.3 Greggi "Milano Centrale"

1.19.2 Forge, Create 0.5.x, 18 trains, used as the clean DataFixer test.

- **Upgrade:** `--forceUpgrade` on the pack ran clean: no DataFixer errors, the track graph and all 18
  trains loaded, chests kept their contents (the station area has no Create item blocks).
- **Districts:** one district at the main station, no boundary inventories, glass box empty.
- **Trains:** no schedules, so nothing moves.
- **Numbers:** idle 3.68 against 3.97 MSPT, busy 6.26 against 6.29. One pair only: the world has
  nothing for rtsim to model, so more pairs would add nothing.

### 4.4 Swedish Panda "StressA" metro

1.19.2 Forge, Create 0.5.1.i, 57 trains, 150 stations, 1,023 signals, 130.7 km of track, 243,611
block entities.

**Clean-up and upgrade** (on a copy; `tools/stress-worlds/remap_tracks.py`):

- Steam 'n' Rails track: 1,881 palette entries (`railways:track_phantom` 1,229, `track_tieless` 652)
  to `create:track`; 2,155 track block entity materials and 6,704 graph edge ends to andesite.
- Steam 'n' Rails bogeys: 118 to Create's (Create 6.0.10 throws a ClassCastException on an unknown
  bogey; it does not fall back as the survey thought).
- Missing mods dropped before the upgrade (Framed Blocks, Macaw's furniture, cars, Create Railways
  Navigator, and five more): 142,726 palette entries to air and 166,369 block entities removed, in
  20,897 chunks. The server would do the same, with one error line per block entity.
- Create Railways Navigator's schedule entries: Create 6.0.10 reads a foreign schedule instruction or
  condition as null and then throws on every save, so the server cannot stop. 14 `travel_section`
  entries dropped; 68 `dynamic_delay` and 4 `train_separation` conditions turned into Create delays.
- `--forceUpgrade`: 80,469 chunks in 118 s, live heap 1.76 GB.

**Create 6 zeroes Create 0.5 positions (found here; the clean-up now converts them).** Create 6 reads
a station's or observer's position as `BlockEntityPos` (an int array), and signal tiles and curve
ends as `{Pos: [x, y, z]}`. A Create 0.5 world has `TilePos {X, Y, Z}`, `{X, Y, Z, Power}` and
`{X, Y, Z}`. Create's own upgrade reads none of them: on the first load every one becomes 0,0,0,
silently, and is saved that way (a station block keeps working, because it finds its edge point by
id, not by position). On StressA that was 172 edge points, 1,125 signal tiles, 6,124 curve ends in
the track graph and 6,080 in curved track block entities. `remap_tracks.py --positions-only`
(e0ff91a) converts them; it has to run before the first Create 6 load. After the fixed upgrade no
position in the track graph is zero and no old form is left (contraption positions keep `{X, Y, Z}`
under `Pos`, which Create still reads). What else in Create the zeros break (signal rendering,
curve ends finding their block entities) was not checked.

**Runs:**

- **al1:** both arms crashed 30 s after boot: the first clean-up deleted schedule condition progress,
  which Create 6 indexes per condition (fixed in 842f0ec, SW24).
- **al2** (jars e330ce3): five `adopt`s (54, 737, 456, 444 and 35 seeds) made 39 districts with 58
  boundary inventories, most of them small districts around stations (the biggest 19 chunks).
  Trains moved and advanced their schedules, but `TrainWatch` counted 0 arrivals: every station sat
  at 0,0,0 (above).
- **al3** (jars 7177451, StressA upgraded again from the fixed clean-up, SW30): the same 39 districts,
  a 15-minute busy window.
  - **Trains:** 7 scheduled trains called at district stations: 44 arrivals and 43 departures in the
    busy window, 67 and 65 by the end (26 minutes after `adopt`), 13 of the arrivals simulated while
    the scene had their districts unloaded. No cargo ports: a passenger metro carries nothing. Only
    22 of the 63 schedule destinations lie in the scene's tiles; the rest of the network runs
    outside any district, as it should. Each departure followed by an arrival between district
    stations becomes a line edge in the engine (trips, travel time, cargo); no command shows them
    (section 7).
  - **Glass box:** every district compiled once, with no restart and no residual. The largest graphs
    are 246 nodes (Svetli Gaj 1, 14 inventories), 196 (district 20) and 24 nodes with 6 edges
    (district 9, a coal unloader): station storage, not machines. Verdict pass, 0 strikes, in 6 of
    the 7 districts with inventories. District 9 straddles the scene's tile edge (19 chunks loaded, 16
    ticking), so it was never live and never judged, as designed.
  - **Detection:** 10 detections, 1.25 s in all at load 60: the five at `adopt` 0.88 s, then 28 to
    160 ms in one tick for each later one, the periodic full pass every 6,000 ticks among them (al2:
    139 ms when the scene loaded its five tiles again). The largest single rtsim tick on any world
    (section 7).
  - **Sampler:** 14,427 inventory reads, 396,000 slots, 0.35 s in all.
  - **Reconcile:** none applied; the station inventories did not change while unloaded.
  - **Errors:** 2,420 ERROR lines on each arm (recipes, c2me chunk sections, invalid items), none
    from rtsim; no mixin failures.
- **Numbers** (two pairs, section 3): busy dMSPT -1.9 (-4.5 to +0.6), server thread -0.1 ms per
  tick. The idle window comes before `adopt` (no districts); al3's idle ran just after boot at load 64
  with an MSPT standard deviation of 36 ms, so its +15.5 is noise.

**Item data lost by the 1.19.2 to 1.21.1 upgrade:** item stacks inside modded block entities keep the
old `{id, Count, tag}` form, which DataFixer does not touch and 1.21.1 reads as count 1 with no
components. Before the first 1.21.1 load the world held 2,481 Create copycats, 80 toolbox stacks (16
with tags), 104 redstone links, 12 deployer stacks (390 items), 11 chute stacks (61 items) and 5 basin
stacks in that form. After the StressA runs (every forceloaded chunk loaded and saved under 1.21.1)
the rest is still in the old form in chunks that never loaded (2,391 copycat stacks, 12 deployer
stacks of 390 items, 11 chute stacks of 61 items), and the stacks in loaded chunks were rewritten as
predicted, count 1 and no components (90 copycats, 23 mechanical rollers, 3 creative crates, 1 belt
stack). Copycats lose nothing (one block, no tag); a deployer, chute or basin holding more than one
item keeps one when its chunk first loads.

## 5. What broke, and the fixes

All on branch `test/stress-worlds`, each with a failing test first.

| Commit | What broke | Where it showed | Fix | Test |
|---|---|---|---|---|
| 1672a3a | `IllegalArgumentException: downward motion must be negative: 0.075`: a fan pushing items up a chute failed the whole district's glass compile, every time | StressB district 1 | `Cycles.chuteTicksPerBlock` models upward chutes (Create's own rule: start at 0, hand off above 1); a bottom chute takes the grab period, a middle one the transit time. Flagged CALIBRATED: not yet checked in game | `CyclesChuteTest` (new JUnit set in the create plugin) |
| 1ed1774 | `NullPointerException` reading the level's tick order: Lithium parks sleeping block entities behind a shared ticker whose position is null | StressB, with Lithium (the GameTest servers have no Lithium) | `TickOrder.places` skips a ticker with no position | `TickOrderTest` |
| 59c68f4 | Create's deployer is a NeoForge `FakePlayer`, and rtsim counted its block placements and breaks as a player editing the district: a journal edit, a glass compile restart, a district rescan and a threat survey each time | StressB: glass never finished, 1,051 rescans, surveys every 200 ticks | `IndustryEvents` and `ThreatEvents` treat `Player.isFakePlayer()` as a machine (no seed, no edit, no survey); a machine breaking boundary stock is still a port. INDUSTRY.md §3.3 says so. GameTests that stood in for a player with a FakePlayer now use a real server player (`FakePlayers.editor`) | `ThreatGameTests.machineEditsAreNoPlayerEdits` |
| e330ce3 | `/rtsim district perf` had no train figures | all train worlds | an optional `IndustryPlugin.perfLine()`; the Create plugin reports arrivals, simulated arrivals, departures, cargo ports, trains seen and at a station | covered by the existing train GameTests |
| 7177451 | Hot spot, not a bug: the glass fingerprint pass hashed every block state's properties and asked the registry for each block entity type's namespace, on every pass, though both answers are fixed | StressB my4 spark: 96 of rtsim's 184 ms per busy minute | cached per block state (`CreateGlass`) and per block (`GlassCompiler`); same answers. The compile log also names the residual's commonest blocks | `GlassExtractionGameTests.fingerprintsCountStructureNotModelState` |

Tool fixes (the clean-up script, not rtsim): 842f0ec keeps schedule condition progress (deleting it
crashed Create 6), e0ff91a converts the Create 0.5 positions Create 6 zeroes (section 4.4). Both
checked by dry runs, an idempotence rerun and a Create 6 boot of the result.

GameTests after the fixes: core 58 of 58 and create 146 of 146 passed (at 7177451, on fresh GameTest
worlds; on worlds reused from earlier runs six create window tests and two herd tests fail, SW28).

## 6. Uninstall

Removing the rtsim and mic_native jars and booting the rtsim arm's save (StressC from pr1; StressB
from myl2, after its glass verdict): both boot (52 and 72 s) and run normally (StressB idle 53.8 and
busy 29.9 MSPT at load 60). A reload boot of myl2's save with rtsim (92 s) restored all 7 districts,
compiled every glass box again on boot and logged no rtsim error.

- **The world loads and runs normally.** Districts, puppets and journals are inert files.
- **`doTraderSpawning` stays false.** rtsim switches wandering traders off through the gamerule
  (TraderSpawning, design C8) and restores it only when its own module is switched off, so removing
  the jars leaves traders off for good (seen on StressC; StressB's own world already had it off).
  See section 8.
- **One WARN listing the 13 rtsim mods as `MISSING`** (NeoForge compares the world's mod list).
  Harmless.
- **One error line per seeded chunk** on first load: `Encountered unknown or non-serializable data
  attachment mic_rtsim:industry_seeds. Skipping.` (73 on StressC, 46 on StressB). NeoForge drops the attachment and
  the line does not come back once the chunk is saved again. Harmless, but it looks alarming.
- **Leftover files:** `data/mic_rtsim_*.dat` and `world/rtsim/` (tens of kB). Harmless.

## 7. Listed, not fixed

- **Glass box recompiles while contraptions settle.** When a factory's chunks load, its gantries,
  bearings and elevators move until they reach their ends, and every move changes the chunk's
  fingerprint: 14 to 21 compile restarts in the first 2.5 to 3 minutes on StressB (my4, myl2), each one
  resetting the checksum window, then quiet. (a) Recommended: leave it; it costs a few compiles once
  per load and the verdict came 5 minutes after it settled. (b) Leave chunks with a moving Create
  contraption out of the fingerprint until it stops.
- **A detection is one tick, however big:** a rescan walks every chunk of the districts in question in
  the tick it runs, and `Districts.industrial` asks up to 15 capabilities of every non-industrial
  block entity there (half the rescan's time in the profiles). On StressA a full pass over 39
  districts took 89 to 160 ms in one tick (al3, at load 60), every 6,000 ticks; reloading five tiles
  at once took 139 ms (al2). On a quiet machine it would be less (not measured). It is the largest
  single rtsim tick seen. (a) Recommended: give a rescan a block entity budget per tick, as the glass
  fingerprint pass has (256), finishing over several ticks, and key its cache by `long` position.
  (b) Stagger the periodic pass, one district at a time: a smaller change, but no help for a mass
  chunk load. (c) Leave it.
- **The engine's learned line edges cannot be seen in game.** Station arrivals and departures show
  in `/rtsim district perf`, but the line edges the engine builds from them (trips, travel time,
  cargo between two district stations) have no command. A `district info` line would make station
  learning checkable on a server.
- **Sampler and detection outliers under load:** the slowest single inventory read was 65 to 77 ms in
  myl2 and al3 (load 60, nice 15 to 19) against 8 to 30 ms in quieter runs. Wall-clock timings on a
  starved server; worth a look only if it shows on the deploy server.
- **Two other event handlers still treat a FakePlayer as a player:** `BridgeEvents.onPlace` (the G12
  player-block marks) and the Ice and Fire plugin's break hook. Outside this test's worlds; same fix
  when wanted.
- **The GameTest servers have no Lithium,** so Lithium-only paths (1ed1774) are covered by a unit
  test, not a GameTest.

## 8. For Ben

Recommended option first in each.

1. **Uninstall leaves wandering traders off.** (a) Recommended: replace the gamerule write with a
   spawn veto (cancel the wandering trader's spawn while rtsim runs; nothing persists in the world),
   and add an uninstall note to the pack docs. (b) Keep the gamerule and document "set
   doTraderSpawning true after removing rtsim". (c) Leave as is.
2. **Create's train collision check** is the single biggest cost on a busy train world (about 20% of
   the server thread at 180 trains). (a) Recommended: report it upstream with StressC's world (the
   world is theirs, attached to their PR) and leave it. (b) A pack-side mixin with a spatial index.
   rtsim's train simulation does not need it.
3. **Server heap for big Create worlds:** at `-Xmx6G` the live heap (1.7 GB) sits near G1's marking
   threshold and costs 10 to 16 ms of background CPU per tick. (a) Recommended: give the deploy
   server `-Xms` equal to `-Xmx` (or at least 8 GB) so marking starts later. (b) Leave the JVM flags.
4. **Old worlds lose item data on upgrade** (copycats, toolboxes, deployers, chutes in a 1.19.2
   world come back as count 1 without components). This is Create and Minecraft, not rtsim. (a)
   Recommended: tell players not to bring 1.19.2 worlds, or (b) write an NBT pass in the style of
   `remap_tracks.py` for the common Create block entities if a server ever migrates.
5. **Create cannot save a train holding a schedule entry from a missing mod** (Create Railways
   Navigator here): the save throws and the server never stops. Worth reporting upstream; the
   clean-up script handles it for a migration.
6. **Station learning was only tested on StressA's passenger metro** (no cargo, so no cargo ports or
   exchange rules). (a) Recommended: the synthetic generator (ST1's second half) should place
   scheduled cargo trains between districts with Portable Storage Interfaces. (b) Script schedules
   into StressC's trains with a datapack.
7. **Pack log noise seen on every boot** (not rtsim): KubeJS `auto_reload.js` ("'global' cannot be
   assigned") and `create_power_loader.js` ("invalid return"), and Ice and Fire's hippocampus crash,
   worked around here with a KubeJS spawn cancel.
8. **Thermoo floods `debug.log`** (not rtsim): it logs "Available Temperature Effects" at DEBUG about
   2,000 lines a second, 77 to 88 MB of `debug.log` in 10 to 16 minutes, on both arms; the file
   rotates every few minutes and buries everything else. (a) Recommended: a `log4j2` filter or the
   Thermoo config that silences it in the deploy server. (b) Leave it: `latest.log` (INFO) is clean.
9. **Create 6 zeroes Create 0.5 station, signal and curve positions on upgrade** (section 4.4), with
   no warning. (a) Recommended: report it upstream (the fix is a few `readBlockPos` fallbacks in
   Create's edge point and curve readers) and keep `remap_tracks.py --positions-only` for any
   migration. (b) Only the script.
10. **The detection tick on many districts** (section 7): (a) Recommended: a budgeted, multi-tick
    rescan. (b) A staggered periodic pass. (c) Leave it; 5-minute spikes of about 0.1 s.

## 9. Reversible decisions taken (SW1 to SWn)

| Id | Decision | Why |
|---|---|---|
| SW1 | Server base copied from the te-perf server (pack dc49fe7, 168 mods), spark kept | the pack as deployed; spark for profiles |
| SW2 | KubeJS script cancelling `iceandfire:hippocampus` spawns in the test servers only | an Ice and Fire crash unrelated to rtsim killed servers |
| SW3 | Simple Voice Chat port per slot | two servers on the default UDP 24454 shut one down |
| SW4 | `world/serverconfig` dropped for 1.19.2 worlds | old mod configs do not load on 1.21.1 |
| SW5 | `max-tick-time -1`, view and simulation distance 8, `sync-chunk-writes false` | the watchdog would kill a server under load; the same settings on both arms |
| SW6 | Heap 6 GB, 10 GB for StressA | the deploy server's size; StressA needs more |
| SW7 | Steam 'n' Rails track to andesite, bogeys to Create's | the brief's clean-up; an unknown bogey crashes Create |
| SW8 | Missing mods' blocks and block entities removed before the upgrade | what the server does anyway, without 166,000 error lines |
| SW9 | Foreign schedule entries removed or turned into Create delays | otherwise the world cannot save |
| SW10 | `mic_native` debug-info build for the runs | readable native frames in profiles |
| SW11 | RCON end marker (a second, unknown-type packet after each command) | concurrent RCON replies interleaved |
| SW12 | No `spark tps` in the windows | it answers asynchronously into the next command |
| SW13 | JFR for allocation profiles | spark's allocation mode needs a restart per mode |
| SW14 | StressA scene of five 256-chunk tiles | the whole map does not fit in memory at once |
| SW15 | JUnit set in the create plugin (pure Java, no Minecraft on the class path) | the chute and tick-order fixes needed unit tests |
| SW16 | Upward chute: a bottom chute takes the grab period, a middle one the transit time; CALIBRATED until checked in game | Create's code for upward chutes, not verified in game |
| SW17 | Lithium's sleeping ticker skipped in the tick-order read | it has no position |
| SW18 | Harness runs servers at `nice -n 10`, the harness itself not niced again | two nices put the servers at 19 and inflated MSPT |
| SW19 | StressB's rescan rate profiled in a long busy window | to see the deployer edit storm |
| SW20 | A `FakePlayer` is a machine: no seed, no edit, no survey; breaking boundary stock still counts as a port | automation is not the player changing the factory (INDUSTRY.md §3.3) |
| SW21 | GameTests use a real server player (`FakePlayers.editor`) to stand in for a player | after SW20 a FakePlayer no longer counts |
| SW22 | `IndustryPlugin.perfLine()` for plugin figures in `/rtsim district perf` | train numbers without a new command |
| SW23 | One pair on Greggi | nothing in it for rtsim to model |
| SW24 | `remap_tracks.py` keeps schedule condition progress and rebuilds it (zeros, empty compounds) for POST_TRANSIT trains whose lists do not match their entry | deleting it crashed Create 6 thirty seconds after boot (al1, both arms) |
| SW25 | The first long StressB run (myl) stopped by hand to free memory for the StressA pair; rerun after it | StressA needs two 10 GB servers on a shared laptop |
| SW26 | Glass fingerprints cached per block entity type (is it Create's) and per block state (the structural hash) | fingerprint passes were rtsim's largest cost on StressB after the fixes |
| SW27 | The compile log names the residual's commonest blocks | the brief asks what the glass box could not model; `district info` had only a count |
| SW28 | GameTest worlds left by earlier runs moved aside (to `/tmp/sw-old-gametest-worlds/`) before the GameTest rerun | reused worlds (game time 175,585) failed eight window and herd tests that pass on fresh worlds |
| SW29 | `remap_tracks.py --positions-only` converts Create 0.5 station, signal and curve positions before the first Create 6 load | Create 6 zeroes them silently; station learning needs them |
| SW30 | StressA upgraded again from the pre-upgrade clean-up copy (`work/stress-a-remap2`) rather than patched after the fact | the first upgraded copy had already saved the zeros |
| SW31 | myl2 (StressB, 20-minute window) ran at nice 19: the harness was started under `nice` by mistake | a single-arm run; only its glass, reconcile and spark shares are used, not its MSPT |
| SW32 | StressA train pair al3: 15-minute busy window, 3-minute unload and reload, rtsim arm first | time for metro round trips; arms alternate between pairs |
| SW33 | StressB's reload and uninstall boots taken from myl2's save (jars 7177451, after its glass verdict) | the latest jars and a world with a judged glass box |
| SW34 | Detection spikes listed with a plan (section 7, section 8 item 10), not fixed | a scheduling change in detection, not a bug; the fix touches how districts are found |
| SW35 | The al3 idle delta (+15.5 MSPT, before any district) read as noise | it ran just after boot at load 64 with an MSPT standard deviation of 36 ms; al2's was -2.2 |
