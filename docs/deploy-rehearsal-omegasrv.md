# Deploy rehearsal: mic-rtsim on a copy of OmegaSrv

Rehearsal of DECISIONS.md SRV1: copy Ben's live MIC server (OmegaSrv) read-only, prune the world to what
players built, boot it locally on the old pack, the current pack, and the current pack with mic-rtsim, and
write down what a real deploy needs. Run 2026-10-20 and 21 on the laptop against a snapshot taken
2026-10-20.

Private data stays private: the world copies, logs and player files live under `~/mic-omegasrv/` on the
laptop and are not in this repository. The tools are in `tools/deploy-rehearsal/` (stdlib Python, no
private data in them; the base coordinates and the seed are read from private files).

> Note (2026-10-21): DECISIONS.md C8a replaced the `doTraderSpawning` gamerule switch with a veto on
> vanilla's trader spawner. Everywhere below, `doTraderSpawning` findings, log lines and rollback steps describe
> the build rehearsed, not today's: the bridge no longer writes the gamerule.

## 0. Findings, by deploy risk

What booted: the July pack (the control), the upgraded pack (dc49fe7), the upgraded pack with rtsim
(main at 689d311, the core and all eleven plugins the pack can load), the rtsim world booted again, the
upgraded pack after rtsim was removed, and a sim reset, twice: with main's jars and with this branch's
reset fix (4.), then that world booted again. Every one of them booted, ran its scenario and stopped
with exit 0, once the Ice and Fire crash (1.) was kept out of the upgraded pack.

1. **Blocker, the pack upgrade: Ice and Fire 2.1-beta.1 crashes the server within minutes** (6.1). A
   hippocampus with no water target throws a NullPointerException in `HippocampusExplorationGoal.tick`,
   and the world has 677 hippocampi. Fixed upstream on 2026-08-18, in no release yet. DR1.
2. **High, the host: its JVM arguments.** `-Xms8G -Xms12G` is a fixed 12 GiB heap on a 16 GB host that
   also runs Elasticsearch and already swaps, and the server dies to the 60 s watchdog about weekly
   (section 1). The live set measured here is 2.3 to 2.5 GiB with or without rtsim, so `-Xmx8G` is
   ample (6.4, 8.5). DR2, DR3.
3. **High, rtsim, fixed in this branch: the glass box could not compile the main base** (6.5, 6.8). An
   upward chute threw, so the main base ran on the black box alone; the fix's GameTest found a second
   bug (a chute's direction was not in the fingerprint). Deploy jars built at or after this branch's
   merge.
4. **High for SRV2, rtsim, fixed in this branch: a sim reset left the engine without the districts,
   villages and base marks** (6.7). Deleting `world/rtsim/` keeps `world/data/mic_rtsim_*.dat`, where
   Java remembers what it has told the engine, so nobody told the fresh engine again: the glass box
   discarded every sweep (`revision`, the engine's district at revision 0) and no village was in the
   engine. The bridge now sees the gap at the session start and announces everything again: with
   the fix, the reset logged `55 districts are announced again` and `2 villages adopted again`, the
   engine counted both villages, and the main base's sweeps were sampled. What a reset still loses is
   what only the engine had: the sim's economy (6.7). For a page format change, reset the whole sim
   (8.6): the bridge cannot see one module starting fresh on its own (6.9).
5. **Medium, rtsim, not fixed: the main base's glass solve takes seconds and never converges** (6.7).
   Once the glass box worked (the fix of 3.), each solve of the main base's graph ran 278 rounds, its
   cap, in 2.4 s at a load of 13 and 5.4 s at a load of 82, without converging. It runs inside the
   engine's step: the server thread does not wait, but the engine skipped 2 to 11 % of its hand-overs,
   and a stop that came during a solve could not save the engine's pages (the journal replays them at
   the next start). The glass box is a read-only checksum today, so switching it off on the host loses
   only its diagnostics. DR9.
6. **Medium, the pack: four bugs the upgrade brings** (6.2). `create_power_loader.js` does not load
   (its recipes are missing), `radar_integration.js` registers 2 recipes short, mic_petrochem 1.0.0's
   two thruster fuels do not parse (1.0.1 fixes them), and Sable's physics tag names a block the pack
   lacks. The world is not hurt. DR4.
7. **Medium, rtsim: removing it leaves wandering traders off.** rtsim sets `doTraderSpawning` false
   while it owns traders and only gives it back while it is installed; the rollback (8.6) gives it
   back first. Otherwise the uninstall is clean (6.6): 243 one-off attachment errors as the chunks
   that carry rtsim data load, no rtsim id left in the world after its save, no block entity lost, and
   nothing left on disk that anything reads.
8. **Medium, rtsim: what the sim sees.** `district adopt 16` at the two factories made 55 districts:
   the main base (166 chunks, 278 inventories) and 54 one to four chunk districts of stray player
   blocks (6.5). The main base is too wide for the threat survey's perimeter cut (DR8). DR6.
9. **Low, the pack upgrade loses nothing on this world** (6.3, 7). No id the world uses leaves the
   pack, the same 18,191 block entities before and after in every run, no chunk lost, no train, no
   Destroy mixture at risk; what the new pack drops (Ice and Fire's portal attachments, Cobblemon's old
   fossil starts) it drops by itself, harmlessly.
10. **Low, rtsim's cost is below what the laptop resolves** (6.4). The mean tick moved +1.7 ms and
   -2.9 ms with rtsim in two side by side pairs, against a standard deviation of the paired difference
   of 38 to 41 ms; boot, stop, memory and the live set are the same. rtsim's own counters put its server
   thread time near 1 ms a second.
11. **Save, stop and reload work** (6.5, 6.7). The engine came back from its pages with every
    journaled record, twice: after r1-rtsim (main's jars) and after the fixed reset, where the main
    base's glass box carried on with its sweeps sampled.
12. **Not checked**: a player login (inventories, ender chests, Cobblemon parties, a player who logged
    out aboard a Sable ship), puppets (rtsim embodies agents only near players), the host's own load (4
    cores; the laptop's numbers are relative only), a run long enough for autosaves at 20 TPS, the
    mixin on Diesel Generators' oil data (applied lazily, at the first oil query), and the start after
    a stop whose engine save failed (6.7: the journal replays it, by design). The first login after each step is the check (8.3).

## 1. The live server today (read-only baseline)

| Fact | Value |
|---|---|
| Host | NixOS 26.05, 4 CPUs (i5-7640X, Kaby Lake X, AVX2), 16 GB RAM, 32 GB swap, glibc 2.42 |
| Service | systemd `minecraft-server`: `screen -DmS minecraft bash /var/lib/mc/server/run.sh`, `Restart=always` |
| Pack | the July pack: NeoForge 21.1.230, 164 jars (plus `.disabled` leftovers, a `wget-log`, and an `l2complements-3.1.3.jar?mr_download_reason=...` duplicate that NeoForge ignores) |
| JVM args | `user_jvm_args.txt` is `-Xms8G` and `-Xms12G` and no `-Xmx`: the JVM runs a fixed 12 GiB heap |
| Memory now | Minecraft's java RSS 11.1 GB with 0.66 GB of it swapped out; Elasticsearch on the same host 0.38 GB RSS and 0.57 GB swapped; `Committed_AS` 24.6 GB on 16 GB RAM |
| Players | one regular (one join in each of the last three logs), building at the main base |
| Boot | 109 s from ModLauncher to `Done (28.771s)` (2026-10-19 20:31 to 20:33) |
| Crashes | 49 crash reports since May; every one since June 27 (14) is the `ServerHangWatchdog` (a tick over 60 s), about weekly, each with the one player at the main base and with a different top frame (String.indexOf, MethodHandleNatives.resolve, a netty allocator, fastutil, Create's SmartBlockEntityTicker, c2me, Cobblemon molang) |
| Log noise, 2.5 days | 508 KubeJS `sleepvote.js` TypeErrors (`PlayerEvents.tick`), 208 `Can't keep up` (median 8.3 s behind, max 12 s), 148 Yigd `Tried to load invalid item` (empty grave slots), about 19,000 Project Atmosphere `No temperature range defined for biome terralith:*` warnings, 4 `unknown or non-serializable data attachment eclipticseasons:biome_holder` |

The watchdog crashes look like memory, not one bad mod: a different top frame each time, a heap
committed to 12 GiB on a 16 GB host that also runs Elasticsearch, and part of the heap in swap. A
G1 collection that has to page heap back in can stall the server thread for tens of seconds. The JVM
argument fix in the runbook (section 8.5) is worth doing whether or not rtsim goes in.

The 3,199 chunks that still carry `eclipticseasons:biome_holder` are the pattern any removed mod with
chunk attachments leaves behind: an ERROR line each time such a chunk loads, until the chunk is
saved again. rtsim's uninstall leaves the same kind of trace (section 6.6).

## 2. The copy and what it is missing

The snapshot was rsynced read-only (`--bwlimit=10m`, `nice -n 19 ionice -c3`) to
`~/mic-omegasrv/snap-2026-10-21/` (2.5 GB, world 1.6 GB). About 264 files are owner-only on the host and
could not be read by the SSH account:

- `world/level.dat` and `level.dat_old`;
- `world/playerdata/*` (15 files: inventories, positions, ender chests);
- `world/data/*.dat`, all of them: Create's `create_tracks.dat` (trains) and `create_logistics.dat`,
  Sable's `sable_sub_level_occupancy.dat`, `sable_tracking_points.dat` and
  `sable_sub_level_force_load_tickets.dat`, `aeronautics_unloaded_balloons.dat`, `seasons.dat`,
  `eclipticseasons.dat`, `scoreboard.dat`, `raids.dat`, maps, the Lootr per-player loot (181 files),
  and the rest;
- some files under `DIM-1`, `DIM1`, `dimensions/` and `config/spark`.

Ben made a tarball of exactly those 264 files as root (`/tmp/mic-private.tgz` on the host, 0.8 MB,
2026-10-21 05:51 server time), which was fetched read-only and extracted over the snapshot. One more
rsync pass right after it changed only 9 files (four entity region files, the Distant Horizons database,
`session.lock`, `kubejs_persistent_data.nbt`): nobody had played since 01:11, so the region files and
the data files describe the same moment. The measured runs (sections 5 and 6) use the real `level.dat`
and data files. An earlier attempt on a synthesized `level.dat` is not reported.

What the real files say:

- the seed matches the one recovered from the chunks; game time is 173.7 million ticks (100 days of
  play), `doTraderSpawning` is true, `naturalRegeneration` false, `doImmediateRespawn` true;
  datapacks `create:dynamic_data`, `tacz_resources`, `tectonic`, `create_dragons_plus:runtime` and the
  defaults;
- **there are no trains**: the host has no `create_tracks.dat`, and the world has no carriage
  contraption entity, so the upgrade cannot break a train on this world;
- Sable: 165 sub-level files, no force-load tickets, tracking points for players who logged out
  aboard a sub-level (they reappear aboard it at login), one unloaded Aeronautics balloon;
- Create's logistics network (one, nine links) and the player data (8 players, inventories and ender
  chests) are now in the copy, but players only load when they join, which nobody did in the
  rehearsal: inventories and Cobblemon parties stay a check for the first real login (runbook 8.3).

## 3. The prune

`prune.py scan` reads every chunk's InhabitedTime, block entities, structure starts and attachments,
and every entity chunk's entity ids; `prune.py plan` keeps a chunk when any of these holds, then adds a
4-chunk ring around the kept set:

- InhabitedTime of at least 24,000 ticks (20 minutes of a player nearby);
- a block entity from a tech namespace (Create and its addons, Destroy, PowerGrid, Diesel Generators,
  Vintage, Big Cannons, Aeronautics, Sable, CC: Tweaked, TaCZ, Overgeared and so on; Cobblemon berries
  excluded, they are everywhere);
- a Create contraption entity or a Sable, Aeronautics or Simulated entity;
- a village start (vanilla or Ribbits) and every chunk its pieces cover, so late village adoption has
  whole villages to adopt.

`prune.py write` copies the world with only the kept chunks in `region/`, `entities/` and `poi/`, and
everything else as is. The unpruned copy is kept beside it.

| Dimension | Chunks before | Kept | Of which core / ring | Region+entities+poi before |
|---|---|---|---|---|
| overworld | 136,702 | 17,425 | 8,454 / 9,150 (inhabited 3,987, village pieces 4,585, tech 168, contraptions 21) | 1,275 MB |
| the_nether | 6,484 | 354 | 55 / 299 (inhabited) | 33 MB |
| world total | | | | 1.6 GB to 189 MB |

Pruned chunks are regenerated from the seed when something loads them, which is why the seed matters.

## 4. Static audit of the pack upgrade (July pack to pack dc49fe7)

`jaraudit.py` compares the mod ids and versions of the two mods folders, `idscan.py` compares the ids
each pack defines (blockstates, item models, lang keys, Destroy molecule data) with every namespaced id
string in the world's chunk and entity NBT.

- 164 jars to 166 (183 pack entries, some client only); one mod id removed outright
  (`create_copper_and_zinc`), 21 added (the MIC mods, Thermoo, World of Color, Create Power Loader),
  71 version changes. The big ones: Destroy 0.3.0 to 0.4.1 (the MIC fork), petrolpark 1.4.36 to 1.5.11,
  Cobblemon 1.7.3 to 1.8.1, PowerGrid 0.5.5.1 to 0.6.2, Ice and Fire 2.0-beta.17 to 2.1-beta.1,
  Distant Horizons 3.1.2 to 3.3.1, Sable 2.0.3 to 2.0.5, Project Atmosphere 0.8.1 to 0.9.1, NeoForge
  21.1.230 to 21.1.249. Create stays 6.0.10.
- No mandatory dependency is missing.
- The July pack defines 87 ids the new one does not (62 petrolpark kitchen and redstone programmer
  entries, 17 `create_encapsulated` paints, 2 misspelled Dragons Plus packages, `iceandfire:dread_portal`).
  **None of them occurs anywhere in the world**, pruned or full. Destroy's chunk attachments
  (`destroy:chunk_pollution`, `radioactivity`, `smog`, on 9,455 chunks) keep their names in 0.4.1.
- The world barely uses Destroy: ores, the pollution attachments, a centrifuge, two aging barrels, a
  few items. No Destroy mixture with a molecule id was found, so the 0.3.0 to 0.4.1 mixture risk does
  not apply to this world.
- The KubeJS content the old pack registered is still there: the graphite molds players hold are
  generated by `overgeared_molds.js`, which the new pack keeps. The petrochem KubeJS fluids and Destroy
  molecules (`kubejs:butane` and so on) are gone from the pack and absent from the world.
- `setup_server.py -y` deletes or overwrites without asking: 72 old jars deleted, 59 of the 62 override files
  that left the pack deleted (3 were already gone), 23 overrides that differ from the server's copy overwritten. Of those 23,
  two KubeJS scripts are **local edits made on the live server** (they match no version in the pack's
  history): `destroy_metallurgy_integration.js` (one tag, `destroy` changed to `borax`) and
  `overgeared_mechanical_forging.js` (a longer header comment). The other 21 are mod configs rewritten
  by the mods themselves and older pack versions. The runbook backs the server up first, so `-y` is
  safe, and says to re-apply the two edits if they are still wanted.
- A pack export made from a checkout that lacks the three raw jars the pack keeps in `mods/` (gitignored,
  among them Destroy) silently omits them; export from a checkout that has them (`sha256` against
  `index.toml`).
- **The pack moved on after the boots.** Its master is now cd019b2, which moves four fluid tag files
  (`pumpjack_output`, `c:crude_oil`, `diesel`, `gasoline`) to the `tags/fluid` folders 1.21.1 reads, so
  the Diesel Generators pumpjack pumps `destroy:crude_oil` and `#c:crude_oil` is the pack's own list.
  Nothing else changes. The rehearsal booted dc49fe7 and was not rerun: a fluid tag adds or removes no
  id, and the world has no pumpjack and no crude oil (`idscan.py`), so nothing in sections 6 and 7
  depends on the difference. The first pumpjack after the upgrade is the check.

## 5. Method

Every run is a reflink copy of one of two server folders (the July pack as on the host, and pack dc49fe7
installed by `setup_server.py -y` over a copy of it) with a fresh copy of the pruned world. The rtsim
runs add the 12 mic-rtsim release jars (core and the 11 plugins the pack has mods for) built at 689d311,
which includes main's 812009f (pages through rtsim-jni, the new page format), and mic_native's jar.
No world an rtsim run started from had ever seen an older rtsim build. The sim resets ran jars built
later: r-reset at a6d7762, this branch's glass fixes (6.8) merged with main at b87d42e, which adds
cc55edd's journal barrier (three new core mixins, among them one on vanilla's `IOWorker`, and a C2ME
path) and OILGATE; r-reset2 and its reload at a0f86db, which adds the reset fix (6.8, 3.) and the
engine's village count in `rtsim perf`. The natives are the same build in all of them.

| Run | Pack | rtsim | Purpose |
|---|---|---|---|
| r0-control | July, NeoForge 21.1.230 | no | the control, beside r0-upgrade |
| r0-upgrade | dc49fe7, NeoForge 21.1.249 | no | the pack upgrade alone |
| r1-upgrade, r1-rtsim | dc49fe7 | no, yes | the rtsim pair; rtsim starts 5 s after its partner |
| r2-rtsim, r2-upgrade | dc49fe7 | yes, no | the same pair in the other order |
| r1-rtsim-reload | dc49fe7 | yes | r1-rtsim's world booted again: save, stop, reload |
| r-uninstall | dc49fe7 | removed | r1-rtsim's world after the reload, booted without the rtsim and mic_native jars |
| r-reset | dc49fe7 | yes, a6d7762 | r1-rtsim's world with `world/rtsim/` deleted: SRV2's sim reset |
| r-reset2 | dc49fe7 | yes, a0f86db | the same reset with the reset fix |
| r-reset2-reload | dc49fe7 | yes, a0f86db | a copy of r-reset2's world booted again: an engine restored on the live world |

The two runs of a pair start 5 s apart and run side by side, so they share the laptop's load; the
comparison is per sample (B minus A), and the order is swapped between p1 and p2.

A scenario (`tools/deploy-rehearsal/scenarios.py`, driven over RCON by `runsrv.py`):

1. boot to `Done`; `neoforge tps`; in rtsim runs `rtsim perf` and `rtsim district list`;
2. the bases: eight `forceload add` boxes, 20 s apart, 1,415 chunks in all (five strips over the main
   base, the two villages, one small outpost). At simulation distance 16 that is about what one and a
   half players standing at the bases keep ticking;
3. 120 s to settle; in rtsim runs `rtsim village` in each village and `rtsim district adopt 16` at two
   factories, then 60 s (the other runs wait the same 60 s);
4. 300 s of `neoforge tps` every 5 s (60 samples): the overall and per dimension ms per tick, the java
   process's CPU and RSS, the laptop's load average;
5. rtsim reports again, an entity census (villagers, wandering traders, ribbits, iron golems, every
   entity), `gamerule doTraderSpawning`, `save-all flush`, `stop`.

The reload and reset runs also log every mixin they apply (`-Dmixin.debug.verbose=true`), and the
three reset runs ask `rtsim district info 1` and `rtsim district debug 1` (the main base, district 1
in every run) at the end, after the sampling. None of them adopts anything: their districts are the
ones r1-rtsim adopted.

Settings: `simulation-distance` and `view-distance` 16 (as on the host), `-Xms2G -Xmx8G`,
`-XX:ActiveProcessorCount=4`, mic_native's pools fixed at the host's sizes (4 foreground threads,
2 background), `nice 10`, not pinned to cores.

Where the rehearsal differs from the host:

- **`max-tick-time` 300000** in the measured runs (the host has 60000). On the shared laptop, loading
  the bases makes single ticks of tens of seconds, and in attempt 4 the watchdog killed 2 of the 4
  servers of the rtsim pair while the bases loaded, one with rtsim and one without (section 6.1).
  How far behind the server fell is read from vanilla's `Can't keep up` lines (`spikes.py`); a line
  sums the lag of up to 300 ticks (it fires at 2 s behind, at most every 15 s of tick schedule), so it
  measures sustained overload, not single ticks.
- **The laptop was shared** with other agents' builds, GameTests and benches (load average 7 to 37 over
  the night, on 24 threads). Absolute ms per tick are this laptop under that load, not the host;
  only the side-by-side differences mean something.
- **Hippocampi were kept out** by a KubeJS script in every run, both packs (finding 1): with them, the
  upgraded pack crashes.
- **No player joined.** Forceload tickets stand in for players: no player data, Cobblemon party,
  inventory or Sable ship rejoin was exercised.
- **mic_native's play cap** comes from the core count and has no override: 4 here, 2 on the host's 4
  cores. The sim may use up to 4 foreground threads during play here, 2 there.
- Attempt 3 (pinned to four physical cores per server, at an inherited nice 15) and attempt 4
  (unpinned, the host's 60 s watchdog) are not reported except for the watchdog findings. p0 is from
  attempt 4 (no tick reached 60 s in it); the other runs are attempt 5.

## 6. Results

### 6.1 Boot and stop

| Run | Wall time to `Done` | `Done (...)` | Stop after `stop` | Exit |
|---|---|---|---|---|
| r0-control | 374.5 s | 56.8 s | 6.9 s | 0 |
| r0-upgrade | 393.3 s | 46.5 s | 3.6 s | 0 |
| r1-upgrade | 158.7 s | 30.9 s | 7.6 s | 0 |
| r1-rtsim | 160.7 s | 30.2 s | 5.0 s | 0 |
| r2-upgrade | 104.3 s | 19.3 s | 5.8 s | 0 |
| r2-rtsim | 104.3 s | 19.0 s | 4.8 s | 0 |
| r1-rtsim-reload | 150.8 s | 23.9 s | 6.5 s | 0 |
| r-uninstall | 118.3 s | 13.0 s | 3.9 s | 0 |
| r-reset | 64.2 s | 11.4 s | 3.9 s | 0 |
| r-reset2 | 64.2 s | 14.6 s | 3.9 s | 0 |
| r-reset2-reload | 58 s | 10.1 s | 13.0 s | 0 |

The wall time is mostly the laptop: p0 ran first on a cold page cache and at a different load, p1 and
p2 later. Within a pair, rtsim adds nothing measurable to the boot (2 s in p1, 0 in p2) or the stop. The
host booted in 109 s on 2026-10-19. r-reset2-reload's 13.0 s stop includes 5 s of waiting for the engine's
final save, which a glass solve kept busy (6.7).

What did not boot or did not finish:

- **Ice and Fire 2.1-beta.1 crashes the upgraded pack** (attempt 3, 3.5 minutes after `Done`):
  `HippocampusExplorationGoal.tick` calls `distanceToSqr(target)` after `findWaterTarget` found
  nothing, a NullPointerException, and the server stops with `Ticking entity`. The stack goes through
  Sable's `ActiveSableCompanion.distanceSquaredWithSubLevels`, but the null is Ice and Fire's. The full
  world has 677 hippocampi (48 in the kept chunks), the July pack's 2.0-beta.17 runs with them on the
  host, and the crash needs only a hippocampus in a loaded chunk with no water target: on the live server
  it would come within minutes of the upgrade. Upstream fixed it on 2026-08-18 (IAFEnvoy/IceAndFire-CE
  `f47956e2`, branch 1.21.1, "backport non-GeckoLib fixes from geckolib test": `if (this.target == null
  || ...)`), but no release has it: the newest on Modrinth are 2.1-beta.1 and 2.0, both 2026-07-18. The
  rehearsal kept hippocampi out with a KubeJS script (`EntityEvents.spawned` cancelled), which also
  deletes every hippocampus in a chunk that loads (3 of the 48 here, tamed ones included): fine for a
  measurement, not for the live world. DR1.
- **The 60 s watchdog** killed 2 of the 4 servers of attempt 4's rtsim pairs while the bases loaded,
  one with rtsim and one without, so it is the laptop's load and not rtsim. The host has the same
  watchdog and dies to it about weekly (section 1); DR2 and DR3.

### 6.2 The upgraded pack's log against the control

`logsum.py` groups every ERROR and WARN line with numbers masked and compares the two runs. New in the
upgraded pack (counts from r0-upgrade, the same in every upgraded run, because every run starts from the
July world):

| Lines | What | Effect |
|---|---|---|
| 1,191 ERROR | `unknown or non-serializable data attachment iceandfire:portal_data` | Ice and Fire 2.1 dropped the attachment (the dread portal is gone). One line per chunk as it loads; the chunk is saved without it. One-off, harmless |
| about 55 WARN and ERROR | `unknown structure 'cobblemon:fossils/prehistoric_*'` / `Unknown structure start` | Cobblemon 1.8 renamed or dropped its fossil structures; the starts are discarded as their chunks load. One-off |
| 1 | the `iceandfire:dread_land` dimension is not loaded | it was empty in the copy; nothing lost |
| 3 ERROR | Sable `Failed to apply tag physics properties. Unknown block: copycats:copycat_catwalk` | a pack tag names a block the Copycats version in the pack lacks; harmless |
| 1 ERROR (a stack) | KubeJS `create_power_loader.js`: `invalid return` | the script has a top-level `return` (`if (!Platform.isLoaded('create_power_loader')) return`), which Rhino rejects: **the whole script does not load**, so its recipes are missing. Pack bug, DR4 |
| 2 ERROR (stacks) | KubeJS `Pattern references symbol '/' but it's not defined in the key` | two shaped recipes in `radar_integration.js` (the data link and the auto yaw controller) use a key they do not define and are not registered: uncraftable. Pack bug, DR4 |
| 2 ERROR | Create Propulsion `Failed to parse thruster fuel definition from createpropulsion:mic_petrochem_gasoline` (and `_diesel`): `No key consumption_multiplier` | mic_petrochem 1.0.0's two thruster fuels use the old key, so neither fuel drives a thruster. Fixed in mic-petrochem 1.0.1 (ea1228f, 2026-10-21), not yet in the pack. DR4 |
| about 12 WARN | mixin targets not found (Farmer's Delight, Friends and Foes) | optional compat mixins of petrolpark, thermoo-patches and PowerGrid for mods the pack does not have; harmless |
| 9 WARN | `Configuration file ... is not correct. Correcting` | new config keys filled in with defaults on first start |

Nothing in the upgraded runs' logs says a block, item, block entity or entity type is missing.

### 6.3 World integrity after the upgrade

`prune.py scan` of each run's world after its `stop`, against the pruned world it started from, over the
17,779 chunks the start had (`becompare.py`, which also compares the count of every block entity and
entity id; no block entity id changed count in any run):

| Run | Chunks missing | Block entities | Entities |
|---|---|---|---|
| r0-control | 0 | 18,191 to 18,191 | 3,263 to 3,132 |
| r0-upgrade | 0 | 18,191 to 18,191 | 3,263 to 3,126 |
| r1-upgrade | 0 | 18,191 to 18,191 | 3,263 to 3,134 |
| r1-rtsim | 0 | 18,191 to 18,191 | 3,263 to 3,127 |

The entity changes are what a running world does on its own, the same with and without the upgrade or
rtsim:
dropped items despawn (606 to about 500), wild Pokemon despawn (105 to 82), mobs spawn and die. The 3
hippocampi are the workaround's. Create contraption entities are unchanged, and Sable rewrote all 165
sub-level files in its new format without an error. There are no trains to break (section 2), and no
Destroy mixture with a molecule id (section 4). What the copy could not check is a player's own data,
because nobody logged in: the first login after the upgrade is a check (runbook 8.3).

### 6.4 Server tick time (MSPT)

`neoforge tps` every 5 s for 300 s, with the bases loaded. Each sample is NeoForge's mean over the last
100 ticks, so neighbouring samples overlap (at these tick rates a window is 10 to 25 s) and the
effective sample count is about 20, not 60.

| Pair | Run | Mean ms | sd | p10 | p50 | p90 | java CPU, cores | RSS max | Load |
|---|---|---|---|---|---|---|---|---|---|
| p0 | r0-control | 99.5 | 26.6 | 83.7 | 91.0 | 130.7 | 1.21 | 7.0 GB | 7.4 |
| p0 | r0-upgrade | 99.4 | 27.5 | 81.0 | 90.4 | 139.5 | 1.28 | 7.6 GB | 7.4 |
| p1 | r1-upgrade | 153.7 | 40.5 | 103.1 | 157.3 | 222.0 | 1.13 | 6.9 GB | 22.6 |
| p1 | r1-rtsim | 155.5 | 40.5 | 103.7 | 155.5 | 223.0 | 1.16 | 6.2 GB | 22.7 |
| p2 | r2-rtsim | 164.0 | 54.1 | 105.3 | 138.7 | 242.3 | 1.17 | 5.4 GB | 25.6 |
| p2 | r2-upgrade | 166.9 | 54.2 | 110.0 | 143.4 | 242.6 | 1.14 | 5.4 GB | 25.8 |

Per sample, the second run minus the first:

| Pair | What the difference is | Mean | sd | p10 | p50 | p90 |
|---|---|---|---|---|---|---|
| p0 | upgrade minus control | -0.1 ms | 19.3 | -11.1 | -0.7 | 19.4 |
| p1 | rtsim minus upgrade | +1.7 ms | 38.1 | -55.3 | +5.5 | 47.8 |
| p2 | rtsim minus upgrade | -2.9 ms | 40.5 | -53.0 | -7.8 | 69.1 |

- **The pack upgrade costs nothing measurable**: -0.1 ms with a standard error of about 4 ms.
- **rtsim's tick cost is below what this laptop can resolve**: +1.7 and -2.9 ms, each with a standard
  error of about 9 ms, in opposite orders. rtsim's own counters put it far lower: the district sweeps
  read 6,976 inventories in 365 s in 295 ms of server-thread time (0.8 ms per second of play), the
  puppet pass took 25 to 41 us a tick, and no tick waited for the engine (0 of 4,890 hand-overs).
- **rtsim's threads** add 0.02 to 0.03 cores of CPU (1.16 against 1.13, 1.17 against 1.14), the engine
  stepping on mic_native's background pool.
- **Memory**: no difference rtsim makes shows through G1's heap sizing (RSS 6.2 against 6.9 GB in p1,
  5.4 against 5.4 GB in p2).
- The absolute numbers (100 to 170 ms, 6 to 10 TPS) are this laptop under a load of 7 to 26 with two
  servers at once, not the host.

The GC log of p2 (both runs alike): once the bases are loaded (300 s on), the heap after a young
collection is 2,349 to 2,532 MiB (p50 2,389 with rtsim, 2,373 without), and G1 committed 4.1 GiB of the
8 GiB allowed. Young pauses over the whole run: p50 37 to 39 ms, p90 107 to 120 ms, p99 235 to 238 ms,
the longest 309 ms; from 300 s on, p50 15 to 17 ms and the longest 113 ms. One full collection at the
final save (`System.gc()`, 1.0 to 1.1 s). The java process's resident memory was the committed heap
plus 1.2 GiB in every run, with or without rtsim. This is what section 8.5 sizes the
host's heap from.

### 6.5 rtsim on the live world

Every rtsim run started cleanly and the same way:

- `mic_native natives: status=OK ... build=x86-64-v3`, then `plugin mic_rtsim 0.1.0 loaded`, from
  `<server>/mic_native/natives/plugins/mic_rtsim/0.1.0-<hash>/librtsim_jni.so`: mic_native extracts
  its natives into a new `mic_native/` folder in the server folder, which the service's user must be
  able to write;
- `session started: native engine`, its pages in `world/rtsim/`;
- **mixins**: 21 of the 22 mixins the 689d311 jars declare applied in the reload's verbose mixin log
  (core 13, create 2, destroy 1, endertransmission 1, ribbits 4). The 22nd, `OilChunksSavedDataAccessor`,
  applies when Diesel Generators' `OilChunksSavedData` first loads, which takes an oil query that no
  headless run made; its target field `chunks` is in the pack's createdieselgenerators-1.21.1-1.3.15.jar
  (javap). The resets' later jars declare 25 (main's journal barrier adds three core mixins), and 23
  applied: the two new ones that did are `IOWorkerMixin` and `RegionFileStorageAccessor`. The third,
  `IOWorkerPendingStoreAccessor`, targets vanilla's `IOWorker$PendingStore`, which never loads in this
  pack: C2ME 0.4.0-alpha.0.122 replaces each level's chunk storage, and the barrier takes its C2ME path
  instead (no barrier warning in any reset run's log). Every rtsim mixin config is `defaultRequire 1`,
  so a mixin that missed its target would stop the server when its class loads, at the boot for all
  but those two;
- **villages**: `6 structures are villages the sim adopts`; both kept villages were adopted by the
  first census after their chunks loaded (118 and 176 pieces); 12 censuses by the end of the settle, 35
  at the end. No market formed in the 13 minutes of a run (`no market snapshot yet`);
- **herds**: two founded from the world model in the first steps (62 sheep, 8 horses), the same in
  both runs;
- **wandering traders**: `the sim owns wandering traders now: doTraderSpawning false (was true)`. The
  entity census was the same in every run, with rtsim or without: 12 villagers, 1 wandering trader,
  1 iron golem, no ribbits in loaded chunks;
- **the engine**: one step a tick (4,890 in r2-rtsim), no tick waited for it; the native log has no
  warning;
- **rtsim's only ERROR and WARN lines** are the glass compile failure below.

**Districts.** `rtsim district adopt 16` at the two factories adopted 3,871 and 3,533 block entities
as seeds and made 55 districts:

- district 1 is the main base: 166 chunks and 278 item inventories in one district;
- the other 54 have 1 to 4 chunks and 0 to 3 item inventories each. They are real player blocks
  (furnaces, tanks, pipes and single machines in the 33 by 33 chunks around each adopt point), each its
  own district because a district is a set of 8-connected industrial chunks and these stand apart. Not
  a detection bug, but 54 one-chunk "factories" are probably not what the sim should see (DR6;
  `rtsim district merge`, `exclude` and `split` exist for this).

The sweeps cost little: in r2-rtsim 1,320 sweeps in 365 s read 6,976 inventories (684,000 slots) in
295 ms of server-thread time in all. The slowest single inventory read was 24 ms (r2) and 28 to 35 ms
(r1): one inventory in the main base that costs half a 50 ms tick to read, or the laptop preempting
the server thread. Worth finding on the host (open item 2).

**The glass box failed on the main base** (fixed in this branch, 6.8): `glass: compiling district 1
failed ... IllegalArgumentException: downward motion must be negative: 0.2`, so district 1 was
`NOT_COMPILABLE` (reason 9) and ran on the C19 black box alone, in every rtsim run. The base has a
fan-driven upward chute (motion 0.2, a fan at 256 RPM).

**The threat survey** logs `district 1 is too wide for the perimeter cut (40000 columns at most):
perimeter sent as 0`: the main base is surveyed without its perimeter cut (`surveyMaxColumns` in
`mic_rtsim-threats-server.toml`; DR8).

**The journal** held 1,051 records (213 KB) unacknowledged at the end of r1-rtsim and r2-rtsim alike,
against 1 to 3 before the adopt: the district records the adopt made. A record is acknowledged when
the engine's pages are saved with the world, and no run reached the 6,000-tick autosave at its 6 to 7
TPS, so the journal only grew until the final save. At the host's 20 TPS the autosave comes every 5
minutes. The reload below shows the stop's save took every record.

**Puppets**: none, in any run. rtsim embodies its agents only near players, and no player joined, so
the uninstall's "puppets gone" check had nothing to find (6.6).

**Save, stop, reload** (r1-rtsim-reload, r1-rtsim's world booted again with the verbose mixin log):

- it booted in 150.8 s (`Done (23.9s)`) and stopped in 6.5 s with exit 0. Against r1-rtsim's log
  nothing is new but the verbose mixin log's own warnings;
- the engine came back from its pages: `world model restored: 88 cells in 9 regions`, `phase 1 world
  restored to world tick ...` (a 40-byte snapshot read in 0 ms, 0 inputs replayed), `herds restored:
  2 herds, 3 events booked` and `engine started: pages include observation 1051, 0 agents, 0 outbox
  commands to resend`. Every record the journal held at the stop (1,051, above) was in the pages;
- the 55 districts, both villages and the 1,415 force-loaded chunks came back, and the censuses carried
  on (23 by the end);
- the journal held 430 records 2 minutes after the boot and 927 (191 KB) from minute 7 to the end,
  while 1,274 sweeps ran: these are district records from the bases loading, not sweeps;
- the glass failure came back (the jars predate the fix);
- its MSPT is not a measurement: other agents' builds held the laptop's load at 69 on average, the
  server got 0.65 cores, and the mean tick was 526 ms (sd 196). The GC log agrees with the other runs
  on the live set: 2,290 to 2,748 MiB after young collections (p50 2,367), with 6.4 GiB committed.

### 6.6 Uninstall

r-uninstall boots r1-rtsim's world (after its reload) on the upgraded pack with every rtsim jar and
mic_native taken out:

- it booted in 118.3 s (`Done (13.0s)`), stopped in 3.9 s, exit 0;
- **the log**: against r1-upgrade the only new lines are 243 `Encountered unknown or non-serializable
  data attachment mic_rtsim:industry_seeds` (240) and `mic_rtsim:player_blocks` (3), one for each chunk
  that carries one, as it loads. No missing-registry line beyond the upgrade's own (the same groups
  and counts as r1-upgrade), no rtsim class or mixin named anywhere. NeoForge's one WARN at load lists
  the 13 missing mods (`The following mods have version differences that were not resolved: mic_native
  (version 0.1.0 -> MISSING) ...`), as it does for any mod a world was last saved with, and the stop's
  save dropped them from `level.dat`;
- **the world after the stop**: `idscan.py` finds no `mic_rtsim` id left in any chunk or entity. Every
  chunk with an rtsim attachment was in the bases' force-loaded area, so all 243 loaded and were saved
  without it. A chunk elsewhere that does not load keeps its attachment, harmlessly, until it does;
- **no lost blocks**: no chunk missing, 18,212 block entities before and after against r1-rtsim's
  own scan (18,191 to 18,191 against the pruned start), and the entities' usual drift (3,127 to 3,091:
  dropped items despawning, wild Pokemon);
- **puppets**: none existed, as no player joined (6.5). None can be left behind either: rtsim never
  lets vanilla save a puppet (`EntitySaveMixin` on `shouldBeSaved` and `saveAsPassenger`), so a world
  saved with rtsim has none in it;
- **wandering traders stay off**: `Gamerule doTraderSpawning is currently set to: false`. rtsim gives
  the rule back only while it is installed (it syncs at start and on a config reload, when `trade` or
  `travel` is off), so taking the jars out leaves it false. The rollback (8.6) turns it back on;
- **what stays on disk**, inert without the jars: `world/rtsim/` (152 KB after two runs),
  `world/data/mic_rtsim_bridge.dat`, `_industry.dat`, `_journal.dat` and `_villages.dat` (23 KB in all),
  `config/mic_rtsim-server.toml`, `-industry-server.toml` and `-threats-server.toml`, and the
  `mic_native/` folder in the server folder (9.3 MB, the extracted natives). Vanilla reads a data file
  only when a mod asks for it by name, so nothing complains about them;
- its MSPT (mean 306 ms at a load of 50) is not a measurement, for the reload's reason.

### 6.7 Sim reset (SRV2)

SRV2's sim reset is: stop, delete `world/rtsim/`, start. The rehearsal ran it twice on r1-rtsim's world
after its reload (55 districts, both villages, 1,051 records the pages had taken): r-reset with the
jars at a6d7762, r-reset2 with this branch's fix (6.8, 3.), then r-reset2's world booted again
(r-reset2-reload).

**What a reset keeps and what it loses.** The world keeps what the bridge knows: the districts with
their names, merges and exclusions (`mic_rtsim_industry.dat`), the village registry with the houses the
sim built (`_villages.dat`), the bases' dwell counts and marks (`_bases.dat`), the gamerule memory
(`_bridge.dat`), the journal (`_journal.dat`), and the chunks' rtsim attachments. The engine starts
from nothing and logs it: `the journal starts at observation 1979 but the pages only include 0:
observations 1 to 1978 are lost`, then `going on from 1979`. Everything the sim made is gone: the
phase 1 world (its agents, markets, prices, caravans, gossip, sentiments toward players, and which
houses it built), the herds (founded again from the world model within a minute: 49 sheep and 9 horses
in r-reset, 57 and 8 in r-reset2, against r1-rtsim's 62 and 8, as many as the loaded chunks held), the industry engine's measurements
and calibrations, the threats and the dragons' lairs (found again each session), the ships. On
OmegaSrv today a reset loses nothing, because rtsim has never run there; after weeks of play it loses
the sim's economy, which is why SRV2 keeps it for format changes with no migration.

**r-reset, main's jars: the engine was never told again.** It booted in 64.2 s (`Done (11.4s)`),
stopped in 3.9 s, exit 0, and Java had everything: 55 districts, both villages, 48 censuses. The main
base's glass box compiled (1,258 nodes, 1,790 edges, converged in one round, 2,033 us: the upward chute
fix on the real base), but its window never opened: `window 0/6000`, `0 sweeps (0 sampled)`,
`revision 2 (district 0), last discard revision ... (revision 44)`. Java persists each district's
declared revision and declares a district only when that is behind, so the fresh engine had no
district at all (revision 0), and every sweep was discarded against it. The villages were the same:
Java's registry knew them, so their starts were never adopted again, and the engine had none. A base
marked before the reset would have stayed marked in Java and unknown to the engine. Nothing in the log
said so beyond the engine's two lines above. The journal held 827 records at 84 s and 928 at the end
(no autosave at the laptop's tick rate). Its MSPT (mean 129.9 ms, sd 40.5, p50 111.7, p90 212.3, at a
load of 23, alone) is not a measurement.

**r-reset2, with the fix: the engine was told again.** It booted in 64.2 s (`Done (14.6s)`), stopped
in 3.9 s, exit 0. 12 s after `Done`, before the engine's own lines, the bridge logged:

```
[mic_rtsim] phase 1: the engine's pages lack observations the journal no longer holds (a sim reset?): 2 villages adopted again, 0 base marks to be made again
[mic_rtsim] industry: the engine's pages lack observations the journal no longer holds (a sim reset?): 55 districts are announced again
```

- `rtsim perf` 20 s after `Done`: `phase 1: villages 2 ...; in the engine: villages 2` (the engine's own
  count, new in this branch for this check);
- the main base: `revision 2 (declared 2)`, and its glass checksum `revision 2 (district 2)`, `52 sweeps
  (40 sampled)`, no discard, the window open (`3900/6000 ticks` at the end: 6,000 ticks is 10 minutes
  at the laptop's 10 TPS, longer than the sampling);
- against r-reset the log has only the two lines above that are new; the journal held 1,213 records
  at 20 s and 1,327 at the end;
- its MSPT (mean 101.1 ms, sd 16.6, p50 96.3, p90 125.5, at a load of 13, alone) is not a measurement.

**What the working glass box showed next: its solve on the main base does not converge.** With sweeps
sampled, the engine solved the main base's graph 6 times, and the last solve ran 278 rounds, its cap
(`glass.solveRoundsMax`, GLASSBOX.md 2.10: two rounds a multi-input machine plus two, 138 of them
here), in 2.40 s, about 8.6 ms a round, `NOT converged`; past the cap the solve falls back to its lower
bound (3.2).
One kind, flint at a black-box node, is predicted at -392.6 a minute (bounds minus to plus infinity)
against 0.867 measured. The solve runs inside the engine's step, so the step stalls: `hand-overs: 5661
submitted, 132 ticks waited on the engine`, against 0 in every earlier run, where the main base never
compiled. The server thread does not wait (a tick that finds the step still running only keeps
collecting), so players do not see it; the sim's own clock lags by the solve. On the host's 4 cores,
with the engine's background pool at nice 19 beside Elasticsearch, a solve will take longer. Open item
5 of 6.9, DR9.

**r-reset2-reload: the reset world booted again, an engine restored on the live world.** It reached
`Done (10.1s)` 58 s after the start and stopped with exit 0. (The rehearsal's tool took its boot from
the copied run's old log, so its own boot numbers for this run are wrong; these are from the log.)

- the engine came back from the reset's pages: `pages include observation 3305`, `phase 1 world
  restored ...: 2 nodes`, `herds restored: 2 herds`; no `lack observations` line, as for any restart;
- the main base's glass box worked on the restored engine: `revision 2 (district 2)`, 11 of 23 sweeps
  sampled, no discard, the window open (`1000/6000`). So what failed on the reused GameTest world (6.9,
  4.) does not happen on a real server's restart;
- the solve again: 13 solves, the last 5.43 s, `NOT converged`, at a load of 82 from other agents'
  builds; `257 ticks waited on the engine` of 2,315 hand-overs;
- **the stop's final save failed**: `the native engine's final save failed (status -11): a step was
  still running after 5000 ms; it finishes on its own, pages not saved; the journal replays what the
  pages miss`. The stop took 13.0 s, the 5 s wait included. Nothing is lost by design (the journal,
  saved with the world, holds what the pages miss, and the next start replays it: the path a crash
  takes), but every stop that lands in a solve takes that path. The world was not booted a third time to
  watch the replay.
- its MSPT (mean 442.5 ms, sd 156.1, at a load of 82, 0.94 cores) is not a measurement.

**For SRV2**: an agent with SSH and sudo can reset the sim (stop, delete `world/rtsim/`, start, 8.6),
and with this branch's jars the engine is told again everything Java knows. What it cannot get back is
what only the engine had, the sim's economy above.

### 6.8 rtsim fixes from the rehearsal

The first two are in the Create plugin's glass box, fixed in this branch with a GameTest
(`GlassExtractionGameTests.anUpwardChuteCompiles`, batch `glass_extract_03`); the rehearsal's main runs
used the jars from before them (built at 689d311), the resets the jars after. The third is in the core,
found by the first reset and checked by the second (6.7).

1. **An upward chute made its whole district not compilable.** `CreateCompile.chuteEdges`'s upward
   branch (a fan pushing or pulling the air up) asked `Cycles.chuteGrabPeriod` for its period, which
   re-enacted only `ChuteBlockEntity.tick`'s downward branch and threw on a positive motion. Now
   `Cycles.chuteTicksPerBlockUp` re-enacts the upward branch (an item starts at 0, moves by the motion
   each tick, is handed on above 1: 6 ticks at motion 0.2, a 256 RPM fan), and `chuteGrabPeriod` picks
   the branch by the motion's sign: 7 ticks a grab at 256 RPM. The node stays CALIBRATED (GB45), so the
   period is the starting estimate a measurement corrects. Unverified in game beyond the GameTest.
2. **A chute's direction was not in the glass fingerprint.** The test above found it: a compile made
   before the fan's network formed had the chute under gravity, and nothing made a new one when the fan
   started, so the compile in force had the chute's edges the wrong way round (pulling from above,
   handing down) for as long as nothing else in the district changed. A chunk that loads before its
   kinetic network settles does exactly this. `CreateGlass.fingerprint` now hashes the chute's
   direction (up or down, not its speed), so a fan starting, stopping or turning round recompiles.
3. **A sim reset left the engine without what Java had announced** (6.7). Java keeps in the world's
   data what it has told the engine: each district's declared revision (`mic_rtsim_industry.dat`), the
   adopted villages (`mic_rtsim_villages.dat`) and the marked bases (`mic_rtsim_bases.dat`). Delete the
   pages and the fresh engine never hears of them again. `BoundarySession.observationsLost()` now
   applies the kernel's own gap rule (`Replay::reconcile`: the journal starts past the observation after
   the pages) at the session start, and then the industry module takes U10's path (forget everything
   announced, declare every district again), phase 1 adopts every known village again (the engine takes
   a village it knows as a no-op) and the base tracker drops its marks, so each base is marked again the
   next time its player dwells there (the engine refuses a mark for a player it does not know yet). Both
   modules log a WARN when they do it. A unit test (`BoundaryProtocolTest.aSimResetIsSeenAsLostObservationsAndARestartIsNot`)
   covers the rule against a restart, a reset and the edge cases; the GameTests were not rerun for it.

### 6.9 Open items (not fixed here)

1. **`rtsim perf` says `0 agents`** while the engine runs two herds (62 sheep, 8 horses): the counter
   is the puppet world's, and the herds are not in it. On a live server the line reads as "the sim is
   empty". A wording or counter change in the core, not a deploy problem.
2. **The slowest single inventory read** was 24 ms in r2-rtsim, 28 to 35 ms in r1-rtsim and 53 ms in
   the reload under a load of 69. Either one inventory in the main base costs half a tick to read, or
   the laptop preempted the server thread mid-read. `rtsim district perf` does not say which inventory;
   worth finding on the host's first day (8.4).
3. **The journal grows until a save** (6.5): 1,051 records (213 KB) after an adopt, all acknowledged
   by the next save. At 20 TPS that is every 5 minutes; a server that falls far behind saves rarely, and
   the journal grows with it. Not a bug, but a reason to keep the autosave on.
4. **The Create plugin's glass GameTests on a reused GameTest world.** On a fresh `run-gametest/world`
   all 146 required tests pass with this branch's change. A second run on the world the first had left
   failed six `glassexit.*` tests in its first two minutes and passed the rest: their windows never
   opened (`window 0/2400 ticks from 0`, no sweep sampled), and the compile of a test 40 s after the
   start reads `accepted at 257020`, one tick after the engine's `phase 1 world restored to world tick
   257019`, as if the industry engine's clock stood still after the restore while the unthrottled
   server ran on. The first run (fresh) also failed `glassexit.crushing_wash_blast` (measured 0 against
   24.2 a minute) and `theDebugOutputShowsWhatJavaKnows` (industry_10) under a load near 40; both pass
   in the fresh rerun. On a real server a restored engine's glass windows do open (r-reset2-reload,
   6.7), so the stalled clock is the reused GameTest world's; its cause was not found.
5. **The glass solve on a large district** (6.7): 278 rounds, the cap, in 2.4 to 5.4 s, never
   converged, inside the engine's step, on the main base's 1,258 nodes and 138 multi-input machines.
   Two ways out, for the glass box's owners: a solve bounded in time (or spread over steps), or solved
   off the step on the background pool with the step taking the last finished solution. Until then,
   DR9. The graph is in the rehearsal's private runs (`rtsim district debug 1`), and a GameTest at
   that size would reproduce it.
6. **A module that starts fresh on its own is not seen by the bridge.** The fix of 6.8 (3.) sees a
   reset of the whole engine. A single module whose pages the build cannot read starts fresh by itself
   (the industry engine, W23; phase 1, the herds, the dragons, the ships and the conflict module,
   PF18) while the other modules' pages and the observation sequence stay, so the bridge does not
   announce again what that module lost: the industry engine would have no districts, phase 1 no
   villages. The fix is the engine telling the bridge which modules started fresh at its start.
   Until then, an rtsim update that changes a page format is done as a whole reset (8.6).
7. **A reset forgets the houses the sim built.** The registry keeps them and the village is adopted
   again with its grown box, but the engine no longer knows which houses it built, nor a build in
   flight at the stop. No house was built in the rehearsal: this is from the code.
8. **Phase 1 switched off and on loses the villages first seen while it was off.** The engine ignores
   phase 1's records while it is off (U10), Java registers the village as adopted all the same, and
   unlike the industry module nothing announces it again when phase 1 comes back. From the code; the
   rehearsal never switched a module.

## 7. What a migration would need

**The pack upgrade (July pack to dc49fe7): no world migration, for this world.** No id the world uses
leaves the pack (section 4), no block entity, contraption or Sable sub-level was lost (6.3), there are
no trains, and there is no Destroy mixture with a molecule id. What the new pack drops by itself as
chunks load, with nothing to do about it: Ice and Fire's `portal_data` attachments, Cobblemon's old
fossil structure starts, the empty `iceandfire:dread_land` dimension. What it needs before it goes
live is not a migration but three pack changes: the hippocampus fix (DR1), the two broken KubeJS
scripts (DR4) and the two live KubeJS edits (DR7). What the rehearsal could not check is the players'
own data at login (inventories, ender chests, Cobblemon parties and PCs, players logged out aboard a
Sable ship): the first login is the check, with the backup behind it.

**rtsim's first install: nothing to migrate.** rtsim starts with an empty `world/rtsim/` and adopts
nothing it is not told to: villages from their structure starts as they load, districts only through
`rtsim district adopt`. What it writes into the world is listed in 6.6.

**An rtsim update that changes the save format.** mic-rtsim is unreleased, and before a public release
it carries no migration code (no backward compatibility before release). So an update that changes the
format is SRV2's sim reset, which is all an agent with SSH and sudo needs to do (runbook 8.6): stop,
delete `world/rtsim/`, start; 6.7 shows it working with this branch's jars. A page format change also
resets a module by itself: a module whose pages the engine cannot read (an older format among them)
starts fresh, and its old pages are deleted at start (PF18). But the bridge does not see that (6.9,
6.), so the runbook resets the whole sim instead. 6.7 says what a reset keeps and what it loses. When rtsim is
released and its saves start to matter, a migration is code in rtsim run at start, and the rehearsal's
tools (a copy of the world, the scenario, `becompare.py`, the log groups) are how to rehearse it.

## 8. Runbook

For Ben, or an agent Ben gives sudo on OmegaSrv (the rehearsal had read access only). Facts it relies on:
the service is `minecraft-server`, runs as `sistermcserver` in `/var/lib/mc/server`, has `Restart=always`
with `RestartSec=10`, and the world's `level.dat`, `playerdata/` and `data/*.dat` are readable by
`sistermcserver` only. The disk had 347 GB free on 2026-10-21. Never stop the server with `/stop` in
its console for maintenance: systemd starts it again 10 seconds later. Use `systemctl`.

### 8.1 Build on the laptop

1. Pack: at cd019b2 or later, with the DR1 and DR4 fixes in (mic-petrochem 1.0.1 among them). Export
   the mrpack from a pack checkout that has the raw jars `index.toml` lists (Destroy among them; check
   each with `sha256sum` against `index.toml`), and copy `scripts/setup_server.py` with it.
2. rtsim: from main at or after the merge of `feat/deploy-rehearsal` (the fixes of 6.8), in the
   mic-rtsim repository, `tools/natives/build-natives.sh --only linux-x86_64-gnu` in
   mic-native's dev shell, then `cd neoforge && ./gradlew -Prelease :core:jar` plus one `:<plugin>:jar`
   for each of the eleven plugins (carryon, climate, create, destroy, dieselgenerators,
   endertransmission, iceandfire, oreexcavation, powergrid, radar, ribbits). Terra Entity and Sandworms
   are not in the pack: leave them out. Build them after any GameTest run in the same checkout:
   `runGameTestServer` rewrites `core/build/libs` with a jar that is not a release one.
3. mic_native: its jar from mic-native (`mic_native-1.21.1-0.1.0.jar`); it carries baseline and
   x86-64-v3 builds and picks v3 on the host's AVX2 CPU.
4. Copy the mrpack, `setup_server.py` and the 13 jars to the host (`scp -P 23 ... carol@host:/tmp/`).

### 8.2 Back up

1. Wait for an empty server: `grep -E 'joined the game|left the game' /var/lib/mc/server/logs/latest.log | tail`.
2. `sudo systemctl stop minecraft-server`, then check `systemctl is-active minecraft-server` says
   `inactive` and `pgrep -u sistermcserver java` finds nothing.
3. `sudo cp -a /var/lib/mc/server /var/lib/mc/server-backup-$(date +%F)`: a full copy, as root,
   so the owner-only files come along (about 2.5 GB plus the Distant Horizons database). This is the
   rollback for everything below.

### 8.3 Upgrade the pack (no rtsim yet)

1. `cd /var/lib/mc/server && sudo -u sistermcserver nix shell nixpkgs#python3 -c python3
   /tmp/setup_server.py --mrpack /tmp/<pack>.mrpack --output /var/lib/mc/server -y`, with the
   service's JDK first on `PATH` for the NeoForge installer. `-y` deletes the 72 old jars and the
   override files that left the pack, and overwrites 23 changed overrides (section 4).
2. Check `run.sh` now names `neoforge/21.1.249/unix_args.txt`.
3. Re-apply the two live KubeJS edits if Ben still wants them (diff
   `kubejs/server_scripts/destroy_metallurgy_integration.js` and `overgeared_mechanical_forging.js`
   against the backup).
4. JVM arguments: replace `user_jvm_args.txt` (section 8.5). The installer leaves it alone.
5. `sudo systemctl start minecraft-server`; follow `logs/latest.log` to `Done`, then compare it with
   section 6.2's list of expected errors. Let one player log in and check an inventory, an ender
   chest, the Cobblemon party and PC, a train and a Sable ship: the rehearsal could not check any of
   them (section 2).

### 8.4 Install rtsim

1. `sudo systemctl stop minecraft-server`.
2. Copy the 13 jars into `mods/` as `sistermcserver`. mic_native extracts its natives into a
   `mic_native/` folder in the server folder at start, so the folder must be writable by that user.
3. Start. In `logs/latest.log` look for `mic_native natives: status=OK ... build=x86-64-v3`,
   `plugin mic_rtsim 0.1.0 loaded`, `session started: native engine`, `6 structures are villages the
   sim adopts`, and `the sim owns wandering traders now: doTraderSpawning false`. No line may say
   `Mixin apply` or `InvalidInjectionException` (every rtsim mixin is required, so a failed one stops
   the server with a crash report).
4. Districts: from the console (`sudo -u sistermcserver screen -r minecraft`, detach with Ctrl-A D),
   `execute positioned <x> <y> <z> in minecraft:overworld run rtsim district adopt 16` at each factory,
   then `rtsim district list` to see what it made (55 districts in the rehearsal, DR6) and `rtsim
   district info <id>` on the main base. With the glass box on (DR9 recommends it off for now): its
   `glass:` line must say `compile N: ... nodes`, the log must have no `glass: compiling district <id>
   failed` and no `is not compilable`, and a few minutes on its `glass checksum:` line must show sweeps
   sampled (`52 sweeps (40 sampled)`) and the same revision twice (`revision 2 (district 2)`). A
   `(district 0)` there, or a last discard of `revision` over and over, is the reset bug of 6.8 (3.).
   With it off, `rtsim district info` says nothing about the glass, and `rtsim perf` should show
   few `ticks waited on the engine`.
5. Watch for a day: `/neoforge tps`, `/rtsim perf`, `rtsim district perf` (the slowest inventory read,
   open item 2 of 6.9), the size of `world/rtsim/`, and the GC log's heap after young collections.

### 8.5 JVM arguments

The host's `user_jvm_args.txt` is `-Xms8G` and `-Xms12G` with no `-Xmx`. The second `-Xms` wins, and
without `-Xmx` the JVM raises its maximum to the initial size, so the heap is a fixed 12 GiB on a 16 GB
host that also runs Elasticsearch, and part of it is in swap (section 1). The rehearsal measured, with
the bases loaded (6.4): 2.3 to 2.5 GiB (2,349 to 2,532 MiB) live after each young collection, 4.1 to 6.4 GiB committed
under an 8 GiB cap, and 1.2 GiB of non-heap memory on top. Replace the file with:

```
-Xms4G
-Xmx8G
-Xlog:gc:file=logs/gc.log:time,uptime:filecount=5,filesize=20M
```

- 8 GiB is three times the live set measured here. A player at the main base, Distant Horizons'
  server-side generation and rtsim's growth over months add to it; the GC log says after a week whether
  it is enough (the `Pause Young` lines' heap after collection should stay under about 5 GiB).
- 4 GiB initial saves the heap growing in small steps during boot, and G1 does not shrink below it.
- The java process then peaks near 9.2 GiB resident instead of over 12, which leaves the OS, the page
  cache and Elasticsearch about 6 GiB and keeps the heap out of swap.
- G1 is JDK 21's default on this host: no collector flags. `-XX:ActiveProcessorCount` and the
  `mic_native` thread properties of the rehearsal are not needed on the host (its defaults are its 4
  cores).
- Keep `max-tick-time` at 60000 in `server.properties` (DR3).

### 8.6 Rollback

- **rtsim only** (keep the new pack): first give the wandering traders back, while rtsim is still
  installed: set `trade = false` under `[modules]` in `config/mic_rtsim-server.toml` and restart, and
  rtsim logs `trade or travel is off: doTraderSpawning back to true`. Then stop; delete
  `mods/mic_rtsim*.jar` and `mods/mic_native-*.jar`; start. If the jars are already gone, run
  `gamerule doTraderSpawning true` instead: nothing else turns it back on (6.6). Expect section 6.6's
  one-off `unknown or non-serializable data attachment mic_rtsim:*` errors. What rtsim leaves on disk
  is inert (6.6): `world/rtsim/`, `world/data/mic_rtsim_*.dat`, `config/mic_rtsim*.toml` and the
  `mic_native/` folder can stay or go.
- **Everything**: stop; `sudo mv /var/lib/mc/server /var/lib/mc/server-failed`; `sudo mv
  /var/lib/mc/server-backup-<date> /var/lib/mc/server`; start.
- **Sim reset** (SRV2, when a save format change has no migration): stop; delete `world/rtsim/` and
  nothing else (the world's `data/mic_rtsim_*.dat` is how the bridge knows what to announce again);
  start. The log must then say `the engine's pages lack observations the journal no longer holds` twice,
  from the industry module and from phase 1, with the counts of districts and villages announced
  again, and `rtsim perf`'s phase 1 line must count as many villages `in the engine` as before the
  semicolon. Section 6.7 says what a reset keeps and what it loses. Only with jars at or after this
  branch's merge: older ones leave the fresh engine without the districts and villages (6.8, 3.). Do
  the same for an rtsim update that changes a page format: delete `world/rtsim/` rather than let the
  engine drop one module's pages by itself, which the bridge does not see (6.9, 6.).
- **Restoring a world from a backup**: `world/rtsim/` and `world/data/mic_rtsim_*.dat` belong to the
  world they were saved with. Restore them with it, from the same backup, never on their own; if in
  doubt, delete `world/rtsim/` (a sim reset).

## 9. Choices for Ben

Each row is a choice the rehearsal cannot make: the pack, the host or what the sim should see. The
recommended option is first.

| Id | Choice | Recommended | Alternatives | Why |
|---|---|---|---|---|
| DR1 | The Ice and Fire hippocampus crash (6.1), a blocker for the pack upgrade | A null-guard mixin on `HippocampusExplorationGoal.tick` that does what upstream `f47956e2` does (`if (this.target == null ...) return`), in one of the pack's MIC mods, until an Ice and Fire release has the fix | (b) Build IceAndFire-CE's `1.21.1` branch at or after `f47956e2` and ship that jar: unreleased code in the pack. (c) Keep Ice and Fire 2.0-beta.17, the July version the host runs with 677 hippocampi: needs checking that the pack's other mods (rtsim's iceandfire plugin among them) accept it. (d) The rehearsal's KubeJS spawn cancel: no, it deletes the players' hippocampi, tamed ones too | The crash needs only a hippocampus in a loaded chunk with no water target, so the live server would die within minutes of the upgrade. A mixin is the smallest change and goes away when a release lands |
| DR2 | The host's heap (8.5) | `-Xms4G -Xmx8G` and a GC log | (b) `-Xmx6G`: more room for Elasticsearch and the page cache, 2.4 times the live set. (c) Leave the fixed 12 GiB | The live set is 2.3 to 2.5 GiB with or without rtsim; a 12 GiB heap on 16 GB with Elasticsearch puts the heap in swap, the likeliest cause of the weekly watchdog crashes |
| DR3 | The watchdog (`max-tick-time`) | Keep 60000 | (b) 300000, as the rehearsal's runs used on the laptop: fewer restarts, but a player waits up to 5 minutes in a hung tick. (c) -1, off | A tick over a minute is a hang that deserves a crash report and a restart; the fix is DR2, not a longer leash. Revisit if the crashes go on after DR2 |
| DR4 | The pack's broken scripts and tag (6.2) | Fix them in the pack before the upgrade: `create_power_loader.js` without its top-level `return` (wrap the body in `if (Platform.isLoaded(...)) { ... }`), the missing `/` key in `radar_integration.js`'s two shaped recipes, `copycats:copycat_catwalk` out of Sable's physics tag, and mic-petrochem 1.0.1 (its thruster fuels' `consumption_multiplier` key) in place of 1.0.0 | (b) Ship as is: Create Power Loader's recipes are missing, the data link and auto yaw controller are uncraftable, Sable logs 3 errors, and neither petrochem fuel drives a Create Propulsion thruster | Pack bugs, not world damage; each is a one-line fix or a version bump in the pack repository |
| DR5 | Deploy order | The pack upgrade alone first, with the GC log; rtsim a week later | (b) Both in one maintenance window: one downtime, but any new problem has two suspects | The upgrade carries the blocker (DR1) and the heap change (DR2); a week on it says whether the watchdog crashes stop before rtsim adds anything |
| DR6 | What the sim sees around the main base (6.5) | `rtsim district adopt 16` at the main base, as rehearsed, and keep the 54 small districts it makes | (b) `rtsim district exclude` the small ones, one command each where they stand. (c) A smaller adopt radius: may leave parts of the main base out | The 54 are real player blocks (0 to 3 inventories each) and cost next to nothing to sweep; whether the sim should count a lone furnace as a factory is a design question, and exclude is reversible |
| DR7 | The two KubeJS edits made on the live server (section 4) | Re-apply both after the upgrade and put the `borax` tag edit in the pack, so the next upgrade keeps it | (b) Drop them | `setup_server.py -y` overwrites them; the tag edit changes recipes, the other is a comment |
| DR8 | The threat survey's column cap (6.5) | Leave `surveyMaxColumns` at 40,000; measure the survey's cost on the host before raising it | (b) Raise it until the main base fits, unmeasured | The main base is surveyed without its perimeter cut today, which is a fallback, not an error |
| DR9 | The glass box on the host (6.7) | `enabled = 0` under `[industry.glass]` in `config/mic_rtsim-industry-server.toml` until a solve of the main base is bounded in time (6.9, 5.) | (b) Leave it on and watch `ticks waited on the engine` in `rtsim perf`: the server thread does not wait, the sim lags by each solve, and a stop during one leaves the pages to the journal. (c) `maxNodes` (20,000) below 1,258, which leaves the main base out and compiles the small districts | The glass box is a read-only checksum today: off, the sim runs on the black box alone, as it did in every rehearsal run before the chute fix, and nothing a player sees changes |
