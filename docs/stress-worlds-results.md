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
- **Detection** is the largest single tick: 0.17 to 0.75 s for a full pass, and up to 0.37 s for
  one adopt at 200 districts (10.4).
- **Station learning:** every line edge is learned, with exact trips and travel times. Exchange
  rules get the load good right everywhere but miss the unload, a snapshot timing problem, and
  simulated calls take their cargo from the factories' input chests (10.5). Both are for Ben
  (10.6).

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

1. **Exchange rules miss the unload** (10.5): the dock starts 4 to 6 ticks before
   `Train.arriveAt`, so the arrival snapshot comes after the unload. (a) Recommended: take the
   arrival snapshot on the final approach, once the train is braking for the station and before a
   PSI can dock (a hook in Create's navigation). (b) Diff each departure against the train's
   previous departure: no new hook, but a stop at a station outside any district would be booked to
   the next district station.
2. **Simulated calls take from the factories and fill the carriage** (10.5). (a) Recommended: give
   each station its port inventories (the containers its PSIs reach) and book simulated exchanges
   on those. (b) Cap a simulated load at the edge's learned cargo per trip. (c) Keep the
   district-wide booking but prefer containers that feed no machine. (b) is the smallest change
   and fixes the 1,664-item legs; only (a) stops the reconcile starving factories.
3. **The detection tick on many districts** (10.4): a full pass of 0.2 to 0.45 s at 200 districts,
   and adopts that grow with the district count. (a) Recommended: land the budgeted rescan and
   re-measure on syn200. (b) Also limit an adopt's work to its own neighbourhood.
4. **A 1.2 to 3.6 s tick at the start of every rtsim session:** it coincides with a C2ME
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

### 10.7 Reversible decisions taken (SG1 to SG24)

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
