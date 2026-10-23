# Stress worlds: results (DECISIONS.md row ST1), 2026-10-21

mic-rtsim was booted on four real Create worlds, run with and without rtsim side by side, profiled,
saved, reloaded and uninstalled. This page gives the numbers, what broke, what was fixed and what is
left. The worlds are described in `research/stress-worlds-2026-10-21.md`. The downloads and every copy
stay in `~/mic-stress-worlds/` (outside git) and are never redistributed. Section 10 (2026-10-22) adds a
synthetic world of up to 200 districts, 6,400 factories and 360 cargo trains.

## 1. Bottom line

1. **rtsim costs little server time on these worlds.** Spark puts rtsim's own work at 0.2% to 0.5% of
   the server thread (about 0.1 to 0.25 ms per tick) on PR #9513, Mysda and Alvedon (0.9% on Alvedon
   before the fingerprint cache). Paired MSPT differences (on minus off) are within the noise between
   two identical servers. See section 3 for the table. Its one visible cost is a 90 to 160 ms
   detection tick every 5 minutes on Alvedon's 39 districts (section 7).
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
   pairwise `findCollidingTrain`, about 20% of the server thread on PR #9513's 180 trains) and G1
   garbage collection under Create's allocation.
4. **Station learning works on the one world with scheduled trains,** once a Create upgrade bug is
   undone: Create 6 sets every Create 0.5 station, signal and curve position to 0,0,0 on its first
   load, so on the first Alvedon run rtsim saw no arrival at all. With the positions converted before
   the upgrade (`remap_tracks.py --positions-only`), 7 metro trains made 67 arrivals and 65
   departures at district stations in 26 minutes, 13 of them simulated while the scene had their
   districts unloaded (section 4.4). PR #9513's 180 trains and Greggi's 18 have no schedules, and
   Mysda's 3 trains sit idle.
5. **The glass box reaches verdicts on real builds:** pass, 0 strikes, on Mysda's 2,779-node factory
   after a 20-minute window, and on the 6 Alvedon districts that hold inventories.
6. **Uninstalling leaves the world clean except for one thing that matters:** the gamerule
   `doTraderSpawning` stays false (section 8, item 1).

## 2. Method

- **Server:** the MIC pack as deployed (NeoForge 21.1.249, 168 mods, Create 6.0.10), copied from the
  te-perf server (pack commit dc49fe7). Java 21, G1, `-Xms2G -Xmx6G` (10G for Alvedon), view and
  simulation distance 8, `max-tick-time -1`, run under `nice -n 10` on the shared laptop
  (Ryzen, 24 threads, 46 GiB; other agents' builds and GameTests ran at the same time).
- **rtsim:** mic-rtsim and every plugin built from this branch (base cc55edd on main, rebuilt after
  each fix; 7177451 for myl2, al3 and the last Mysda boots), with `mic_native` (debug-info build).
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
| PR #9513 | idle | 26 to 49 | 26 to 51 | +1.6 (-0.8 to +2.9) | +12.6 (+5.4 to +16.2) | +2.7 (+2.0 to +3.8) | 3 |
| PR #9513 | busy | 28 to 32 | 28 to 34 | +1.2 (+0.6 to +2.0) | +10.7 (-0.6 to +16.8) | +1.0 (+0.7 to +1.2) | 3 |
| Mysda | idle | 3.9 to 8.6 | 4.0 to 7.9 | +0.1 (-0.7 to +0.8) | +0.5 (-1.4 to +1.5) | +0.2 (-0.3 to +0.4) | 3 |
| Mysda | busy | 17.6 to 21.8 | 18.2 to 21.3 | +0.1 (-0.4 to +0.6) | +2.8 (+1.0 to +4.3) | +0.6 (+0.4 to +1.0) | 3 |
| Greggi | idle | 3.68 | 3.97 | +0.29 | +1.2 | +0.36 | 1 |
| Greggi | busy | 6.26 | 6.29 | +0.04 | +1.0 | +0.13 | 1 |
| Alvedon | idle | 10.0 to 14.8 | 7.9 to 30.3 | +6.7 (-2.2 to +15.5) | -6.0 (-28.1 to +16.1) | +1.2 (-2.4 to +4.8) | 2 |
| Alvedon | busy | 29.9 to 66.4 | 30.6 to 61.9 | -1.9 (-4.5 to +0.6) | +1.0 (+0.2 to +1.8) | -0.1 (-0.2 to 0.0) | 2 |

The Mysda pairs are my1 and my2 (before the deployer fix) and my4 (after). my3 is left out: the
laptop's load rose to 80 during it and its idle window read 95 against 165 MSPT. The Alvedon pairs
are al2 (stations at 0,0,0, so no trains in rtsim's view) and al3 (positions fixed).

**Spark (busy, server thread):**

| World | rtsim share | Per tick | Biggest rtsim parts | Biggest costs overall |
|---|---:|---:|---|---|
| PR #9513 | 0.215% | 0.11 ms | sampler 48 ms, glass 36 ms, engine hand-over 24 ms, habitat 8 ms per 60 s | Create trains 32 to 42%, of which `findCollidingTrain` 20.7% self time |
| Mysda (my1, before the fixes) | 0.44% | 0.22 ms | detection rescans, glass restarts | Create kinetics and logistics |
| Mysda (my4, after) | 0.457% | 0.23 ms | glass box 108 ms, of which fingerprint passes 96 ms; sampler 32 ms; districts (detection) 32 ms; threats 8 ms per 60 s | Create kinetics and logistics |
| Mysda (myl2, fingerprint cache) | 0.432% | 0.22 ms | glass box 68 ms, of which fingerprint passes 64 ms; sampler 48 ms; districts (detection) 16 ms; threats 12 ms per 60 s | Create kinetics and logistics |
| Alvedon (al2, e330ce3) | 0.885% | 0.38 ms | districts 128 ms (a rescan 84 ms), glass box 104 ms (fingerprint passes 68 ms), engine hand-over 88 ms, sampler 24 ms per 60 s | Create trains 33.7%, `findCollidingTrain` 3.6% |
| Alvedon (al3, 7177451) | 0.523% | 0.25 ms | districts 88 ms (a rescan 56 ms), glass box 68 ms (fingerprint passes 44 ms), engine hand-over 40 ms, threats 36 ms, sampler 16 ms per 60 s | Create trains 14.7%, `findCollidingTrain` 2.1% |

**Memory, save and boot (rtsim arm against off arm):**

| World | Live heap | Metaspace | RSS | rtsim dir | Save flush | Boot (wall, "Done") |
|---|---|---|---|---|---|---|
| PR #9513 | 1.63 to 1.70 GB, same as off | +4 to +5 MB | +70 MB | 15 to 29 kB | 0.46 to 0.67 s, same as off | 64 to 160 s wall, 4.7 to 10 s Done, same as off |
| Mysda | 1.77 GB, same as off | +4 MB | +120 MB | 47 kB | 0.58 s (2.0 s in a loaded pair), same as off | 72 to 88 s wall |
| Greggi | 1.80 GB, same as off | +4 MB | +18 MB | 7 kB | 0.56 s | 66 s wall |
| Alvedon | 1.98 GB, same as off | +4 MB | -6 to +420 MB (committed heap differs by arm) | 50 to 54 kB | 0.79 to 1.14 s (off 0.94 to 1.50 s) | 90 to 247 s wall, 11 to 19 s Done, same as off |

### 3.1 PR #9513's CPU difference is garbage collection, not rtsim

The first two PR #9513 pairs showed the rtsim arm using 16 ms more CPU per tick, idle and busy, with
zero districts in the idle window. The server thread differed by only 1 to 4 ms. JFR and the GC logs
explain it:

- both arms allocate about 0.3 to 0.5 GB/s, 57% of it `Vec3` from Create's train collision code;
  rtsim's own allocation is 0.2% of the total;
- the live heap is about 1.7 GB of 6 GB, near G1's marking threshold, so one arm often sits in
  back-to-back concurrent marking (old GCs every 4 s) while the other does not. The CPU difference is
  G1's concurrent threads, plus the write-barrier work marking puts on the server thread;
- the third pair flipped it: busy CPU per tick -0.6 ms (rtsim arm lower).

So the CPU-per-tick column on PR #9513 is which arm landed in marking, not rtsim. A larger heap or
`-XX:InitiatingHeapOccupancyPercent` tuning would remove it for both (section 8).

## 4. Per world

### 4.1 Create PR #9513 train-collision world

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

### 4.2 Mysda "This is Create"

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

### 4.4 Swedish Panda "Alvedon" metro

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
id, not by position). On Alvedon that was 172 edge points, 1,125 signal tiles, 6,124 curve ends in
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
- **al3** (jars 7177451, Alvedon upgraded again from the fixed clean-up, SW30): the same 39 districts,
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
stacks in that form. After the Alvedon runs (every forceloaded chunk loaded and saved under 1.21.1)
the rest is still in the old form in chunks that never loaded (2,391 copycat stacks, 12 deployer
stacks of 390 items, 11 chute stacks of 61 items), and the stacks in loaded chunks were rewritten as
predicted, count 1 and no components (90 copycats, 23 mechanical rollers, 3 creative crates, 1 belt
stack). Copycats lose nothing (one block, no tag); a deployer, chute or basin holding more than one
item keeps one when its chunk first loads.

## 5. What broke, and the fixes

All on branch `test/stress-worlds`, each with a failing test first.

| Commit | What broke | Where it showed | Fix | Test |
|---|---|---|---|---|
| 1672a3a | `IllegalArgumentException: downward motion must be negative: 0.075`: a fan pushing items up a chute failed the whole district's glass compile, every time | Mysda district 1 | `Cycles.chuteTicksPerBlock` models upward chutes (Create's own rule: start at 0, hand off above 1); a bottom chute takes the grab period, a middle one the transit time. Flagged CALIBRATED: not yet checked in game. Main fixed the same bug on its own (e1ff012, found on the omegasrv deploy rehearsal) with `Cycles.chuteTicksPerBlockUp` and a chute's direction in the fingerprint; the merge (7e4042a) keeps main's version, same numbers | `CyclesChuteTest` (new JUnit set in the create plugin), now on main's function |
| 1ed1774 | `NullPointerException` reading the level's tick order: Lithium parks sleeping block entities behind a shared ticker whose position is null | Mysda, with Lithium (the GameTest servers have no Lithium) | `TickOrder.places` skips a ticker with no position | `TickOrderTest` |
| 59c68f4 | Create's deployer is a NeoForge `FakePlayer`, and rtsim counted its block placements and breaks as a player editing the district: a journal edit, a glass compile restart, a district rescan and a threat survey each time | Mysda: glass never finished, 1,051 rescans, surveys every 200 ticks | `IndustryEvents` and `ThreatEvents` treat `Player.isFakePlayer()` as a machine (no seed, no edit, no survey); a machine breaking boundary stock is still a port. INDUSTRY.md §3.3 says so. GameTests that stood in for a player with a FakePlayer now use a real server player (`FakePlayers.editor`) | `ThreatGameTests.machineEditsAreNoPlayerEdits` |
| e330ce3 | `/rtsim district perf` had no train figures | all train worlds | an optional `IndustryPlugin.perfLine()`; the Create plugin reports arrivals, simulated arrivals, departures, cargo ports, trains seen and at a station | covered by the existing train GameTests |
| 7177451 | Hot spot, not a bug: the glass fingerprint pass hashed every block state's properties and asked the registry for each block entity type's namespace, on every pass, though both answers are fixed | Mysda my4 spark: 96 of rtsim's 184 ms per busy minute | cached per block state (`CreateGlass`) and per block (`GlassCompiler`); same answers. The compile log also names the residual's commonest blocks | `GlassExtractionGameTests.fingerprintsCountStructureNotModelState` |

Tool fixes (the clean-up script, not rtsim): 842f0ec keeps schedule condition progress (deleting it
crashed Create 6), e0ff91a converts the Create 0.5 positions Create 6 zeroes (section 4.4). Both
checked by dry runs, an idempotence rerun and a Create 6 boot of the result.

GameTests after the fixes: core 58 of 58 and create 146 of 146 passed (at 7177451, on fresh GameTest
worlds; on worlds reused from earlier runs six create window tests and two herd tests fail, SW28).

After the merge with main (7e4042a), every suite: core 62, ribbits 5, carryon 2, create 147, Ice and
Fire 8, and Ore Excavation, Diesel, Ender Transmission, Destroy, climate and radar 1 each, all passed.
PowerGrid's server failed to boot once (below) and passed 2 of 2 on the rerun.

## 6. Uninstall

Removing the rtsim and mic_native jars and booting the rtsim arm's save (PR #9513 from pr1; Mysda
from myl2, after its glass verdict): both boot (52 and 72 s) and run normally (Mysda idle 53.8 and
busy 29.9 MSPT at load 60). A reload boot of myl2's save with rtsim (92 s) restored all 7 districts,
compiled every glass box again on boot and logged no rtsim error.

- **The world loads and runs normally.** Districts, puppets and journals are inert files.
- **`doTraderSpawning` stays false.** rtsim switches wandering traders off through the gamerule
  (TraderSpawning, design C8) and restores it only when its own module is switched off, so removing
  the jars leaves traders off for good (seen on PR #9513; Mysda's own world already had it off).
  See section 8.
- **One WARN listing the 13 rtsim mods as `MISSING`** (NeoForge compares the world's mod list).
  Harmless.
- **One error line per seeded chunk** on first load: `Encountered unknown or non-serializable data
  attachment mic_rtsim:industry_seeds. Skipping.` (73 on PR #9513, 46 on Mysda). NeoForge drops the attachment and
  the line does not come back once the chunk is saved again. Harmless, but it looks alarming.
- **Leftover files:** `data/mic_rtsim_*.dat` and `world/rtsim/` (tens of kB). Harmless.

## 7. Listed, not fixed

- **Glass box recompiles while contraptions settle.** When a factory's chunks load, its gantries,
  bearings and elevators move until they reach their ends, and every move changes the chunk's
  fingerprint: 14 to 21 compile restarts in the first 2.5 to 3 minutes on Mysda (my4, myl2), each one
  resetting the checksum window, then quiet. (a) Recommended: leave it; it costs a few compiles once
  per load and the verdict came 5 minutes after it settled. (b) Leave chunks with a moving Create
  contraption out of the fingerprint until it stops.
- **A detection is one tick, however big:** a rescan walks every chunk of the districts in question in
  the tick it runs, and `Districts.industrial` asks up to 15 capabilities of every non-industrial
  block entity there (half the rescan's time in the profiles). On Alvedon a full pass over 39
  districts took 89 to 160 ms in one tick (al3, at load 60), every 6,000 ticks; reloading five tiles
  at once took 139 ms (al2). On a quiet machine it would be less (not measured). It is the largest
  single rtsim tick seen. (a) Recommended: give a rescan a block entity budget per tick, as the glass
  fingerprint pass has (256), finishing over several ticks, and key its cache by `long` position.
  (b) Stagger the periodic pass, one district at a time: a smaller change, but no help for a mass
  chunk load. (c) Leave it.
  **Built as option (a) (DS-11).** Measured on Alvedon with the same jar and only the budget changed
  (256 against 10,000,000, which is the old one-tick detection): the biggest detection reads 960 block
  entities; its longest tick fell from 16.1 ms to 11.1 ms (277 block entities, a cold first look: about
  40 us a block entity, against 17 us for the whole pass in one tick), and the total detection time did
  not change (66.5 ms against 68.8 ms over the same 41 detections). The 39-district pass that took
  89 to 160 ms no longer exists on Alvedon: DS-10 stopped storage seeding, so the world has 4 districts
  and a full pass is 0.1 to 5 ms. A GameTest of nine chunks with 1,800 inert block entities
  (`aBigDetectionsLongestTickIsBelowTheOneCallTime`) measures 1.3 to 1.5 ms in one call and a 0.4 to
  0.5 ms longest tick at the default budget over 5 ticks. The pair 'before' (main at 42fec21) against
  'after' (the branch) on Alvedon's few-adopt scene showed 109 against 89 ms of detection in the settle
  window; a 320-adopt scene was too noisy to read (0.99 to 2.19 s for the same code under a loaded
  laptop).
- **The engine's learned line edges cannot be seen in game.** Station arrivals and departures show
  in `/rtsim district perf`, but the line edges the engine builds from them (trips, travel time,
  cargo between two district stations) have no command. A `district info` line would make station
  learning checkable on a server. `rtsim-cli lines` now reads them from a saved world (section 10).
- **Sampler and detection outliers under load:** the slowest single inventory read was 65 to 77 ms in
  myl2 and al3 (load 60, nice 15 to 19) against 8 to 30 ms in quieter runs. Wall-clock timings on a
  starved server; worth a look only if it shows on the deploy server.
- **Two other event handlers still treat a FakePlayer as a player:** `BridgeEvents.onPlace` (the G12
  player-block marks) and the Ice and Fire plugin's break hook. Outside this test's worlds; same fix
  when wanted.
- **The synthetic "thousands of machines" world was not built** that night. It has been built
  since, with the cargo trains section 8 item 6 asked for: section 10.
- **The GameTest servers have no Lithium,** so Lithium-only paths (1ed1774) are covered by a unit
  test, not a GameTest.
- **The PowerGrid GameTest server failed to boot once and then passed.** Create's Registrate threw
  "Found unused register callbacks" at the first `RegisterEvent`, but logged no unused entry. That
  means its map counted entries it did not hold, which points to a race during parallel mod
  construction, not to rtsim. The same branch passed on the rerun, and main passed it too. If it
  comes back, look at which mod touches `Create.REGISTRATE` from its own constructor thread.

## 8. For Ben

Recommended option first in each.

1. **Uninstall leaves wandering traders off.** (a) Recommended: replace the gamerule write with a
   spawn veto (cancel the wandering trader's spawn while rtsim runs; nothing persists in the world),
   and add an uninstall note to the pack docs. (b) Keep the gamerule and document "set
   doTraderSpawning true after removing rtsim". (c) Leave as is.
2. **Create's train collision check** is the single biggest cost on a busy train world (about 20% of
   the server thread at 180 trains). (a) Recommended: report it upstream with PR #9513's world (the
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
6. **Station learning was only tested on Alvedon's passenger metro** (no cargo, so no cargo ports or
   exchange rules). (a) Recommended: the synthetic generator (ST1's second half) should place
   scheduled cargo trains between districts with Portable Storage Interfaces. (b) Script schedules
   into PR #9513's trains with a datapack. Done as (a): section 10, whose 10.6 has what it found.
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
| SW6 | Heap 6 GB, 10 GB for Alvedon | the deploy server's size; Alvedon needs more |
| SW7 | Steam 'n' Rails track to andesite, bogeys to Create's | the brief's clean-up; an unknown bogey crashes Create |
| SW8 | Missing mods' blocks and block entities removed before the upgrade | what the server does anyway, without 166,000 error lines |
| SW9 | Foreign schedule entries removed or turned into Create delays | otherwise the world cannot save |
| SW10 | `mic_native` debug-info build for the runs | readable native frames in profiles |
| SW11 | RCON end marker (a second, unknown-type packet after each command) | concurrent RCON replies interleaved |
| SW12 | No `spark tps` in the windows | it answers asynchronously into the next command |
| SW13 | JFR for allocation profiles | spark's allocation mode needs a restart per mode |
| SW14 | Alvedon scene of five 256-chunk tiles | the whole map does not fit in memory at once |
| SW15 | JUnit set in the create plugin (pure Java, no Minecraft on the class path) | the chute and tick-order fixes needed unit tests |
| SW16 | Upward chute: a bottom chute takes the grab period, a middle one the transit time; CALIBRATED until checked in game (superseded by main's e1ff012 at the merge, same rule) | Create's code for upward chutes, not verified in game |
| SW17 | Lithium's sleeping ticker skipped in the tick-order read | it has no position |
| SW18 | Harness runs servers at `nice -n 10`, the harness itself not niced again | two nices put the servers at 19 and inflated MSPT |
| SW19 | Mysda's rescan rate profiled in a long busy window | to see the deployer edit storm |
| SW20 | A `FakePlayer` is a machine: no seed, no edit, no survey; breaking boundary stock still counts as a port | automation is not the player changing the factory (INDUSTRY.md §3.3) |
| SW21 | GameTests use a real server player (`FakePlayers.editor`) to stand in for a player | after SW20 a FakePlayer no longer counts |
| SW22 | `IndustryPlugin.perfLine()` for plugin figures in `/rtsim district perf` | train numbers without a new command |
| SW23 | One pair on Greggi | nothing in it for rtsim to model |
| SW24 | `remap_tracks.py` keeps schedule condition progress and rebuilds it (zeros, empty compounds) for POST_TRANSIT trains whose lists do not match their entry | deleting it crashed Create 6 thirty seconds after boot (al1, both arms) |
| SW25 | The first long Mysda run (myl) stopped by hand to free memory for the Alvedon pair; rerun after it | Alvedon needs two 10 GB servers on a shared laptop |
| SW26 | Glass fingerprints cached per block entity type (is it Create's) and per block state (the structural hash) | fingerprint passes were rtsim's largest cost on Mysda after the fixes |
| SW27 | The compile log names the residual's commonest blocks | the brief asks what the glass box could not model; `district info` had only a count |
| SW28 | GameTest worlds left by earlier runs moved aside (to `/tmp/sw-old-gametest-worlds/`) before the GameTest rerun | reused worlds (game time 175,585) failed eight window and herd tests that pass on fresh worlds |
| SW29 | `remap_tracks.py --positions-only` converts Create 0.5 station, signal and curve positions before the first Create 6 load | Create 6 zeroes them silently; station learning needs them |
| SW30 | Alvedon upgraded again from the pre-upgrade clean-up copy (`work/alvedon-remap2`) rather than patched after the fact | the first upgraded copy had already saved the zeros |
| SW31 | myl2 (Mysda, 20-minute window) ran at nice 19: the harness was started under `nice` by mistake | a single-arm run; only its glass, reconcile and spark shares are used, not its MSPT |
| SW32 | Alvedon train pair al3: 15-minute busy window, 3-minute unload and reload, rtsim arm first | time for metro round trips; arms alternate between pairs |
| SW33 | Mysda's reload and uninstall boots taken from myl2's save (jars 7177451, after its glass verdict) | the latest jars and a world with a judged glass box |
| SW34 | Detection spikes listed with a plan (section 7, section 8 item 10), not fixed | a scheduling change in detection, not a bug; the fix touches how districts are found |
| SW35 | The al3 idle delta (+15.5 MSPT, before any district) read as noise | it ran just after boot at load 64 with an MSPT standard deviation of 36 ms; al2's was -2.2 |
| SW36 | At the merge with main, main's upward chute fix (e1ff012) kept over this branch's 1672a3a, and `CyclesChuteTest` moved onto it | the same bug fixed twice; main's also puts a chute's direction in the fingerprint |
| SW37 | The PowerGrid suite's one failed boot read as a flake, not a blocker for landing | Registrate's map said it was not empty but logged no entry (a race); the same tree passed on the rerun, and origin/main passed too |

## 10. The synthetic stress world (ST1's machines, ST3's cargo trains), 2026-10-22

Section 7 left the synthetic world unbuilt and section 8 item 6 asked for cargo trains with Portable
Storage Interfaces. Both now exist: a generator builds a flat world of Create factories in districts,
with scheduled cargo trains between them, and the same harness measured it at two scales.
`tools/stress-worlds/SYNTHETIC.md` says how to build and measure one.

In short:

- **Cost:** rtsim's own tick handlers take 0.7 to 0.8 ms per tick at 40 districts (1,280
  factories, 64 trains) and 1.2 to 1.6 ms at 200 (6,400 factories, 360 trains). MSPT differences
  are the size of the A/A controls'.
- **Detection** was the largest single tick: 0.17 to 0.75 s for a full pass, and up to 0.37 s for
  one adopt at 200 districts (10.4). DS-11 spreads the passes: at syn40 the periodic pass now runs
  over 70 ticks, with a longest tick of 14 to 23 ms. Commands still detect in one call: an adopt
  costs 41 to 46 ms at 40 districts and grows with the districts found, as before DS-11 (10.8).
- **Station learning:** every line edge is learned, with exact trips and travel times. The first
  runs missed most unloads, and simulated calls took full carriages out of the factories' input
  chests (10.5). Both are fixed (10.8): at syn40 every station's rule now has its load and unload,
  a simulated call moves about 30 items through its station's own port vaults, and the factories'
  chests change only by their own machines' work.
- **syn200 again (10.9),** on main b5cd4d2: every station's load and unload learned at 200 districts,
  the busy window's worst rtsim tick 34 to 51 ms (201 to 230 before), and rtsim's whole cost 1.6 to
  1.7 ms a tick, which 10.3's MSPT could not see (it ends before the Post handlers). New: edges from
  a station to itself after deferred adopts, and a slower one-call rescan.

### 10.1 What the generator builds

- **How:** one GameTest in the create plugin (`gametest/stress/StressWorld.java`), run by its own
  Gradle run (`:create:runStressGen -PskipJni`, wrapped by `tools/stress-worlds/synth.sh`). Everything
  in it is Create's, placed through Create's own code: the glass corpus factory builders, track,
  station block entities, train assembly and `Schedule`. It adds no block, item or entity, and nothing
  ships in the mod's runtime path (D3): the namespace runs only in that Gradle run, behind a system
  property.
- **Districts:** a grid of near-square aspect. Each district is 2 chunk rows by a west station
  column, ceil(cells / 8) machine columns and an east station column (6 x 2 chunks at 32 factories),
  then one empty chunk column and row, so rtsim's 8-neighbour detection keeps districts apart (40 of
  40 and 200 of 200 found). Each factory is one of the glass corpus's four builds (millstone flour,
  press sheets, saw shafts, brass mixing; weights 4:2:2:1 by seed) with a creative motor and full
  input chests.
- **Trains:** each pair of horizontal neighbours in a grid row gets lanes in turn (default two, at
  most four). A lane is one straight track from "D{a} E{k}" to "D{a+1} W{k}". A train is one carriage
  on a small bogey with controls at both ends, a blaze burner conductor, a chest and a PSI. Its cyclic
  schedule is: go to B, wait 10 s, go to A, wait 10 s. Good X goes A to B and good Y comes back.
- **Ports:** beside each station, a sink vault, a smart chute pulling the incoming good from the
  station's PSI, the PSI, a smart chute pushing the outgoing good, and a supply vault holding 8,000 of
  it. A PSI only docks when the carriage's PSI enters its block at 0.25 blocks per tick or less, which
  happens on the train's last block of braking. So each port is built where the first stop actually
  put the carriage PSI (calibrated, not computed), and docking starts on the second visit.
- **Check:** the test, and so the build, fails unless every line's two sink vaults hold the good the
  other end sent.

| World | Districts | Factories (millstone, press, saw, brass) | Trains | Build | World size |
|---|---:|---|---:|---|---:|
| syn40 (seed 1) | 40 (5 x 8) | 1,280 (565, 264, 322, 129) | 64 on 32 pairs | 1,248 game ticks, 12.8 s in the test, 96 s with the server | 19 MB |
| syn200 (seed 2) | 200 (10 x 20) | 6,400 (2,855, 1,496, 1,379, 670) | 360 on 180 pairs | 1,470 game ticks, 103 s in the test, 234 s with the server | 40 MB |

Most of the build is the calibration stops (1,176 ticks at both sizes).

### 10.2 Method

The section 2 harness, server and pack, with these changes:

- **Jars:** set `sg-91eeea0`, i.e. this branch after merging main 8522dd2 with `mic_native` from
  7177451. It adds the tick cost below. The budgeted rescan (`fix/district-seeds`, 974ba1c) had not
  reached main, so these runs use main's one-tick detection.
- **rtsim's own tick cost:** a new `TickCost` times rtsim's server tick handlers (the industry
  pre-tick, the queues and the session end-of-tick) on every tick. `/rtsim perf` reports mean, p50,
  p99, p99.9 and worst since `/rtsim perf reset`, which the harness sends as the busy window starts.
  Create's own train hooks that rtsim mixes in (arrival and departure) fall outside it.
- **Controls:** arms of a run start 5 s apart and share the laptop's load.
  - syn40 ran as two triples, sg40a (off, on, on) and sg40b (on, on, off). Each gives two on-off
    comparisons and an A/A control between two rtsim arms on one world.
  - syn200 ran as pairs, since three 10 GB servers did not fit beside other agents' servers: sg200d
    (off, on) and sg200c (on, on, the A/A control). sg200a (on, off) came first, before the harness
    synced its arms' windows (SG22). Its rtsim arm's adopts and reports started its busy and reloaded
    windows 110 s after the off arm's, so its deltas are left out. Its tick cost, detection and line
    edges are used. sg40a had the same skew, smaller: 14 s.
- **Scene** (written by the generator):
  1. forceload every district row;
  2. `rtsim district adopt` (radius 1) every three chunks along each district;
  3. settle for 120 s;
  4. a 600 s busy window. At 20 TPS it spans the periodic detection pass at game tick 12,000;
     syn200 runs at 8 to 10 TPS and reached it in sg200d and sg200c, not in sg200a;
  5. a 60 s spark profile;
  6. one timed `rtsim district rescan`;
  7. unloaded and reloaded windows of 90 s;
  8. save.

  Since SG22 the arms wait for each other before steps 4 and 7.
- **Line edges and exchange rules** are read from each rtsim arm's saved world with `rtsim-cli lines`
  (new; no in-game command lists them, section 7).
- **Load:** the laptop (24 threads) was shared with other agents' pack servers, GameTests, a game
  client and builds. The one-minute load average over the busy windows was 3 to 21 in sg40a, 3 to
  39 in sg40b (another agent's Rust build), 4 to 10 in sg200d and 4 to 25 in sg200c. sg200c's busy
  MSPT (131 to 133, standard deviation 50 ms, against sg200d's 102 and 10 ms) shows that load; its
  two arms shared it.

### 10.3 Numbers

MSPT, whole-process CPU per tick and server-thread CPU per tick, in ms, busy window (600 s, all
districts loaded, every train running).

| Run | Arm | MSPT (sd) | CPU/tick | Server thread/tick | TPS |
|---|---|---:|---:|---:|---:|
| sg40a | off | 39.63 (19.8) | 55.9 | 43.49 | 18.81 |
| sg40a | on | 40.29 (20.2) | 57.7 | 44.74 | 18.65 |
| sg40a | on (A/A) | 39.36 (19.4) | 58.1 | 44.13 | 18.79 |
| sg40b | on | 55.55 (34.0) | 68.2 | 51.76 | 16.35 |
| sg40b | on (A/A) | 55.24 (33.5) | 67.7 | 51.79 | 16.40 |
| sg40b | off | 54.54 (33.4) | 65.3 | 50.56 | 16.59 |
| sg200d | off | 101.88 (9.6) | 130.6 | 108.58 | 9.87 |
| sg200d | on | 101.92 (9.6) | 134.5 | 109.81 | 9.74 |
| sg200c | on | 132.70 (50.7) | 172.4 | 131.16 | 8.08 |
| sg200c | on (A/A) | 131.13 (50.4) | 172.0 | 129.81 | 8.16 |

sg200a's busy windows (on 117.93, off 121.62) started 110 s apart and are not compared.

| Comparison (busy) | dMSPT | dCPU/tick | dServer thread/tick |
|---|---:|---:|---:|
| syn40, on minus off (sg40a, 2 comparisons) | +0.66, -0.27 | +1.8, +2.2 | +1.25, +0.64 |
| syn40, A/A (sg40a) | -0.93 | +0.4 | -0.61 |
| syn40, on minus off (sg40b, 2 comparisons) | +1.01, +0.70 | +3.0, +2.4 | +1.20, +1.23 |
| syn40, A/A (sg40b) | -0.31 | -0.5 | +0.03 |
| syn200, on minus off (sg200d) | +0.04 | +3.9 | +1.23 |
| syn200, A/A (sg200c) | -1.57 | -0.4 | -1.35 |

At both scales the MSPT differences (-0.3 to +1.0 ms) are the size of the A/A controls' (-0.3 to
-1.6). The server thread shows about rtsim's own tick cost: +0.6 to +1.3 ms per tick on every
rtsim arm, against A/A differences of -1.4 to 0. Whole-process CPU per tick runs 2 to 4 ms higher
with rtsim, which includes the engine's own threads off the server thread. syn200 is Create-bound:
it runs at 8 to 10 TPS with or without rtsim, with block entities at 88% of the server thread and
Create's code at 76% (sg200a, off arm).

**rtsim's own tick cost** (busy window, `TickCost`):

| Run | Arm | Ticks | Mean | p50 | p99 | p99.9 | Worst (game tick) |
|---|---|---:|---:|---:|---:|---:|---:|
| sg40a | on | 11,193 | 0.69 ms | 0.49 ms | 2.40 ms | 3.82 ms | 632 ms (12001) |
| sg40a | on (A/A) | 11,273 | 0.69 ms | 0.50 ms | 2.57 ms | 4.77 ms | 326 ms (12001) |
| sg40b | on | 9,811 | 0.82 ms | 0.52 ms | 2.98 ms | 8.73 ms | 755 ms (12001) |
| sg40b | on (A/A) | 9,842 | 0.79 ms | 0.52 ms | 3.01 ms | 7.50 ms | 573 ms (12001) |
| sg200a | on | 5,271 | 1.41 ms | 1.20 ms | 5.45 ms | 9.31 ms | 15.8 ms (6784; no detection pass in the window) |
| sg200d | on | 5,849 | 1.23 ms | 1.10 ms | 2.95 ms | 7.09 ms | 212 ms (12001) |
| sg200c | on | 4,849 | 1.58 ms | 1.19 ms | 6.72 ms | 23.2 ms | 201 ms (12001) |
| sg200c | on (A/A) | 4,901 | 1.59 ms | 1.19 ms | 7.33 ms | 16.5 ms | 230 ms (12001) |

rtsim costs 0.7 to 0.8 ms a tick at 40 districts and 1.2 to 1.6 ms at 200, with five times the
districts, factories and trains. Where a busy window held the periodic detection pass, that pass
was its worst tick (10.4). The first ticks of a session are not in the table: they hold a single
1.2 to 3.6 s tick (10.6 item 4).

**Spark** (60 s profiles just after the busy window, server thread; four rtsim profiles per scale):

| Share of the server thread | syn40 | syn200 |
|---|---:|---:|
| rtsim in all | 1.18% to 1.38% | 0.80% to 1.35% |
| the same, in ms per tick | 0.6 to 0.85 | 0.8 to 1.6 |
| inventory sweeps | 0.50% to 0.61% (3,267 inventories) | 0.26% to 0.40% (16,289) |
| the glass box | 0.23% to 0.40% | 0.24% to 0.35% |
| district upkeep (load states, watching) | 0.13% to 0.32% | 0.19% to 0.55% |
| the engine hand-over | 0.1% | 0.01% to 0.2% |
| threats | 0.04% | 0.01% to 0.05% |
| Create's trains, every arm | 2.8% to 4.0% | 5.2% to 5.4% |
| `findCollidingTrain`, every arm | 0.05% to 0.08% | 0.15% to 0.31% |

Spark's per-tick figure agrees with `TickCost`'s mean. Straight lanes rarely meet, so Create's
collision check stays small here, unlike PR #9513's world (section 8 item 2).

### 10.4 Detection

Detection still runs in one tick: the budgeted rescan on `fix/district-seeds` (974ba1c) had not
reached main. Its passes are the largest rtsim ticks in these runs.

| Detection, in one tick | syn40 | syn200 |
|---|---|---|
| The timed full rescan, every district loaded | 167 and 322 ms (sg40b) | 232 to 448 ms (four arms) |
| The periodic pass (game tick 12,001) | 326 to 755 ms (four arms) | 201 to 230 ms (three arms) |
| The last adopt of the scene's loop | 31 to 36 ms | 226 to 374 ms |
| The scene's whole adopt loop (80 or 400 adopts) | 1.6 to 2.2 s | 41 to 59 s |
| The pass after the reload | 54 to 189 ms | 234 to 253 ms |
| District upkeep per tick (spark, share of the server thread) | 0.13% to 0.32% | 0.19% to 0.55% |

- **Full passes** cost 0.17 to 0.75 s here, against 89 to 160 ms for Alvedon's 39 districts
  (section 7). A synthetic district holds about 81 inventories and 285 Create block entities.
  These are wall-clock times on a shared laptop: two arms of one run differ by up to 2x, and
  syn40's passes were no cheaper than syn200's. So the cost of one pass is not simply proportional
  to the district count, and these runs cannot say what it is proportional to. What they do show
  is that a single detection tick reaches half a second on a large base.
- **Adopts:** an adopt's cost grows with the districts already found, so the scene's adopt loop is
  quadratic: each of the last adopts at 200 districts took a quarter to a third of a second, in its
  own tick. A player adopts one district at a time; a first sweep of a big base, or a script, would
  feel it.
- **Upkeep:** `Districts`' per-tick work over every district (load states, watching) showed no
  clear growth from 40 to 200 districts within the profiles' spread.
- **`sweepCostOnLargeVaults`** was not seen: no GameTest of the other branch ran here. The sweeps'
  slowest single inventory read was 12 to 23 ms at syn40 and 15 to 18 ms at syn200.
- When the budgeted rescan lands, one `sw.py pair` on syn200 with the new jars measures it against
  these runs. The worlds, scenes and jar set are kept in `~/mic-stress-worlds`.

### 10.5 Station learning with cargo

The edges are learned, with trips and travel times exact. The cargo and the exchange rules are not
right yet, for two reasons the measurement found. Both were diagnosed from the saved worlds and the
journal; neither is fixed here.

- **Line edges** (`rtsim-cli lines` on every rtsim arm's saved world): every directed edge has trips
  and cargo, 128 of 128 at syn40 (64 trains, both directions) and 720 of 720 at syn200 (360 trains).
  - Trips: 25 to 29 each at syn40; 14 to 17 at syn200, which runs at 8 to 10 TPS.
  - Travel time: 129 to 131 ticks at both scales, the real run between stations about 41 blocks
    apart.
  - Cargo per trip: real calls move about 30 items of the line's good (22 to 36: a 10 s dwell
    through one smart chute). A simulated call during the unloaded window can carry a full carriage
    of 1,664 items. At syn40 every edge had such legs, so the mean per trip was 205 to 322. At
    syn200 they appeared only where a district's glass box had a verdict before the unload: on 40
    of 720 edges in sg200d (107 reconciles after the reload), and on none in sg200a and sg200c (no
    reconcile at all).
- **Exchange rules** (checked against `stressgen.json`): every station has one, observed 21 to 26
  times at syn40 and 10 to 14 at syn200. The load good is right at every station on every arm. The
  unload good is never wrong, but it is missing:

  | Run | Arm | Stations | Unload learned | Unload missing |
  |---|---|---:|---:|---:|
  | sg40a | on | 128 | 116 | 12 |
  | sg40a | on (A/A) | 128 | 74 | 54 |
  | sg40b | on | 128 | 75 | 53 |
  | sg40b | on (A/A) | 128 | 128 | 0 |
  | sg200a | on | 720 | 0 | 720 |
  | sg200d | on | 720 | 33 | 687 |
  | sg200c | on | 720 | 0 | 720 |
  | sg200c | on (A/A) | 720 | 0 | 720 |

  - This is a timing problem. A PSI docks while the train brakes (at 0.25 blocks per tick or less),
    4 to 6 ticks before Create calls `Train.arriveAt`. The port's pull chute has emptied the incoming
    good by the time `TrainWatch.arriving` takes its snapshot.
  - So the arrival snapshot holds 0 of the incoming good, against about 30 at the partner's
    departure, and the engine (`engine.rs` 2706-2740) sees no drop to learn an unload from.
  - The stations that did learn an unload learned it by accident. Either it came on the first real
    arrival after a simulated leg (with up to 1,601 engine-loaded items still aboard), or a call where
    the dock had just started. syn200 confirms it: where no simulated leg carried cargo, no station
    learned an unload at all.
- **Simulated calls charge the wrong containers.**
  - The bridge sends a train's cargo port with no inventory (`TrainWatch.java:168`, `NO_INVENTORY`).
    The engine books a simulated exchange on the district-wide stock (`engine.rs` 2559-2586), and a
    simulated load takes the smaller of that stock and the carriage's room, which is a full carriage.
  - On reload, the reconcile spreads the removal over whatever inventories hold the good, in
    proportion (`reconcile.rs` 162-194). So the load came out of the factories' input chests, and the
    port's supply vault was not touched.
  - In sg40a's district 13 (generator district 12), three press input chests lost 576 iron ingots
    each and their presses made 1, 1 and 57 sheets. The other five lost 125 each, matching their
    123-sheet simulated run. A brass mixer's zinc chest went from 1,312 to 509. The east port's
    supply and sink vaults did not change.
  - The starving did not show in MSPT. sg40a's reloaded windows ran 7 to 8.5 ms faster on the
    rtsim arms, but those windows started 14 s apart (before SG22). With the windows synced, sg40b's
    reloaded differences were -0.1 and +1.4 ms against an A/A of +1.5, with 456 to 472 reconciles
    applied. At syn200, sg200d's was +3.3 (107 reconciles) and sg200c's A/A +0.4. A starved press
    probably still turns, so its block entities still tick.
  - Where the unload was missed, a simulated leg also carries the same good back: the edge
    D38 W0 to D37 E0 shows 4,992 iron ingots (3 x 1,664) going the wrong way (sg40a).

### 10.6 Findings and For Ben

Recommended option first in each.

1. **Exchange rules miss the unload** (10.5). **Decided (a), fixed (10.8).** The dock starts 4 to 6
   ticks before `Train.arriveAt`, so the arrival snapshot comes after the unload. (a) Recommended:
   take the arrival snapshot on the final approach, once the train is braking for the station and
   before a PSI can dock (a hook in Create's navigation). (b) Diff each departure against the
   train's previous departure: no new hook, but a stop at a station outside any district would be
   booked to the next district station.
2. **Simulated calls take from the factories and fill the carriage** (10.5). **Decided (a) with (b),
   fixed (10.8).** (a) Recommended: give each station its port inventories (the containers its PSIs
   reach) and book simulated exchanges on those. (b) Cap a simulated load at the edge's learned
   cargo per trip. (c) Keep the district-wide booking but prefer containers that feed no machine.
   (b) is the smallest change and fixes the 1,664-item legs; only (a) stops the reconcile starving
   factories.
3. **The detection tick on many districts** (10.4). **(a) landed as DS-11; syn40 re-measured in
   10.8, syn200 in 10.9.** A full pass of 0.2 to 0.45 s at 200 districts, and adopts that grow with
   the district count. (a) Recommended: land the budgeted rescan and re-measure on syn200. (b) Also
   limit an adopt's work to its own neighbourhood.
4. **A 1.2 to 3.6 s tick at the start of every rtsim session.** **Profiled (a): Project
   Atmosphere's, reached through rtsim's habitat sampler (10.8).** It coincides with a C2ME
   density-function compile on the server thread (1.0 to 3.3 s). Both arms log such compiles in
   their first half minute, the off arms one and the rtsim arms two (sg40a, sg200a). So one compile
   per boot is the pack's, and the rtsim arms' second falls inside rtsim's tick handlers. (a)
   Recommended: one boot profiled from its first tick to name the caller. (b) Leave it: once per
   boot.
5. **Create's boot race came back:** sg200b's first server failed during `RegisterEvent` with an
   `ExceptionInInitializerError` in Create's `AllAdvancements` ("unbound value
   create:andesite_alloy"). That is the same family as section 7's Registrate failure, and not
   rtsim's: the other 13 boots of that jar set passed. (a) Recommended: leave it unless the
   deploy server shows it; then find the mod that touches Create's registrate from its own
   constructor thread.

### 10.7 Reversible decisions taken (SG1 to SG24, SG30 onwards)

| Id | Decision | Why |
|---|---|---|
| SG1 | The generator is a GameTest in the create plugin under its own namespace (`mic_rtsim_stressgen`), run only by the `stressGen` Gradle run behind the `mic_rtsim.stressgen` property | Create's own code places, assembles and schedules everything; nothing reaches a player (D3) |
| SG2 | Sized by `MIC_STRESS_*` environment variables, wrapped by `synth.sh` | a Gradle run takes no arguments; one call builds and files a world |
| SG3 | Factories are the glass corpus builds, placed through a shifted helper, input chests topped up | machines the glass box already models, running for the whole measurement |
| SG4 | West station, machine columns, east station, then an empty chunk column and row; up to four lanes per neighbour pair, two by default | 8-neighbour detection keeps districts apart; straight lanes keep Create's collision check small |
| SG5 | Ports calibrated after each train's first stop | a PSI docks only at 0.25 blocks per tick or less, at the end of braking, which cannot be computed ahead |
| SG6 | The stub engine during the build; rtsim's state stripped after it | the build must not be part of what rtsim learns |
| SG7 | `TickCost` and `/rtsim perf reset` | rtsim's own tick cost, not only whole-server MSPT |
| SG8 | `stressgen.json` format 1, dev-only | to read back what was built; not a public interface |
| SG9 | The scene: forceload the rows, adopt every three chunks, settle 120 s, busy 600 s, profile 60 s, rescan, unload and reload 90 s each | every district adopted; at 20 TPS the busy window holds one periodic pass |
| SG10 | `rtsim-cli lines` | no in-game command lists line edges (section 7) |
| SG11 | The GameTest server's `serverconfig` dropped and its experimental datapacks disabled | the pack's defaultconfigs apply, as on a fresh server |
| SG12 | The GameTest world's flat terrain and game rules kept | no mobs, weather or random ticks to differ between arms |
| SG13 | One carriage with controls at both ends, a fixed 10 s dwell | the smallest Create train that runs a cyclic schedule both ways; each call moves one smart chute's worth |
| SG14 | Scales 40 x 32 (syn40, seed 1) and 200 x 32 (syn200, seed 2) | 40 is about Alvedon's district count; 200 x 32 is 6,400 factories |
| SG15 | syn40 as triples, syn200 as pairs | three 10 GB servers did not fit beside other agents' servers |
| SG16 | Jar set `sg-91eeea0`, `mic_native` from 7177451 | main at 8522dd2 plus the tick cost |
| SG17 | A timed `rtsim district rescan` in the scene | a full detection at a known time, whatever the TPS; a slow world can miss the periodic pass |
| SG18 | Heaps: generator 12 GB, servers 6 GB (syn40) and 10 GB (syn200) | the build holds the whole world loaded; syn200's live heap is 2 GB plus Create's caches |
| SG19 | Harness slots 60 to 62, under the bridge lock | clear of the other agents' slots |
| SG20 | The unload timing and the simulated calls' charging reported, not fixed | engine and bridge design; for Ben (10.6) |
| SG21 | The syn200 A/A pair rerun after Create's boot flake (sg200b to sg200c) | arm A never started |
| SG22 | The harness's arms wait for each other before the busy window and the unload | the rtsim arm's adopts, reports and rescan started its windows up to 110 s late |
| SG23 | No 200 x 8 world | 200 x 32 already gives 6,400 factories and 56,918 Create block entities |
| SG24 | sg200a (before SG22) kept for tick cost, detection and line edges, not for its deltas | its busy and reloaded windows started 110 s apart |
| SG30 | An arrival's cargo is read at the first Portable Storage Interface dock near the station (a mixin at the head of `startTransferringTo`, PATCHES.md row 19a), flag 2 | a dock comes on the last block of braking at 0.25 blocks per tick or less, before `arriveAt`, on every call that moves goods; nothing moves before it |
| SG31 | No dock near the station on a watched trip: the head of `arriveAt` (flag 2); a dock that could not name its station: the cargo at the previous departure (flag 4); a first watched stop: `arriveAt`, late (flag 0). `/rtsim district perf` counts each path | coordinator's option (b) only where no early hook fires; the flags say which path each arrival took |
| SG32 | Station ports (IND_STATION_PORTS, 0x0415): at a loaded call's departure, a walk from the docked interfaces through chutes, funnels, hoppers, pipes, pumps and valves, at most 8 blocks, loaded chunks only, no belts; sent when the set changes; cleared by a district revision | the containers the station's interfaces serve, found from what docked; belts would reach the factories |
| SG33 | A simulated call books only on its station's ports (black box: the district's pool as well, so its processes see the goods; glass: a port's store pool for the good, else the port's own contents); a station with a rule and no ports moves nothing (`exchanges_portless`) | the coordinator's (a); never the district's stock |
| SG34 | A simulated load is at most the median of the station's last 8 loaded calls' amounts of that good (placeholder window); unloads are not capped | the coordinator's (b), about 30 items here; the median ignores the first thin calls and a catch-up call after an absence |
| SG35 | Before a station call on a black-box span, the replay's change so far is placed in the ports by the reconcile's own plan (as their `held`), and the span's base moves with it | otherwise a port's bookings from earlier calls grow without bound while the replay's change waits for the reconcile, and a load finds nothing to take |
| SG36 | The first-tick stall is reported, not fixed: it is Project Atmosphere's region forecast (a fresh `RandomState` that C2ME compiles), reached through rtsim's habitat sampler and mic_climate | the cost is PA's and C2ME's; any first temperature query in a new region pays it |
| SG37 | The fixes' GameTest (`stationcargo.saw_shafts`) builds the glass corpus' saw shafts factory far from the yard and a port as the generator builds one, runs a synthetic train, and records its dock by hand (`TrainWatch.dockedForTest`) | a synthetic train has no carriage entity, so no interface can dock; the rest (ports walk, simulated call, reload, reconcile) is the real path |
| SG38 | `tools/stress-worlds/stock.py` reads the saved worlds' chests and port vaults, off arm first | factory stock and port vaults are the coordinator's question; no command lists them, and the saved world is what a player would find |
| SG39 | Jar set `sg-cce8b6a`: this branch after merging main 72fa63e (DS-11), `mic_native` from 7177451; the create suite run on the same tree just before. sg40c, started before DS-11 landed, was stopped and deleted | the coordinator asked for DS-11 in the re-run |
| SG40 | The adopt question measured by the adopt loop alone, two servers side by side (before DS-11, `sg-2313699`, and after, `sg-cce8b6a`), reading `rtsim district perf` after each adopt, with spark on both | sg40d's settle window mixes adopts with ticked passes on a loaded laptop; the perf line's last detection after an adopt is that adopt's own |
| SG41 | syn200 not re-run then (re-run on main b5cd4d2 in 10.9) | other agents' benches and GameTests were queued on the bridge lock and the load average stayed between 8 and 14 |
| SG42 | The adopt spike reported, not fixed | the adopt command and `rescanNow` are the district work's files (DS-11 left them unbudgeted on purpose) |
| SG43 | Scene phases can end by game ticks (`"ticks"` in the scene); both arms of a pair end a phase together | at 7 to 8 TPS a 600 s busy window holds about 4,600 ticks and can miss the periodic pass at 12,000 |
| SG44 | A KubeJS script on every arm (`swticks`) records each tick's ring time and gap; the gap is the whole tick | the tick ring, and so `neoforge tps`, ends before `ServerTickEvent.Post` and leaves out most of rtsim's work |
| SG45 | Page segments copied as they appear and decoded by table; `/micperf zones` read after the final save | compaction deletes the autosaves' segments; the snapshot and the write run off the server thread |
| SG46 | `/rtsim district perf` once more at the end of the reloaded window | the reloaded report comes before the reloaded window; this one covers the run up to the save |
| SG47 | Ticks with a gap over 10 s dropped (two lid-close suspends, 238 s and 1,726 s); pair a's reloaded window not compared | a suspend is one tick on both arms; pair a's reloaded window ran at a load of 30 to 32 and 3.7 TPS |
| SG48 | Jar set `sg-b5cd4d2` (main b5cd4d2, `mic_native` as before), `syn200r` rebuilt by the `stressGen` run, three pairs (off, on; on, on; on, off) | the coordinator asked for main's fixes and the generator's own build; arm order alternates and the A/A pair gives the noise |
| SG49 | The self edges, the slower `rescanNow`, the whole globals page, the deferred adopts and the first-tick stall reported, not fixed; no jar swapped for mic_climate | engine, bridge and district design for Ben (10.9); the coordinator asked for the stall to be noted only |

### 10.8 The station fixes, and syn40 again

10.6 items 1, 2 and 4 were decided (the coordinator, while Ben slept: these can be undone) and are
fixed or reported here. Item 3 is the budgeted rescan, DS-11, which reached main at 72fa63e and is
in the re-run's jars.

**The unload (10.6 item 1, option (a)).** The arrival's cargo is now read when the first Portable
Storage Interface docks at the station, not at `Train.arriveAt` (SG30, a mixin at the head of
`PortableStorageInterfaceBlockEntity.startTransferringTo`, PATCHES.md row 19a).

- No navigation hook fires earlier on every arrival. Create's approach is arithmetic inside
  `Navigation.tick` with no event, and a train already under the approach speed never takes the
  slowing branch. The dock is the earliest point that matters: an interface docks only within reach
  at 0.25 blocks per tick or less, which every stopping train passes through, and nothing moves
  between a train and a station before it.
- Where no dock names the station, the bridge falls back (SG31), and the arrival record's flags and
  `/rtsim district perf` say which path each arrival took:
  - a watched trip with no dock near the station: nothing can have moved, so the head of `arriveAt`
    is read (flag 2, counted as an approach);
  - a dock near the station that could not name it (the train had no destination): the cargo at the
    train's departure for this station, option (b) (flag 4, counted as a departure);
  - a train's first watched stop: the head of `arriveAt`, late (no flag).

**Simulated calls (10.6 item 2, options (a) and (b) together).**

- (a) Station ports (SG32, IND_STATION_PORTS 0x0415). At a loaded call's departure the bridge walks
  from each interface that docked through chutes, funnels, hoppers, pipes, pumps and valves (at most
  8 blocks, loaded chunks only, no belts, so it stops before the factories) and sends the district
  inventories it reached. On the generated ports that is the supply vault and the sink vault.
- A simulated call books only on its station's ports (SG33). A station with a rule and no ports
  moves nothing (`exchanges_portless` in the engine); it never falls back to the district's stock.
- (b) A simulated load is capped at the median of the station's last 8 loaded calls' amounts of the
  good (SG34): about 30 items here. Unloads are not capped: they put down what the carriage holds.
- On a black-box span the replay's change so far is placed in the ports before each call (SG35);
  otherwise a port's bookings from earlier calls grow while the replay waits for the reconcile.
- `rtsim-cli lines` shows each rule's ports, its recent per-call amounts and the load cap.

**The GameTest** (`stationcargo.saw_shafts`, in the create suite, SG37). It builds the glass corpus'
saw shafts factory far from the yard, with 256 copper and 128 gold added to its input chest, and a
port as the generator builds one: a supply vault, a smart chute, the interface, a smart chute and a
sink vault, with a station beside it. A synthetic train (no carriage entity, so the test records the
dock through `TrainWatch.dockedForTest`) makes one loaded call that moves 30 gold into the sink and
30 copper into the carriage, then leaves. The test checks the station's ports are exactly the two
vaults, unloads the district, brings the train back carrying 30 gold, waits for the simulated call's
`IND_CARRIAGE_RESULT`, and reloads. After the reload's reconcile:

- the factory's input chest holds the same copper and gold as before the unload (its alloy changed,
  574 to 549, by the factory's own replay);
- the supply vault fell by 30 copper (481 to 451) and the sink rose by 30 gold (30 to 60), each
  exactly the learned amount;
- the carriage holds 30 copper.

All 142 of the create suite's GameTests pass with it.

**The first tick (10.6 item 4).** One boot was profiled from its first tick (Java Flight Recorder,
`sgprof1`). The second density-function compile is not rtsim's own work, but rtsim reaches it:

- rtsim's habitat sampler asks mic_climate for a temperature (`HabitatSampler.sample` and `write`,
  `Climates.temperature`, `ClimatePlugin.temperature`).
- mic_climate asks Thermoo, whose Project Atmosphere source asks PA for the region's weather
  (`ProjectAtmosphereSource.celsius`, `AtmoApi.getCurrentWeather`).
- PA's forecast for a region it has not loaded builds a `BiomeSampler` with a fresh `RandomState`
  (`RegionForecastOrchestrator.ensureLoaded`, `ForecastGenerator.sampleSquare`), and C2ME compiles
  that state's density functions on the server thread.
- PA already compiles once in its own `onServerStarted`, on both arms. rtsim's first habitat sample,
  3 s later, pays the second: about 0.9 s of server-thread samples.

So the cost is Project Atmosphere's and C2ME's: any first temperature query in a new region pays it,
rtsim's or another mod's (SG36). Not fixed here. Recommended: mic_climate gives PA's `BiomeSampler`
the level's own `RandomState` (a mixin in mic_climate), so nothing is compiled twice.

**syn40 again (sg40d: on, on, off).** Jar set `sg-cce8b6a`: this branch after merging main 72fa63e
(DS-11), with `mic_native` from 7177451 (SG39). The create suite passed on the same tree first. The
one-minute load average over the busy windows was 3 to 14.

- **Exchange rules:** the unload is learned at all 128 stations on both rtsim arms, and the load at
  all 128; none is wrong. Each rule was observed 25 or 26 times.

  | Run | Arm | Stations | Load right | Unload learned | Unload missing |
  |---|---|---:|---:|---:|---:|
  | sg40d | on | 128 | 128 | 128 | 0 |
  | sg40d | on (A/A) | 128 | 128 | 128 | 0 |

- **Cargo source:** every real arrival's cargo was read at an interface dock: 3,020 approaches, no
  departure fallback and no late read, beside 500 simulated arrivals (3,511 docks in all). 128
  station port records at the first departures and 134 after the reload, where four districts were
  declared again and some of their stations sent their ports anew.
- **Line edges:** 128 of 128, 29 trips each, travel 129 to 131 ticks, and cargo 29 to 31 items per
  trip (mean 30), against 205 to 322 before: no full-carriage legs.
- **Factory stock no longer drops.** Against the off arm, each factory chest changed by its own
  machines' replay over the unloaded window, the same amount for every factory of a kind:
  - every saw shafts chest -42 andesite alloy (322 chests); every millstone chest -61 wheat (565);
  - every press chest -125 iron ingots (237) or -61 (27), with +116 to +118 iron sheets;
  - no chest lost more than its own machines used. In sg40b the same check found saw chests that
    had lost 1,437 alloy each, and 10.5's sg40a presses lost 576 iron ingots while making 1 sheet.
- **The ports took the simulated calls instead.** The supply vaults held 13,989 and 14,191 items
  fewer than the off arm's, and the sink vaults 14,981 and 15,201 more: about 30 items a call over
  the 500 simulated calls. Per good, 463 to 1,974 left the supply vaults and 482 to 2,066 reached
  the sinks.
- **Reload:** 384 reconciles applied on each rtsim arm, no short deltas.
- **Cost** (busy window; the A/A is the second rtsim arm minus the first):

  | Comparison | dMSPT | dCPU/tick | dServer thread/tick |
  |---|---:|---:|---:|
  | busy, on minus off (2) | +0.88, -0.25 | +4.0, +1.5 | +1.07, +0.38 |
  | busy, A/A | -1.13 | -2.5 | -0.69 |
  | reloaded, on minus off (2) | +0.42, -0.12 | +3.0, +2.2 | +0.28, -0.35 |
  | reloaded, A/A | -0.54 | -0.8 | -0.62 |

  MSPT differences stay inside the A/A's, as in 10.3. rtsim's own tick cost over the busy window
  had a mean of 0.60 and 0.61 ms (p99 2.3 ms, p99.9 4.9 and 5.4 ms), against 0.69 to 0.82 in sg40a
  and sg40b.
- **Detection (DS-11):** the periodic pass at game tick 12,001 now runs over 70 ticks (142 and 143
  ms in all). Its longest tick was 14 and 23 ms (256 and 310 block entities), and the busy window's
  worst rtsim tick 16 and 26 ms, against 326 to 755 ms in one tick before. The timed `rtsim district
  rescan`, which a command still runs in one call, took 80 and 81 ms (167 and 322 ms in sg40b).
  After the reload, the pass ran over 70 ticks (52 and 55 ms in all), and the reloaded window's
  worst rtsim tick was 38 and 42 ms.
- **The first tick** still holds the 1.65 s stall at game tick 1,293 on both rtsim arms (SG36).
- **syn200 was not re-run:** other agents' client benches and GameTests were queued on the bridge
  lock and the load average stayed between 8 and 14, so two 10 GB servers would have measured the
  laptop, not rtsim (re-run on main b5cd4d2 in 10.9).

**Does an adopt still spike (the coordinator's DS-11 question)?** Yes, and the spike comes from the
adopt command itself, not from the detection triggers DS-11 spreads over ticks.

- `IndustryCommands.adopt` calls `Districts.rescanNow`, which runs a full detection in the command's
  own tick with no budget (`Rescan.step(Integer.MAX_VALUE)`). A full detection takes every
  district's chunks as candidates, so an adopt costs more with each district already found. DS-11
  left this path unbudgeted on purpose, and `/rtsim district perf` leaves it out of its longest
  tick.
- Measured side by side (SG40): the scene's forceload and 80 adopts on syn40, one server before
  DS-11 and one after, reading `rtsim district perf` after each adopt.

  | Adopt's own detection | Before DS-11 | After DS-11 |
  |---|---:|---:|
  | First ten adopts (median) | 7.0 ms | 9.4 ms |
  | Last ten, at about 40 districts (median) | 46.1 ms | 40.7 ms |
  | Largest | 55.6 ms | 52.0 ms |
  | Sum (79 and 80 adopts read) | 2.15 s | 1.96 s |

  The cost climbs steadily with the district count on both, so DS-11 did not change it. Spark puts
  93% of the adopts' server-thread time in the walk (`Rescan.root` and `walk`) and 7% in the commit.
- sg40d's own adopt loop took 11.5 to 12.2 s (3.2 to 3.3 s in sg40b), and its settle window held 4.9
  to 5.1 s of detection (1.6 to 1.7 s). Those servers were slow at that moment, not the adopts: each
  command's round trip was about 150 ms from the very first adopt, when an adopt's own detection
  costs under 10 ms, and both rtsim servers logged running 12 s behind just after the loop. The
  side-by-side pair is the comparison to use.
- The ticked detection, the path DS-11 changed, had a longest tick of at most 14 and 23 ms over the
  whole of each sg40d rtsim run.
- Not changed here: `Districts.java` and `IndustryCommands.java` belong to the district work.
  Recommended: let an adopt detect only around its own chunks (a dirty-chunk pass, not a full one,
  as DS-11's ticked path already does), so its cost is one neighbourhood's at any district count.
  Alternatively, let the adopt only mark the chunks and leave the detection to the next ticked pass;
  then the command cannot report the new district count at once.
- **Fixed afterwards (DS-12):** the adopt now detects around its own chunks (`Districts.adoptAround`), budgeted
  like a ticked pass. `merge`, `split` and `exclude` still call `rescanNow`.

### 10.9 syn200 again, on main b5cd4d2

SG41 left syn200 unmeasured after the station fixes. This re-run uses main at b5cd4d2, which since
10.3 has gained DS-11 (the budgeted district rescan), DS-12 (an adopt detects around its own
chunks), 10.8's station cargo fixes (IND_STATION_PORTS, the dock snapshot, the load cap), G2-9 (the
glass state paged), dirty-page saves (b5cd4d2), EQUILIBRIUM S8 and the oil F6 nests.

In short:

- **Cost:** rtsim's whole cost is 1.6 to 1.7 ms a tick at 200 districts, about 1.3% of a 122 to
  133 ms tick. 10.3's MSPT cannot see most of it (it ends before the Post handlers); a new per-tick
  measure can, and its on minus off difference matches rtsim's own tick cost to 0.03 ms. MSPT
  differences on minus off (+0.6 to +2.5 ms) stay inside the A/A control's (+3.3).
- **Detection:** the periodic pass now runs over 363 ticks, and the busy window's worst rtsim tick
  is 34 to 51 ms, against 201 to 230 ms. Adopts no longer spike. The one-call `rtsim district
  rescan` got slower: 0.49 to 1.09 s, against 0.23 to 0.45 s.
- **Station learning:** load and unload learned at all 720 stations on every rtsim arm (0 to 33
  unloads before), none wrong. New and wrong: 21 to 31 one-trip edges from a station to itself on
  three of the four rtsim arms, from DS-12's deferred adopts.
- **Factory stock:** factory chests change only by their own machines' replay; the simulated calls
  took from the ports' vaults.
- **Saves:** 3.4 to 3.7 MB a save while every district runs, 2.2 to 2.4 MB of it the glass pages (new
  with G2-9); a save seconds later writes 0.95 to 1.5 MB. The globals page is written whole every
  save and grows with the line legs.
- **First tick:** the 1.9 to 3.5 s stall is still there (mic_climate 0.2.0, SG36), with a second, 2.0
  to 5.7 s, at the first forceload.

#### Method

- **Jars** (SG48): set `sg-b5cd4d2`, every rtsim jar built from main b5cd4d2; `mic_native` is the jar of
  every earlier set (7177451). The pack ships mic_climate 0.2.0, so the first-tick stall was
  expected; no jar was swapped.
- **World:** `syn200r`, rebuilt by the `stressGen` run on this tree (seed 2, 200 x 32, 360 lines
  verified, 1,470 game ticks, 85 s in the test). Its layout is syn200's (districts, factories,
  station pairs, goods); its trains' UUIDs and build ticks differ.
- **Pairs** (10 GB heaps, side by side, `free -g` checked before each): syn200r-a (off, on),
  syn200r-b (on, on: the A/A control) and syn200r-c (on, off). Two single-arm trials of the
  harness changes (t1, t2) are not used.
- **Scene** (`synthetic-syn200r-ticks.json`): 10.2's, with its phases ended by game ticks (SG43):
  settle 1,200 (the adopts at its middle), busy 6,600, unload wait 1,800, unloaded 600, reload wait
  900, reloaded 600. syn200 runs at 7 to 8 TPS here, where a 600 s busy window would hold about
  4,600 ticks and could miss the periodic pass at game tick 12,000; every busy window here held it.
  A phase ends once both arms have run its ticks, so the faster arm runs a few more (6,609 to 6,781
  in busy).
- **Per tick** (SG44): a KubeJS script on every arm (not part of the pack, like the hippocampus
  workaround) records two numbers each tick, read with `swticks dump`:
  - "ring", the server's own tick time (`tickTimesNanos`, what `neoforge tps` and so 10.3's MSPT
    average);
  - "gap", the wall time from one tick's end to the next, taken after every `ServerTickEvent.Post`
    handler: the whole tick.
- **Saves** (SG45): each rtsim arm's page segments are copied as they appear (compaction deletes
  them) and decoded by table, and the engine's `rtsim.snapshot` (step thread) and `rtsim.save` (IO
  lane) zones are read from `/micperf zones` after the final save.
- **Final report** (SG46): `/rtsim district perf` once more at the end of the reloaded window, so the
  reconcile count covers the whole reload.
- **Load and suspends:** the one-minute load average over the busy windows was 12 (pair a), 6 (b)
  and 9 (c). The laptop suspended twice when its lid closed (the sleep inhibitor does not cover the
  lid): one 238 s tick in pair a's busy window and one 1,726 s tick in pair b's reloaded window, on
  both arms. Ticks with a gap over 10 s are dropped (SG47). Pair a's reloaded window ran at a load
  of 30 to 32 and 3.7 TPS and is not compared (SG47).

#### What 10.3's MSPT measured

The tick ring is written before `ServerTickEvent.Post` fires, so it leaves out every Post handler,
where most of rtsim's tick work runs. The gap holds them. On each on-off pair the rtsim arm's gap
minus ring exceeds the off arm's by rtsim's own tick cost (`TickCost`), and on the A/A pair the two
are equal:

| Pair | Gap minus ring, rtsim arm | Gap minus ring, other arm | Difference | `TickCost` mean |
|---|---:|---:|---:|---:|
| syn200r-a (off, on) | 1.89 | 0.28 | +1.61 | 1.58 |
| syn200r-c (on, off) | 1.94 | 0.27 | +1.67 | 1.66 |
| syn200r-b (A/A) | 1.83 | 1.83 | 0.00 | 1.53, 1.53 |

So the MSPT columns of 10.3 and 10.8 never held rtsim's Post handlers. Their server-thread CPU and
`TickCost` columns did, and 10.3's reading of rtsim's cost rests on those. From here the gap is the
whole tick.

#### Numbers

Busy window (at least 6,600 game ticks, all districts loaded, every train running). Means in ms,
p95 and p99 in brackets.

| Run | Arm | Ticks | Ring (p95, p99) | Gap (p95, p99) | CPU/tick | Server thread/tick | TPS |
|---|---|---:|---:|---:|---:|---:|---:|
| syn200r-a | off | 6,781 | 122.13 (195.6, 281.4) | 122.41 (196, 282) | 158.1 | 130.54 | 8.17 |
| syn200r-a | on | 6,621 | 122.97 (199.2, 284.1) | 124.86 (202, 289) | 162.6 | 132.61 | 8.01 |
| syn200r-b | on | 6,779 | 129.76 (166.1, 250.0) | 131.59 (169, 252) | 160.2 | 134.77 | 7.60 |
| syn200r-b | on (A/A) | 6,609 | 133.07 (171.8, 237.0) | 134.90 (174, 238) | 172.9 | 137.97 | 7.41 |
| syn200r-c | on | 6,615 | 129.73 (195.7, 259.4) | 131.67 (199, 263) | 193.4 | 142.64 | 7.59 |
| syn200r-c | off | 6,739 | 129.10 (196.0, 255.4) | 129.37 (196, 256) | 176.0 | 140.40 | 7.73 |

| Comparison | dRing | dGap | dCPU/tick | dServer thread/tick |
|---|---:|---:|---:|---:|
| busy, syn200r-a, on minus off | +0.84 | +2.45 | +4.5 | +2.07 |
| busy, syn200r-c, on minus off | +0.63 | +2.30 | +17.4 | +2.24 |
| busy, syn200r-b, A/A | +3.31 | +3.31 | +12.7 | +3.20 |
| busy, sg200d, on minus off (10.3; MSPT is the ring) | +0.04 | | +3.9 | +1.23 |
| busy, sg200c, A/A (10.3) | -1.57 | | -0.4 | -1.35 |
| reloaded, syn200r-c, on minus off (859 reconciles) | +1.45 | +3.23 | +15.7 | +3.52 |
| reloaded, syn200r-b, A/A | +2.40 | +2.38 | +9.8 | +3.03 |
| reloaded, syn200r-a (load 30 to 32, 3.7 TPS; not compared) | +3.32 | +5.36 | +5.0 | +4.15 |

- rtsim costs about 1.6 to 1.7 ms of a 122 to 133 ms tick. Every on minus off difference sits
  inside the A/A pair's, whose two identical arms ran 3.3 ms apart on both measures.
- Whole-process CPU per tick (GC, C2ME and the engine's threads as well) moved 4 to 17 ms between
  arms, A/A included, and is not read as rtsim's.
- Every arm ran slower than sg200d (122 to 133 against 102 MSPT), off arms included: the laptop's
  load and this copy of the world, not rtsim.

**rtsim's own tick cost** (busy window, `TickCost`, ms):

| Run | Arm | Mean | p50 | p99 | p99.9 | Worst (game tick) |
|---|---|---:|---:|---:|---:|---:|
| syn200r-a | on | 1.58 | 1.33 | 6.20 | 9.84 | 50.8 (7,919) |
| syn200r-b | on | 1.53 | 1.29 | 7.18 | 12.18 | 33.5 (6,544) |
| syn200r-b | on (A/A) | 1.53 | 1.28 | 7.28 | 14.05 | 45.9 (10,366) |
| syn200r-c | on | 1.66 | 1.42 | 6.02 | 11.87 | 33.8 (7,662) |

The mean stays where 10.3 had it (1.23 to 1.59 ms). The worst tick falls from 201 to 230 ms (the
periodic pass in one tick, game tick 12,001) to 34 to 51 ms.

**Spark** (60 s profiles just after the busy window, server thread, the four rtsim arms; 10.3's
syn200 figures in brackets):

| Share of the server thread | syn200r |
|---|---:|
| rtsim in all | 0.76% to 1.01% (0.80% to 1.35%) |
| inventory sweeps | 0.22% to 0.28% (0.26% to 0.40%) |
| the glass box | 0.24% to 0.29% (0.24% to 0.35%) |
| district upkeep (`Districts`) | 0.17% to 0.27% (0.19% to 0.55%) |
| the engine hand-over | 0.03% to 0.20% (0.01% to 0.2%) |
| `TrainWatch` | 0.02% to 0.04% |
| Create's trains, every arm | 4.2% to 5.2% (5.2% to 5.4%) |

**Longest ticks.** On every arm the busy window's longest tick is the vanilla autosave at game time
7,507: 1.31 to 2.63 s, with or without rtsim (1.31 s on both arms of pair a; 1.65 s on pair c's rtsim
arm against 1.93 s off). Other busy ticks reach 0.5 to 0.97 s on both arms alike, and their ring
time equals their gap, so none of them is in a Post handler.

**Engine hand-over and heap.** The server waited on the engine on 0 to 7 ticks through the busy
window and on 25 to 71 ticks by the end of the unloaded window (78 by the end of pair c's reloaded
window). Live heap after the run was 2.07 to 2.13 GB on every rtsim arm (RSS 4.8 to 6.0 GB), as
before (SG18).

#### Detection

| Detection | sg200 (10.4, one tick) | syn200r (DS-11, DS-12) |
|---|---|---|
| The periodic pass (game tick 12,000) | 201 to 230 ms in one tick | 296 to 413 ms in all, over 362 to 363 ticks |
| The pass after the reload | 234 to 253 ms in one tick | 343 to 658 ms over 364 to 369 ticks |
| Longest detection tick over a whole run | the passes above | 14 to 63 ms; once 150 ms (pair a, load 67) |
| The busy window's worst rtsim tick | 201 to 230 ms | 34 to 51 ms |
| The scene's 400 adopts | 41 to 59 s; each of the last adopts 226 to 374 ms in its tick | 42 to 52 s; each adopt seeds its block entities and the ticked passes detect |
| The timed `rtsim district rescan` (one call, no budget) | 232, 246, 360 and 448 ms | 493, 789, 812 and 1,088 ms |

- The ticked passes did what DS-11 promised at 200 districts: a pass costs about what it did, spread
  over 363 ticks at 256 block entities a tick.
- The longest detection ticks are now commits (declaring districts), not walks: pair a's 63 ms held
  58 ms of commit, pair c's 23 ms was all commit, and the one 150 ms tick was a commit at a load of
  67.
- The adopt loop takes 42 to 52 s because each RCON command waits for a tick at 7 to 8 TPS: it
  measures the tick rate, not the adopts.
- The one-call rescan is two to three times slower than sg200's, even on the least loaded arm (pair
  c, 493 ms at a load of 9). `merge`, `split` and `exclude` still call `rescanNow`, so they pay it
  in one tick. It was not profiled here.

#### Station learning

- **Exchange rules** (`rtsim-cli lines`, checked against `stressgen.json`): the load and the unload
  are learned at all 720 stations on all four rtsim arms; none is wrong. sg200 learned 0 to 33
  unloads (10.5).
- **Cargo source:** every real arrival's cargo was read at an interface dock (7,587 by the end of
  pair a's busy window), with no departure fallback and no late read, beside 2,067 to 3,012
  simulated arrivals.
- **Line edges:** all 720 real edges on every arm, 16 to 19 trips each, travel 129 to 131 ticks, the
  line's good right on every edge.
- **New and wrong: edges from a station to itself.** 25 (pair a), 31 and 21 (pair b), none (pair c);
  none in sg200 or sg40d. Every one is an east station's, with one trip, 30 items of that station's
  load good, a travel time of 461 to 465 ticks (one round trip: two runs and two dwells) and its
  arrival at game time 6,129 to 6,179.
  - DS-12's `Districts.adoptAround` does not detect while a detection job runs in the dimension: it
    marks the chunks dirty and returns ("the next pass has them"). The next pass starts only once no
    job or pass is left, after `rescanIntervalTicks` (20), and works at 256 block entities a tick
    (`rescanBlockEntitiesPerTick`) against about 285 in a district. So while the adopts keep a job
    running, the deferred chunks wait. At the end of the loop 110, 186, 175 and 200 districts were
    declared, and 315 to 358 of the 400 adopts had been deferred.
  - The adopts run west to east. A train that left a declared district's east station at game time
    5,664 to 5,718 reached its partner's west station before that district was declared.
    `TrainWatch.arrived` returns when no district holds the station, so no stop is sent, and
    `departing` then finds none either.
  - The engine's `last_departure` for the train still names the first station, so the next arrival
    there makes an edge from the station to itself.
  - Pair c's districts were all declared by the end of its loop, and it has none.
  - Each is a wrong edge with one phantom trip; no simulated leg used them. Any train that calls at
    a station outside every district and comes back, out and back, makes the same edge: a base
    adopted while its trains run, or a player's undeclared station on a route.

#### Factory stock and reconciles

- Against the off arm, each factory chest changed only by its own machines' replay over the
  unloaded window. In pair c's 88 replayed districts: presses -50 to -133 iron ingots with +99 to
  +123 sheets, millstones -38 to -65 wheat, saws -20 to -43 andesite alloy. No chest lost more than
  its machines used (worst -133; pair a's -141).
- The simulated calls took from the ports: the replayed districts' supply vaults fell 180 to 498
  items each (34,082 in all in pair c, 3,268 in pair a) and their sink vaults rose (27,734 and
  1,157).
- The other districts' chests moved only by the arms' different tick counts (worst chest -57 to -59).
- Reconciles applied: 78 (pair a, 8 districts), 938 and 577 (pair b), 859 (pair c, 88 districts),
  against sg200d's 107. Only a district whose glass box had a verdict before the unload replays
  and reconciles. The others have no sim span, so they skip simulated exchanges and stay frozen, as
  in vanilla, with nothing to reconcile. The verdict window (6,000 ticks) closed near the end of the
  busy window, so the count depends on when each arm's districts were declared; `rtsim district
  info` sampled 0 to 5 of 12 with a verdict.

#### Saves (dirty-page saves)

| Save | Industry pages written | Segment |
|---|---|---:|
| First autosave (game time 7,507) | 200 district, 200 glass, the globals (91 KB raw, pair c) | 1.76 to 2.36 MB |
| Second autosave (13,507) | 200, 200, the globals (383 KB raw, pair c) | 3.10 to 3.22 MB |
| `save-all flush` after the reloaded window | 200 district (766 to 782 KB stored), 200 glass (2.2 to 2.4 MB stored, 11.5 to 11.7 MB raw), the globals (616 to 640 KB raw, 428 to 445 KB stored) | 3.43 to 3.65 MB |
| The stop's save, seconds later | 94 to 195 district, 12 to 29 glass, the globals | 0.95 to 1.54 MB |

- While every district is loaded and running, every district and glass page changes between saves,
  so dirty-page saves write them all. They pay off on a save soon after another: the stop's save
  wrote half or less.
- The glass pages are new with G2-9 and are two thirds of a full save. They compress about 5:1.
- The globals page (0x4900) is written whole on every save, and it grows with the line legs it holds
  (about 45 to 50 bytes raw a departure; legs expire after a game day): 91, 383 and then 636 KB raw.
  At this world's departure rate it would reach about 1.2 MB raw once a day of legs is held.
- Time: the final save's snapshot took 55 to 113 ms on the engine's step thread and its write 66 to
  107 ms on the IO lane; over each arm's three saves the largest were 143 and 271 ms. The segment
  appeared within about a second after `save-all flush` returned. The flush took 1.44 to 3.80 s on
  rtsim arms and 2.11 to 3.57 s on off arms, and the autosave tick shows no rtsim difference.
- The rtsim directory is 3.48 to 3.68 MB, against 1.47 to 1.53 MB in sg200 (whose industry was in
  SavedData then).

#### The first tick and the first forceload

- The first tick (game time 1,508) took 1.85, 3.49, 3.33 and 2.71 s on the rtsim arms. Its ring time
  was 13 to 35 ms, so the stall is in a Post handler: SG36's Project Atmosphere forecast, reached
  through rtsim's habitat sampler and mic_climate 0.2.0.
- The first forceload's tick took 1.98, 5.68, 4.85 and 3.50 s on the rtsim arms only. rtsim arms
  log three large density-function compiles (1.4 to 4.2 s each), off arms one, as sg200d did: the
  same cause, as the habitat sampler meets new regions. Not new.

#### Findings and For Ben

Recommended option first in each; none is fixed here (SG49).

1. **Edges from a station to itself.** (a) Recommended: the bridge sends an "unwatched stop" when a
   watched train calls at a station outside every district, so the engine ends the leg there and
   starts no edge. (b) The engine drops an edge from a station to itself. (c) Leave them: one
   phantom trip each, and nothing simulated used them here.
2. **The one-call `rescanNow` is two to three times slower at 200 districts** (0.49 to 1.09 s).
   (a) Recommended: budget `merge`, `split` and `exclude` as DS-12 budgeted the adopt, and profile
   one call to find what grew. (b) Profile only.
3. **The globals page is written whole on every save and grows with the legs** (0.6 MB raw here,
   about 1.2 MB at a day of legs). (a) Recommended: page the legs apart from the globals, by cell or
   by train, so a save writes only the changed ones. (b) Split the page into a few fixed parts.
   (c) Leave it: 0.4 MB stored a save at 200 districts.
4. **Deferred adopts wait for the running job** (behind item 1). (a) Recommended: let a deferred
   adopt's chunks join the running job, or start a pass at once after it. (b) Leave it, if item 1
   (a) lands: the deferral then costs only time.
5. **The scene's reconcile count depends on glass verdict timing.** (a) Recommended: lengthen the
   busy window or delay the unload (about 8,000 ticks after the adopts), so every district's verdict
   closes before the unload. (b) Leave it and report the count per run, as here.
6. **The first-tick and first-forceload stalls** (SG36). (a) Recommended: the mic_climate fix of
   10.8 (PA's `BiomeSampler` given the level's own `RandomState`). (b) Leave it: once per region.

### 10.10 Fixes for 10.9's findings

The coordinator relayed Ben's decisions on 10.9's findings: fix them, in this order, each on main
and pushed alone, each after an adversarial review. Choices are numbered from SG50, with gaps for
the fixes that follow.

#### Self edges (finding 1)

A train that left a district's station, called at a station outside every district and came back
made an edge from the first station to itself, carrying the round trip's travel time. The bridge
now says so when a watched train calls outside every district, and the engine ends the leg there.

| # | Choice | Why |
|---|---|---|
| SG50 | New record IND_TRAIN_UNWATCHED (0x0416, train and station), sent by `TrainWatch.arrived` when the station is in no district, once a leg; the engine forgets the train's open leg | finding 1 (a); the leg between a district's station and an unwatched one is not a line the engine can use, and the next departure from a district starts a new leg. A train's second unwatched call sends nothing; a departure from a district re-arms it |
| SG51 | The engine also refuses to book an edge whose ends are one station, and counts it (`self_edges_refused`) | defense in depth: an engine fed by an older bridge, or a replayed journal from before the record, must not learn a line from a station to itself. A GameTest on the native engine asserts the count stays 0 with the bridge fix, so the guard cannot hide a bridge regression |
| SG52 | A departure the report flushes before the train's destination is known is journaled with no destination and an ETA of 0, stamped at the unwatched call; reported, not fixed | it needs a train that left and called at an unwatched station with no destination set at any tick between (Create sets one at once); the threat leg it books is one tick long |
| SG53 | A station with no position or dimension is reported as unwatched too | such a station is in no district, so the leg is over |
| SG54 | The GameTests read the engine's counts as changes over the journey and look for edges by the journey's own station ids; the report switch is reset by `forget` | the engine's counts run across the whole suite, and a failed control must not leave the switch off for the rest of the run |


#### The globals page (finding 3)

The booked train legs were most of the globals page (about 45 to 50 bytes raw a departure, held a game
day: 91, 383, 636 KB raw), which every save rewrote whole. They are now a table of their own
(`persist::LEG_TABLE` 0x4904, ID10 in docs/choices-industry-dirty-pages.md), so a save writes only the
leg pages a booking, a loot or an expiry changed. A format change with no migration, as before release.

| # | Choice | Why |
|---|---|---|
| SG70 | Legs are paged by id, 256 to a page (`LEGS_PER_PAGE`), the page of a leg being its id divided by 256; each leg is stored with its dimension and the regions its line crosses | finding 3 (a), the recommended option, paged "by cell or by train" in the brief. Paging by id instead: legs are booked in id order and expire oldest first, so the pages that change are the newest (being filled) and the oldest (thinned by the daily expiry), and the rest are left alone. A cell or a train would put a leg in several pages, and a booking would write all of them |
| SG71 | The cell lookup (`by_cell`) and the train lookup (`by_train`) are not saved: a restore rebuilds them from the legs in id order, through the insert a booking uses. The next leg id stays in the globals page | the lookups are a function of the legs, so one copy cannot disagree with the other (a unit test asserts a restored index equals the booked one, a seam line included). The counter outlives the legs it numbered, so a restore after an expiry does not reuse an id (tested) |
| SG72 | A restore refuses a leg page that holds an id of another page, ids out of order, an id at or past the counter, or another magic (`ILEG`); a page with no leg is deleted, never written | a save writes and deletes a page at its own key only, as for a district page (ID8); an id past the counter would be reused by the next booking |
| SG73 | The viewer's loader (`rtsim-view` `IndustryView::load`) reads the leg pages with the globals; the district inspector still restores from the globals, district and cluster pages alone, which restores the legs as none | the map draws the legs, and a district card does not need them |

Measured, a leg of four carriages carrying two goods each: about 77 to 80 bytes raw a leg in the page
(the cells included), so a page of 256 legs is about 20 KB. At the 10.9 arms' 636 KB raw of legs, a save
that books a few trains writes the page being filled, about 20 KB, where it wrote the 636 KB. The
quiet save (no booking) writes the globals page without the legs. Not measured on syn200: the full
pair is not run (the brief); the counts above are the engine's, from the unit tests' pages.
