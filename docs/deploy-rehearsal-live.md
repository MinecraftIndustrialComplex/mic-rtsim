# Deploy rehearsal: mic-rtsim on a copy of LiveSrv

Rehearsal of DECISIONS.md SRV1: copy Ben's live MIC server (LiveSrv) read-only, prune the world to what
players built, boot it locally on the old pack, the current pack, and the current pack with mic-rtsim, and
write down what a real deploy needs. Run 2026-10-20 and 21 on the laptop against a snapshot taken
2026-10-20.

Private data stays private: the world copies, logs and player files live under `~/mic-livesrv/` on the
laptop and are not in this repository. The tools are in `tools/deploy-rehearsal/` (stdlib Python, no
private data in them; the base coordinates and the seed are read from private files).

> Note (2026-10-21): DECISIONS.md C8a replaced the `doTraderSpawning` gamerule switch with a veto on
> vanilla's trader spawner. Everywhere below, `doTraderSpawning` findings, log lines and rollback steps describe
> the build rehearsed, not today's: the bridge no longer writes the gamerule.

> Round 2 (2026-10-23): the rehearsal ran again on that day's pack and mic-rtsim main (DECISIONS.md
> DR-0a). Its findings, R2-1 to R2-11, are in section 10; sections 0 to 7 are round 1's. The runbook
> (section 8) and the choices (section 9) are updated for round 2 and say where.

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
`~/mic-livesrv/snap-2026-10-21/` (2.5 GB, world 1.6 GB). About 264 files are owner-only on the host and
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
LiveSrv today a reset loses nothing, because rtsim has never run there; after weeks of play it loses
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

For Ben, or an agent Ben gives sudo on LiveSrv (the rehearsal had read access only). Facts it relies on:
the service is `minecraft-server`, runs as `mcserver` in `/var/lib/mc/server`, has `Restart=always`
with `RestartSec=10`, and the world's `level.dat`, `playerdata/` and `data/*.dat` are readable by
`mcserver` only. The disk had 347 GB free on 2026-10-21. Never stop the server with `/stop` in
its console for maintenance: systemd starts it again 10 seconds later. Use `systemctl`.
The SSH account is in `wheel`, but its sudo needs a password (2026-10-23, DR-0b): run `sudo -n true`
first, and stop if it asks for one.

### 8.1 Build on the laptop

1. Pack: at aaed844 or later, the commit round 2 rehearsed, with the DR1 and DR4 fixes in (mic_fixes
   1.0.2 and mic_petrochem 1.0.2 among them). Export the mrpack from a pack checkout that has the raw
   jars `index.toml` lists (Destroy among them; check each with `sha256sum` against `index.toml`), with
   `packwiz-0-unstable-2026-02-18` or newer (the pack's cache is version 2, which the older packwiz in
   the flake cannot read), and copy `scripts/setup_server.py` with it.
2. rtsim: from main after the last format change before the deploy, in the mic-rtsim repository,
   `tools/natives/build-natives.sh --only linux-x86_64-gnu` in mic-native's dev shell, then `cd
   neoforge && ./gradlew -Prelease :core:jar` plus one `:<plugin>:jar` for each plugin whose mod is in
   the pack's index: twelve at round 2's commit (carryon, climate, create, destroy, dh,
   dieselgenerators, endertransmission, iceandfire, oreexcavation, powergrid, radar, ribbits), plus
   mowziesmobs if main has that plugin by then. Terra Entity and Sandworms are not in the pack: leave
   them out. Build them after any GameTest run in the same checkout: `runGameTestServer` rewrites
   `core/build/libs` with a jar that is not a release one. Round 2 ran main at `4bc03a6d`, and a format
   changed after it (Mowzie M2, `89fb716f`): run 10.6's rtsim steps once more on the deploy's jars
   (rtsim, reload, reset) before they go to the host.
3. mic_native: its jar from mic-native (`mic_native-1.21.1-0.1.0.jar`); it carries baseline and
   x86-64-v3 builds and picks v3 on the host's AVX2 CPU.
4. Copy the mrpack, `setup_server.py` and the jars (14 at round 2: the core, 12 plugins and
   mic_native) to the host (`scp -P <port> ... <user>@<host>:/tmp/`).

### 8.2 Back up

1. Wait for an empty server: `grep -E 'joined the game|left the game' /var/lib/mc/server/logs/latest.log | tail`.
2. `sudo systemctl stop minecraft-server`, then check `systemctl is-active minecraft-server` says
   `inactive` and `pgrep -u mcserver java` finds nothing.
3. `sudo cp -a /var/lib/mc/server /var/lib/mc/server-backup-$(date +%F)`: a full copy, as root,
   so the owner-only files come along (about 2.5 GB plus the Distant Horizons database). This is the
   rollback for everything below.

### 8.3 Upgrade the pack (no rtsim yet)

1. `cd /var/lib/mc/server && sudo -u mcserver nix shell nixpkgs#python3 -c python3
   /tmp/setup_server.py --mrpack /tmp/<pack>.mrpack --output /var/lib/mc/server -y`, with the
   service's JDK first on `PATH` for the NeoForge installer. `-y` deletes the 72 old jars and the
   override files that left the pack, and overwrites 23 changed overrides (section 4).
2. Check `run.sh` now names `neoforge/21.1.249/unix_args.txt`.
3. Re-apply the two live KubeJS edits if Ben still wants them (diff
   `kubejs/server_scripts/destroy_metallurgy_integration.js` and `overgeared_mechanical_forging.js`
   against the backup).
4. JVM arguments: `setup_server.py` sets `-Xms` equal to `-Xmx` and with `-y` does so without asking,
   but it never lowers a heap: it takes the largest of the requested size and the file's old `-Xms` and
   `-Xmx`, so the host's `-Xms8G -Xms12G` comes out as `-Xms12G -Xmx12G` (round 2, R2-2). After the
   script, replace `user_jvm_args.txt` with section 8.5's three lines and check it with `cat`.
5. `sudo systemctl start minecraft-server`; follow `logs/latest.log` to `Done`, then compare it with
   section 6.2's list of expected errors and 10.2's additions. Distant Horizons 3.3.3 resets
   `config/DistantHorizons.toml` to its defaults at this first start (`config is of an older version
   ... resetting config`) and updates its LOD database (R2-7); read the new file once if Ben wants
   non-default DH settings on the server. Let one player log in and check an inventory, an ender
   chest, the Cobblemon party and PC, a train and a Sable ship: the rehearsal could not check any of
   them (section 2).

### 8.4 Install rtsim

1. `sudo systemctl stop minecraft-server`.
2. Copy the jars (14 at round 2) into `mods/` as `mcserver`. mic_native extracts its natives
   into a `mic_native/` folder in the server folder at start, so the folder must be writable by that
   user.
3. Start. In `logs/latest.log` look for `mic_native natives: status=OK ... build=x86-64-v3`,
   `plugin mic_rtsim 0.1.0 loaded`, `session started: native engine`, `6 structures are villages the
   sim adopts`, and `the sim owns wandering traders now: vanilla's trader spawner is cancelled while
   trade and travel are on (doTraderSpawning is not touched)` (C8a; round 1's build logged
   `doTraderSpawning false` instead). No line may say `Mixin apply` or `InvalidInjectionException`
   (every rtsim mixin is required, so a failed one stops the server with a crash report).
4. Districts: from the console (`sudo -u mcserver screen -r minecraft`, detach with Ctrl-A D),
   `execute positioned <x> <y> <z> in minecraft:overworld run rtsim district adopt 16` at each factory,
   then `rtsim district list` to see what it made (8 districts in round 2, the main base in three of
   them; round 1's build made 55; DR6 says whether to merge) and `rtsim district info <id>` on each part
   of the main base. The glass box is on (DR9): each `glass:` line must say `compile N: ... nodes` and
   a `last N us` in microseconds to tens of milliseconds (round 2: 32.2 ms at most split, 15.3 ms
   merged), the log must have no `glass: compiling district <id> failed` and no `is not compilable`,
   and a few minutes on the `glass checksum:` line must show sweeps sampled (`55 sweeps (43
   sampled)`) and the same revision twice (`revision 1 (district 1)`). A `(district 0)` there, or a
   last discard of `revision` over and over, is the reset bug of 6.8 (3.). `rtsim perf` should show no or few `ticks waited on the
   engine` (round 2: 0).
5. Watch for a day: `/neoforge tps`, `/rtsim perf`, `rtsim district perf` (the slowest inventory read,
   open item 2 of 6.9), the size of `world/rtsim/`, and the GC log's heap after young collections.

### 8.5 JVM arguments

The host's `user_jvm_args.txt` is `-Xms8G` and `-Xms12G` with no `-Xmx`. The second `-Xms` wins, and
without `-Xmx` the JVM raises its maximum to the initial size, so the heap is a fixed 12 GiB on a 16 GB
host that also runs Elasticsearch, and part of it is in swap (section 1). The rehearsal measured, with
the bases loaded (6.4): 2.3 to 2.5 GiB (2,349 to 2,532 MiB) live after each young collection, 4.1 to 6.4 GiB committed
under an 8 GiB cap, and 1.2 GiB of non-heap memory on top. Replace the file with:

```
-Xms8G
-Xmx8G
-Xlog:gc:file=logs/gc.log:time,uptime:filecount=5,filesize=20M
```

- 8 GiB is three times the live set measured here. A player at the main base, Distant Horizons'
  server-side generation and rtsim's growth over months add to it; the GC log says after a week whether
  it is enough (the `Pause Young` lines' heap after collection should stay under about 5 GiB). Round 2
  ran these arguments (10.7): 2.3 to 2.6 GiB live with the bases loaded, 3.4 to 3.7 GiB on the full
  world, which filled the heap as many far-apart chunks loaded at once (R2-6). On the host, `Evacuation
  Failure` or `Pause Full` lines other than the one at each stop are that, and worth a look.
- `-Xms` equal to `-Xmx` (ST3): on big Create worlds the live heap near G1's marking threshold at
  `-Xmx6G` cost 10 to 16 ms of background CPU per tick (`docs/stress-worlds-results.md` §8). A heap of
  at least 8 GB with `-Xms` equal to `-Xmx` avoids it, and the heap no longer grows in steps during boot.
- The java process then sits near 9.2 GiB resident (the 8 GiB heap plus the 1.2 GiB of non-heap memory),
  all of the heap committed from the start, instead of over 12. That leaves the OS, the page cache and
  Elasticsearch about 6 GiB and keeps the heap out of swap.
- G1 is JDK 21's default on this host: no collector flags. `-XX:ActiveProcessorCount` and the
  `mic_native` thread properties of the rehearsal are not needed on the host (its defaults are its 4
  cores).
- Keep `max-tick-time` at 60000 in `server.properties` (DR3).

### 8.6 Rollback

- **rtsim only** (keep the new pack): stop; delete `mods/mic_rtsim*.jar` and `mods/mic_native-*.jar`;
  start. Since C8a rtsim never writes `doTraderSpawning` (it cancels vanilla's trader spawner while it
  runs), so there is nothing to give back first: round 2 read the gamerule true in every run, with
  rtsim, after it and without it (10.6). Only a world that ran a build from before C8a (round 1's) can
  have it false: check `gamerule doTraderSpawning` after the start and set it true if not. Expect
  section 6.6's one-off `unknown or non-serializable data attachment mic_rtsim:*` errors, one per chunk
  that carries one as it loads (round 2: 74). What rtsim leaves on disk is inert (6.6): `world/rtsim/`,
  `world/data/mic_rtsim_*.dat`, `config/mic_rtsim*.toml` and the `mic_native/` folder can stay or go.
- **Putting rtsim back** after the world ran without it (UR-1): put the jars back and start; nothing
  else. rtsim reads `level.dat`'s mod list at the start, sees that the world ran without it, and resets
  the whole sim itself (docs/choices-reinstall-guard.md). Expect, at ERROR, `[mic_rtsim] REINSTALL:
  ...` and `[mic_rtsim] REINSTALL RESET (UR-1): ...`, which say that `world/rtsim/` moved to
  `world/rtsim-reinstall-<time>/`; then `[industry] reinstall reset: N chunks of M districts are
  adopted again as they load`, the sim reset's two `the engine's pages lack observations the journal no
  longer holds (the reinstall reset)` lines, and `reinstall reset: saved; marker
  mic_rtsim-reinstall.txt deleted` at the first save. `rtsim perf` says `reinstall guard: SIM RESET at
  this start (UR-1)` with the re-adoption's progress for the whole session. Check that `rtsim district
  list` keeps its districts as their chunks load. `rtsim district adopt` only the machines built during
  the gap in a chunk that was in no district. Delete `world/rtsim-reinstall-*` once satisfied. To undo
  the reset (a gap known to be harmless): stop, move that directory back to `world/rtsim/`, delete
  `world/mic_rtsim-reinstall.txt` if it is still there, and start (RG9). Measured on a copy of round 2's
  uninstalled world (10.6): 6 districts before and after, none dissolved.
- **Everything**: stop; `sudo mv /var/lib/mc/server /var/lib/mc/server-failed`; `sudo mv
  /var/lib/mc/server-backup-<date> /var/lib/mc/server`; start.
- **Sim reset** (SRV2, when a save format change has no migration): stop; delete `world/rtsim/` and
  nothing else (the world's `data/mic_rtsim_*.dat` is how the bridge knows what to announce again);
  start. The log must then say `the engine's pages lack observations the journal no longer holds` twice,
  from the industry module and from phase 1, with the counts of districts and villages announced
  again (round 2: 6 districts and 2 villages, 10.6), and `rtsim perf`'s phase 1 line must count as many
  villages `in the engine` as before the semicolon. Section 6.7 says what a reset keeps and what it
  loses. Only with jars at or after this branch's merge: older ones leave the fresh engine without the
  districts and villages (6.8, 3.). Do
  the same for an rtsim update that changes a page format: delete `world/rtsim/` rather than let the
  engine drop one module's pages by itself, which the bridge does not see (6.9, 6.).
- **Restoring a world from a backup**: `world/rtsim/` and `world/data/mic_rtsim_*.dat` belong to the
  world they were saved with. Restore them with it, from the same backup, never on their own; if in
  doubt, delete `world/rtsim/` (a sim reset).

## 9. Choices for Ben

Each row is a choice the rehearsal cannot make: the pack, the host or what the sim should see. The
recommended option is first. Round 2 (section 10) changed DR1, DR2, DR3, DR4, DR6, DR8 and DR9 and
added DR10; each says so.

| Id | Choice | Recommended | Alternatives | Why |
|---|---|---|---|---|
| DR1 | The Ice and Fire hippocampus crash (6.1), a blocker for the pack upgrade | **Round 2: done.** mic_fixes 1.0.2 in pack aaed844 carries the guard, and it applied on the full world (R2-1, 10.2). Round 1's recommendation was: a null-guard mixin on `HippocampusExplorationGoal.tick` that does what upstream `f47956e2` does (`if (this.target == null ...) return`), in one of the pack's MIC mods, until an Ice and Fire release has the fix | (b) Build IceAndFire-CE's `1.21.1` branch at or after `f47956e2` and ship that jar: unreleased code in the pack. (c) Keep Ice and Fire 2.0-beta.17, the July version the host runs with 677 hippocampi: needs checking that the pack's other mods (rtsim's iceandfire plugin among them) accept it. (d) The rehearsal's KubeJS spawn cancel: no, it deletes the players' hippocampi, tamed ones too | The crash needs only a hippocampus in a loaded chunk with no water target, so the live server would die within minutes of the upgrade. A mixin is the smallest change and goes away when a release lands |
| DR2 | The host's heap (8.5) | `-Xms8G -Xmx8G` and a GC log, written after `setup_server.py` (8.3 step 4). Round 2 ran it (10.7); this row said `-Xms4G` where 8.5 said `-Xms8G`, and 8.5 is right | (b) `-Xmx6G`: more room for Elasticsearch and the page cache, 2.4 times the live set. (c) Leave the fixed 12 GiB | The live set is 2.3 to 2.5 GiB with or without rtsim; a 12 GiB heap on 16 GB with Elasticsearch puts the heap in swap, the likeliest cause of the weekly watchdog crashes |
| DR3 | The watchdog (`max-tick-time`) | Keep 60000 | (b) 300000, as the rehearsal's runs used on the laptop: fewer restarts, but a player waits up to 5 minutes in a hung tick. (c) -1, off | A tick over a minute is a hang that deserves a crash report and a restart; the fix is DR2, not a longer leash. Revisit if the crashes go on after DR2. Both rounds ran 300000 on the laptop, for the laptop's sake (10.1), not as a value for the host |
| DR4 | The pack's broken scripts and tag (6.2) | **Round 2: done** in pack aaed844, all four checked in the boot log (10.2). Round 1's recommendation was: fix them in the pack before the upgrade: `create_power_loader.js` without its top-level `return` (wrap the body in `if (Platform.isLoaded(...)) { ... }`), the missing `/` key in `radar_integration.js`'s two shaped recipes, `copycats:copycat_catwalk` out of Sable's physics tag, and mic-petrochem 1.0.1 (its thruster fuels' `consumption_multiplier` key) in place of 1.0.0 | (b) Ship as is: Create Power Loader's recipes are missing, the data link and auto yaw controller are uncraftable, Sable logs 3 errors, and neither petrochem fuel drives a Create Propulsion thruster | Pack bugs, not world damage; each is a one-line fix or a version bump in the pack repository |
| DR5 | Deploy order | The pack upgrade alone first, with the GC log; rtsim a week later | (b) Both in one maintenance window: one downtime, but any new problem has two suspects | The upgrade carries the blocker (DR1) and the heap change (DR2); a week on it says whether the watchdog crashes stop before rtsim adds anything |
| DR6 | What the sim sees around the main base (6.5, round 2: 10.4) | **Round 2:** keep the 8 districts `rtsim district adopt 16` makes since DS-10, the main base as three of them | (b) Merge the main base's three into one (`rtsim district merge` twice, where they stand): one factory, as a player sees it, and its Create logistics network in one district, but then too wide for the threat survey's perimeter cut (DR8). (c) Round 1's 54 small districts no longer arise: DS-10 seeds only machines | Split, nothing breaks: each part sweeps, solves its glass box in at most 32 ms and gets its perimeter cut; the logistics network that spans two parts only makes its gauges wait while one part is unloaded. Merge and split are both reversible commands, so this can change after the deploy |
| DR7 | The two KubeJS edits made on the live server (section 4) | Re-apply both after the upgrade and put the `borax` tag edit in the pack, so the next upgrade keeps it | (b) Drop them | `setup_server.py -y` overwrites them; the tag edit changes recipes, the other is a comment |
| DR8 | The threat survey's column cap (6.5) | **Round 2:** moot while the main base stays split (DR6). Merged, it is too wide again: then leave `surveyMaxColumns` at 40,000 and measure the survey's cost on the host before raising it | (b) Raise it until the main base fits, unmeasured | The main base is surveyed without its perimeter cut when too wide, which is a fallback, not an error |
| DR9 | The glass box on the host (6.7, round 2: 10.5) | **Round 2:** leave it on (the default). A solve of the merged main base takes 5.7 to 15.3 ms, of its largest part split 12.6 to 32.2 ms, bounded by its work budget, the engine skipped no hand-over and no stop failed the final save (R2-3) | (b) `enabled = 0` under `[industry.glass]` in `config/mic_rtsim-industry-server.toml`, round 1's recommendation while a solve took seconds: off, the sim runs on the black box alone and only the glass box's diagnostics are lost | The glass box is a read-only checksum today, now cheap; on, its checksum lines are the first sign of a wrong flow model on a real base (10.5 has two) |
| DR10 | What the sim builds in the players' villages (round 2: R2-5, 10.6) | Leave it on, and tell the players that the villages near them will start to build | (b) Switch the `settlements` module off: nothing built, but no phase 1 either (villages, markets, caravans). (c) A switch for building alone: code, not in rtsim today | It is the design: a village's works purse buys lamps, decor and golems (SINK.md §4.2) and Ribbit villages grow houses (C7). In round 2 the two adopted villages built three lamp posts within minutes, over air, grass and leaves only. The free-plot test refuses block entities and any block that is not air, a plant, leaves, water or floor terrain, but blocks a player placed before the install are not marked, so a player's flowers or leaf hedge in a village could be built over. Players will see it; Ben decides whether they hear first |

## 10. Round 2 (2026-10-23)

Ben's answer of 2026-10-23 (DECISIONS.md DR-0a): rehearse again now on today's mic-rtsim main and today's
pack, with the DR rows' recommendations in, and deploy later. Nothing was deployed. Sections 0 to 9 are
round 1 (2026-10-20 and 21); where round 2 changes a DR row or a runbook step, section 8 and 9 now say
so and point here.

### 10.0 Findings, by deploy risk

Verdict: **today's pack plus mic-rtsim main is ready to deploy once the format changes land**, after one
short repeat of the rtsim runs on the final formats (10.1). Nothing found in round 2 blocks it. The
runbook changed in three places (8.3 step 4 for R2-2, 8.3 step 5 for R2-7, and 8.6 for C8a and R2-9),
and Ben has two choices to make before rtsim goes on: DR6, changed by DS-10, and DR10, new (section 9).

- **R2-1. Resolved, the pack's blockers are gone (DR1, DR4).** mic_fixes 1.0.2 is in the pack and its
  hippocampus guard applies (`applying the null guard`, and the mixin's `Mixing
  HippocampusExplorationGoalMixin` line): 694 hippocampi in 618 force-loaded chunks of the full world
  ran 600 s with no crash (h-on, h-nodh), and the runs went without the KubeJS script round 1 needed.
  The negative control reproduced round 1's crash: the pruned world with the bases loaded and the guard
  switched off (`-Dmic_fixes.hippocampusGuard=false`, r-hoff) died 74 s after the last box was
  force-loaded, `Ticking entity`, the NullPointerException in `HippocampusExplorationGoal.tick`; with
  the guard on, the same world and boxes ran clean in all 11 runs. DR4's four fixes are all in the
  pack (10.2).
- **R2-2. High, the runbook: `setup_server.py -y` turns the host's heap into a fixed 12 GiB.** Its rule
  is `max(requested, old -Xms, old -Xmx)`, so the host's `-Xms8G -Xms12G` becomes `-Xms12G -Xmx12G`,
  not the 8 GiB of DR2. Step 4 of 8.3 now writes 8.5's file after the script. DR2's row also said
  `-Xms4G` where 8.5 said `-Xms8G`; both say `-Xms8G -Xmx8G` now (10.7).
- **R2-3. Medium, rtsim, solved: the glass box is bounded now (DR9).** The main base merged into one
  district (1,072 nodes, 1,699 edges, round 1's graph was 1,258 nodes) solves in 5.7 to 15.3 ms over
  three runs (r1-reload's 16 timed solves: p50 6.7 ms), stopped by its work budget after one round and
  not converged; round 1's solve took 2.4 to 5.4 s. Split, its largest part solves in 12.6 to 32.2 ms.
  In every rtsim run the engine skipped no hand-over in the sampling window (0 of 1,860 to 4,637), no
  stop failed the engine's final save, and the main base's glass checksum sampled its sweeps.
  DR9 now recommends leaving the glass box on (10.5).
- **R2-4. Medium, rtsim, Ben's choice: what the sim sees around the main base (DR6).** DS-10 seeds
  districts from machines only. The same two `district adopt 16` now make 8 districts, not 55: the main
  base is three of them (42, 37 and 8 chunks), and one Create logistics network spans two, so its gauges
  wait while either is unloaded. Two `rtsim district merge` commands make it one district (87 chunks,
  200 inventories), which is then too wide for the threat survey's perimeter cut again (DR8) (10.4).
- **R2-5. Medium, players will see it: the sim builds in their villages from the first minutes.** A
  village's works purse buys lamps, decor and golems (SINK.md §4.2). In the rtsim run the two adopted
  villages built three lamp posts (oak fence, stripped oak wood, wall torches) within seven minutes of
  the start. A block diff of the two village boxes against the pack run beside it shows they replaced
  only air, short grass and leaves. Blocks a player placed before the install are not marked
  (`PlayerBlocks` marks placements from the install on), but the free-plot test refuses any block entity
  and any block that is not air, a plant, leaves, water or, in the floor layer, terrain, so a player's
  flowers or leaf hedge in a village could be built over and little else. DR10 (10.6).
- **R2-6. Medium, the host: loading many far-apart chunks filled the 8 GiB heap.** The three hippocampus
  runs, which force-load 618 chunks scattered over the full world 50 at a time, filled the heap with
  humongous allocations as the batches loaded, failed evacuations (7 to 37 times) and paid 1 to 5 full
  collections of 1.0 to 2.4 s each, before settling at a live set of 3.4 to 3.7 GiB, with the guard on
  and off alike. Deleting Distant Horizons' 2.8 GB database did not help (h-nodh was the worst of the
  three), so the database's size is not the cause; DH's own work on the chunks is not ruled out, and
  which mod makes the humongous allocations was not measured. The pruned world's runs, 1,415 chunks in
  8 compact boxes and no DH database either, never did (live set 2.3 to 2.6 GiB). 8 GiB still holds the
  live set twice over; the GC log is how the host shows whether it happens there (8.5) (10.7).
- **R2-7. Low, the pack: Distant Horizons 3.1.2-b to 3.3.3.** At its first start it resets the host's
  `config/DistantHorizons.toml` (version 4) to 3.3.3's defaults, as it has no config updater: the
  host's two values that differ from a fresh file (`realTimeUpdateDistanceRadiusInChunks` 256 and
  `maxGenerationRequestDistance` 4096) equal 3.3.3's defaults, the `enableServerGeneration` and
  `enableDistantGeneration` keys are gone from version 5, and it picks a new `serverId`. It runs a SQL
  update on the LOD database (`0110-sqlite-addApplyToParentIndex.sql`), so the backup of 8.2 is the
  only way back to the July pack's DH. And it logs one ERROR at the first chunk load, before Chunky has
  started (`Chunky is not loaded`, from DH's Chunky hook; the other chunk load listeners ran). Runbook
  8.3 step 5.
- **R2-8. Low, rtsim's cost is still below what the laptop resolves.** The two pairs at a usable load:
  +0.76 ms and -0.08 ms mean tick with rtsim, against standard deviations of the paired difference of
  35.9 and 41.1 ms; boot, stop, resident memory and the live set are the same (10.3).
- **R2-9. Low, the rollback path: putting rtsim back loses every district (UR-1).** Save, stop, reload,
  uninstall and the sim reset all behaved (10.6): no stop failed the engine's final save, the reload
  restored every module from its pages, the uninstall left no rtsim id in the world, and the sim reset
  announced the 2 villages and 6 districts again. Putting the jars back on the uninstalled world is
  where it differs: rtsim does not see that the world ran without it (UR-1's detection is not built),
  restores its pages as they were, villages and herds included, and every district dissolves as its
  chunks load, without a log line, because the uninstall's save dropped their seed attachments (6
  districts at the start, 0 at the end). No block or block entity changed, and no reconcile replayed,
  but none was owed in this world, so UR-1's replay risk was not exercised. **Fixed** by the reinstall
  guard (UR-1, docs/choices-reinstall-guard.md): rtsim now sees the gap in `level.dat`'s mod list and
  resets the whole sim itself, adopting the stripped districts again as their chunks load. Rerun on a
  copy of r-uninstall's world (10.6): 6 districts kept, none dissolved. 8.6 says what to expect.
- **R2-10. What the new features showed without a player** (10.8): the syn200 commands work on the real
  base (two merges and a rescan); the ships module's `RTS4` pages and phase 1's `RTW4` pages were written
  and read back, but this world has no docks, so S8b's ports and havens had nothing to run on (`ships
  restored: 0 docks, 0 lanes, 0 ships, 0 havens`); TALK, the DH far view and Mowzie need a player or a
  client (Mowzie has no plugin yet; M1 is the engine's). Not checked: a client joining, skipped (10.9).
- **R2-11. Low, rtsim, for its owners: the first detection after a start takes one long tick.** On a
  world with districts, the session's first detection spent 0.68 to 2.2 s in one tick at boot (cold
  code; warm detections take at most 35 ms), and its slice budget counts block entities, not time, so
  nothing cuts that slice short.
  One slow tick before players are on; not a deploy blocker (10.6).
  **Fixed** with the reinstall guard (docs/choices-reinstall-guard.md, Part 3): the time went to chunk
  loads, not cold code. Two reads waited for a chunk that is due but still loading, while every other
  chunk load due ran: 534 to 972 chunk loads inside one look at one chunk. Create's fan links asked
  `Level.isLoaded`, which such a chunk passes, and read a block in it; and a vault's item handler, built
  the first time anything asks for it, read every block of the vault, one of them in such a chunk. Fan
  links now read only FULL chunks (RG15), a vault with a block in a chunk not FULL is described from its
  controller without its handler (RG17), and a detection's tick also stops at a time limit, 5 ms by
  default (RG12).

### 10.1 What ran

- **rtsim**: main at `4bc03a6d` (2026-10-23, with syn200's leg pages of `f6847446`), built with
  `-Prelease`: the core, 12 plugins (carryon, climate, create, destroy, dh, dieselgenerators,
  endertransmission, iceandfire, oreexcavation, powergrid, radar, ribbits) and mic_native, 14 jars.
  Every plugin under `neoforge/` whose mod is in the pack's index is in; mowziesmobs is in the pack but
  has no plugin at this commit, and dh's dependencies are all client side, so on a server it loads and
  does nothing. Mowzie M2 landed later (`89fb716f`, a new item in the catalogue, a format change): the
  deploy's jars come from main after the last format change, and the rtsim runs of 10.6 get one short
  repeat on them before the deploy.
- **The pack**: master at `aaed844` (2026-10-22): mic_fixes 1.0.2 (the hippocampus guard),
  mic_petrochem 1.0.2, Distant Horizons 3.3.3, Chunky on the server only, NeoForge 21.1.249. Exported
  with the packwiz the pack's cache needs (`packwiz-0-unstable-2026-02-18`; the flake's older one cannot
  read the cache) and set up with `setup_server.py` on a copy of the host's server folder, as 8.3 does.
- **The world**: round 1's pruned copy of the 2026-10-20 snapshot (17,779 chunks), not a fresh one: the
  host's `level.dat`, `playerdata/` and `data/*.dat` are owner-only, the regions had changed little,
  and the same copy compares directly with round 1. The hippocampus runs used the whole snapshot.
- **The JVM**: 8.5's arguments, `-Xms8G -Xmx8G` and the GC log, plus the laptop's stand-ins for the
  host's 4 cores (`-XX:ActiveProcessorCount=4`, `-Dmic_native.fgThreads=4`, `-Dmic_native.bgThreads=2`).
  Round 1 ran `-Xms2G -Xmx8G`.
- **`max-tick-time` 300000 on the laptop, not the host's value.** The laptop is shared with other agents'
  builds and GameTests at loads up to 80, where a single tick of a minute is the laptop's and not the
  server's, and a watchdog kill ends a measurement (round 1 lost 2 of 4 servers to it). The host keeps
  60000 (DR3); the long ticks are read from the `Can't keep up` lines instead.
- **The method** is round 1's (section 5): reflink copies, pairs started 5 s apart with the order swapped
  between pairs, simulation and view distance 16, the 8 force-load boxes 20 s apart, `rtsim village` at
  both villages and `rtsim district adopt 16` at both factories, 300 s of `neoforge tps` every 5 s, an
  entity census, `save-all flush`, stop, and the next step on a copy of the world it left. New: each
  sample also reads `rtsim perf` and `rtsim district info 1` (glass solves and hand-overs), every
  district's `info` at the end, and the hippocampus runs. Every boot held the shared heavy lock.

| Run | World | What | Exit |
|---|---|---|---|
| r1-pack, r1-rtsim | pruned | p1: the pack, and the pack with rtsim, pack first | 0, 0 |
| r2-rtsim, r2-pack | pruned | p2: the same, rtsim first; at a load of 46 to 49, not a measurement | 0, 0 |
| h-on, h-off | full | DR1: every chunk that holds a hippocampus force-loaded for 600 s, the guard on and off | 0, 0 |
| r1-reload | r1-rtsim's | reload; then the main base's three districts merged, the measurement, a rescan | 0 |
| r-uninstall, r-reset | r1-reload's | 8.6's uninstall; the sim reset (`world/rtsim/` deleted) | 0, 0 |
| r-reinstall, r-reset-reload | theirs | rtsim put back on the uninstalled world (UR-1); the reset world booted again | 0, 0 |
| r3-pack, r3-rtsim | pruned | p3: p1 again (pack first), as p2 measured the laptop | 0, 0 |
| r-hoff, h-nodh | pruned, full | round 1's crash conditions with the guard off; h-on without DH's database | crashed, 0 |

### 10.2 The pack

- **DR1.** See R2-1. Why h-off did not crash: a single force-loaded chunk ticks its entities only while
  they stay inside it, so no hippocampus could swim off into shallow water. Round 1's crash was in a
  chunk that holds no hippocampus in the snapshot, so the one that crashed had swum there, with the
  bases loaded and whole areas ticking. Those conditions with the guard off (r-hoff) crashed again,
  in a chunk other than round 1's, 5 minutes after the start; the server then hung until the
  watchdog ended it (300 s on the laptop, 60 s on the host). mic_fixes' own GameTests prove the crash
  and the guard in a shallow pool.
- **DR4, all four fixed**: `Loaded script server_scripts:create_power_loader.js` and no `invalid return`;
  `radar_integration.js` loads with no `Pattern references symbol '/'`; no Sable error on
  `copycat_catwalk`; mic_petrochem 1.0.2 loads with no thruster fuel parse error.
- **New log groups against round 1's upgraded pack**: DH's config reset and its Chunky ERROR (R2-7, the
  ERROR logged a second time by C2ME's chunk load invoker), and one petrolpark mixin WARN (`Shift.BY`).
  The expected ERROR groups of 6.2 are unchanged: 1,191 `iceandfire:portal_data` attachments, 148 `No
  key id` items, Cobblemon's old fossil starts.
- **mic_petrochem skips 10 of its reactions at every start**: 8 name Create Metallurgy items the pack
  does not have (`cobalt_dust` 4, `platinum_dust` 3, `nickel_ingot` 1), 2 name Destroy molecules that do
  not exist (`destroy:butane`, `destroy:propane`). Not new (round 1's upgrade logged them too); a pack
  bug for mic_petrochem's owners, with no effect on the world.

### 10.3 rtsim's cost (MSPT)

| Pair | Run | Boot | Done | Mean | SD | p50 | p90 | Cores | RSS | Stop | Load |
|---|---|---|---|---|---|---|---|---|---|---|---|
| p1 | r1-pack | 125.1 s | 29.1 s | 67.4 | 25.6 | 57.0 | 98.0 | 1.16 | 9,043 MB | 3.0 s | 12.2 |
| p1 | r1-rtsim | 134.7 s | 14.3 s | 68.2 | 27.7 | 57.5 | 88.9 | 1.14 | 8,983 MB | 3.0 s | 11.6 |
| p2 | r2-rtsim | 78.1 s | 13.2 s | 251.5 | 217.2 | 114.2 | 610.4 | 0.75 | 9,066 MB | 15.0 s | 45.6 |
| p2 | r2-pack | 78.2 s | 12.8 s | 288.2 | 242.7 | 115.8 | 701.1 | 0.71 | 8,790 MB | 17.7 s | 48.9 |
| p3 | r3-pack | 52.1 s | 10.4 s | 98.3 | 40.1 | 81.5 | 193.3 | 1.08 | 8,784 MB | 4.3 s | 18.8 |
| p3 | r3-rtsim | 56.1 s | 9.3 s | 99.5 | 38.5 | 82.0 | 183.9 | 1.08 | 8,784 MB | 4.3 s | 19.2 |

Milliseconds per tick over 300 s with the bases loaded. p1's rtsim minus pack per sample: mean +0.76 ms,
standard deviation 35.9, median +0.47 (58 samples). p2 ran at a load near 48 (other agents' work), its
ticks four times p1's, so it measures the laptop. p3 repeated p1 at a load of 19: rtsim minus pack
mean -0.08 ms, standard deviation 41.1, median +1.43 (57 samples). p3 kept p1's order (pack first), so
the order swap rests on p2 alone. rtsim's own counters: its server thread time was 3.3 s over 10,539
ticks in r1-rtsim (311 us a tick mean, p50 120 us) and 3.0 s over 8,516 in r3-rtsim (354 us, p50
159 us); the sweeps read 12,600 inventories in 199 ms (the slowest one 27 ms, open item 2 of 6.9).

### 10.4 What the sim sees (DR6, DR8)

- `district adopt 16` at the two factories took 596 and 202 seeds (round 1: 3,871 and 3,533, every
  block entity) and made 8 districts: 42, 37, 12, 8, 3, 2, 2 and 1 chunks, with 74, 91, 7, 35, 13, 0,
  1 and 4 inventories. The first factory's point itself is in no district now.
- The main base is districts 1, 2 and 4. A Create logistics network spans 2 and 4 (`its gauges wait
  while any of them is unloaded`, a WARN at detection).
- `rtsim district merge 1 2`, then `merge 1 4` (each waits for the detection the last started) made one
  district of 87 chunks and 200 inventories, revision 3. Round 1's main base was 166 chunks and 278
  inventories: DS-10 leaves out the storage and stray blocks round 1 counted.
- Split, no district is too wide for the threat survey's perimeter cut (709 and 686 columns for the two
  largest), so DR8 does not arise. Merged, the main base is (`too wide for the perimeter cut (40000
  columns at most): perimeter sent as 0`, twice), as in round 1.
- A `rescan` afterwards changed nothing (same chunks, inventories and revision).

### 10.5 The glass box (DR9)

| District | Nodes | Edges | Rounds | Solve |
|---|---|---|---|---|
| 1 (part of the main base) | 469 | 395 | 1, converged | 223 to 455 us |
| 2 (part of the main base) | 484 | 1,155 | 1, not converged | 12.6 and 32.2 ms (r2-rtsim, r3-rtsim) |
| 4 (part of the main base) | 123 | 95 | 1, converged | 139 us |
| 3, 5 to 8 | 2 to 55 | 0 to 54 | 1 or 2, converged | 36 to 196 us |
| 1 merged (1, 2 and 4) | 1,072 | 1,699 | 1, not converged | 5.7 to 13.2 ms, p50 6.7 ms (r1-reload, 16 timed); 8.2 to 15.3 ms (r-reset), 13.7 ms (r-reset-reload) |

- A solve that cannot converge now stops at its work budget (G2-58) and bounds the parts it did not
  finish (G2-59), where round 1's ran 278 rounds in 2.4 to 5.4 s. The engine's step waited on nothing:
  hand-overs waited 0 of 10,539 (r1-rtsim), 1 of 5,192 (r2-rtsim at a load near 48, none in the
  sampling window), 0 of 9,814 (r1-reload, merged) and 0 of 7,109 to 8,516 in r3-rtsim and the reset,
  reset-reload and reinstall runs. No final save failed in any run.
- The main base's checksum sampled its sweeps: 43 of 55 (r1-rtsim), 19 of 31 in the merged district's
  first window (it recompiled at each merge). District 4's window had not opened (`last discard
  recompile`), district 6 has no inventory to sweep.
- What the checksum says, for the glass box's owners: district 3's first window failed (lava measured at
  -947 a minute and shears at 0.4 a minute, both predicted 0), and the main base's black box at one node
  predicts flint and cobblestone unbounded where the sweeps measure -1 and 253 to 263 a minute. A
  diagnostic, read-only today: nothing a player sees changes.

### 10.6 Save, reload, uninstall, reset, reinstall

- **Reload** (r1-reload): `engine started: pages include observation 589`, every module restored from
  its pages in 0 ms (phase 1 at its world tick, 2 herds, 0 dragons, 0 ships, 0 factions, the world model
  of 88 cells), the 8 districts and 2 villages as they were, no re-announce. The pages hold `RTW4` and
  `RTS4`.
- **Village works** (R2-5): `houses 3 built / 0 refused` in r1-rtsim (the counter counts works too;
  round 1 built none), `1 houses the sim built` and `2` in the two villages; the block diff is
  `tools/deploy-rehearsal/blockdiff.py`.
- **C8a**: `the sim owns wandering traders now: vanilla's trader spawner is cancelled while trade and
  travel are on (doTraderSpawning is not touched)`, and `gamerule doTraderSpawning` read true in all 11
  runs that read it, with rtsim, after it and without it. 8.6 no longer has a gamerule step.
- **Census**, in all 11 runs on the pruned world: 12 villagers, 1 iron golem, no ribbit, 1,470 to 1,514
  entities, the same with and without rtsim. The snapshot's one wandering trader was there in every run
  from the pruned start and in the reload, and gone from the uninstall's world on (traders despawn).
  No chunk lost and the same 18,191 block entities in the kept chunks.
- **Uninstall** (r-uninstall, 8.6 on r1-reload's world): booted and stopped clean (exit 0, `Done` in
  13.6 s). 74 one-off `unknown or non-serializable data attachment` errors as the seeded chunks loaded
  (71 `mic_rtsim:industry_seeds`, 3 `mic_rtsim:player_blocks`; round 1 had 243, when adoption seeded
  every block entity). `idscan.py` finds no rtsim id in the world after the stop (it finds the same 71
  and 3 in r1-reload's world). `doTraderSpawning` read true.
- **Sim reset** (r-reset, `world/rtsim/` deleted from r1-reload's world): `engine started: pages include
  observation 0`, then both lines 8.6 asks for, `phase 1: the engine's pages lack observations the
  journal no longer holds (a sim reset?): 2 villages adopted again, 0 base marks to be made again` and
  `industry: ... 6 districts are announced again`. Phase 1 counted 2 villages in the engine, the
  districts swept and compiled their glass, no final save failed. Booted again (r-reset-reload), it
  restored from its new pages with no re-announce and the same 6 districts and 2 villages.
- **Reinstall** (r-reinstall, the jars put back on r-uninstall's world, UR-1): `engine started: pages
  include observation 1717`, every module restored as it was at the uninstall (2 villages with their
  works, 2 herds, 6 districts in the engine), no re-announce: nothing detects that the world ran
  without rtsim. `rtsim district list` showed 4 districts after the start and 0 at the end (`detections
  3`): each district dissolved as its chunks loaded with no seed attachment left, and no line in the log
  says so. `reconciles: 0 applied`, and the world's block entities and chunks match r-uninstall's
  (18,212 and 18,732, none missing).
  **Rerun with the reinstall guard** (2026-10-24, a copy of r-uninstall's world, the branch's build):
  the start logged `REINSTALL` and `REINSTALL RESET (UR-1)`, moved `world/rtsim/` aside, and the engine
  started over with a new engine life; `reinstall reset: 69 chunks of 6 districts are adopted again as
  they load`, all 69 already loaded at the session's start (796 machines adopted again, none found
  with its seeds kept). Phase 1 adopted the 2 villages again and the industry announced the 6
  districts again. `rtsim district list` showed the same 6 districts at 2, 3 and 8 minutes, with
  every base forceloaded, `reconciles: 0 applied, 0 duplicates`, and the first save deleted the
  marker. Started again (no gap), it kept the engine life and the 6 districts.
- **The first detection after a start takes one long tick.** Where districts exist at the start (the
  reload, reset and reinstall runs), the session's first detection looked at 271 to 305 block entities
  in one tick of 0.68 s (r1-reload, load 19), 1.33 s (r-reset, load 10) and 2.2 s (r-reset-reload, load
  36), with no GC pause over 0.3 s near it; in r1-rtsim and r2-rtsim, where every detection came minutes
  after the start, the longest tick was 16 to 35 ms. The core reads only loaded chunks (`getChunkNow`),
  so this was first read as cold code; it was chunk loads inside one look: Create's fan links asked
  `Level.isLoaded`, which a chunk still loading passes, and waited for that chunk, and so did a vault's
  item handler for a block of the vault in such a chunk (R2-11, fixed). One slow tick at boot, before
  players are on.

### 10.7 Heap and GC (DR2)

- With the bases loaded, the live set past 300 s was 2,387 to 2,597 MiB (p50 2,437) without rtsim and
  2,370 to 2,548 MiB (p50 2,393) with it; resident memory 9.0 GB. The longest young pause after boot was
  102 and 177 ms. At boot, with all 8 GiB committed from the start, G1's young generation is large and
  its first mixed collection took up to 2.27 s at a load of 33 (`4759M->1133M`). A full collection at the
  stop (`System.gc()`) took 0.66 s at a load of 12 and 6.1 to 6.6 s at 48.
- R2-6, the full world's runs (618 scattered chunks force-loaded 50 at a time, then 600 s):

| Run | Evacuation failures | Full collections (compaction) | Longest | Live set after | Mean tick |
|---|---|---|---|---|---|
| h-on (guard on) | 12 | 2, at 385 and 398 s | 2.43 s | 3,450 to 3,558 MiB | 16.7 ms |
| h-off (guard off) | 7 | 1, at 381 s | 2.21 s | 3,452 MiB | 16.2 ms |
| h-nodh (guard on, no DH database) | 37 | 5, at 168 to 263 s and one in the stop's save | 2.28 s | 2,533 to 3,701 MiB | 12.1 ms |

  The pruned world's runs had no evacuation failure and no full collection but the stop's
  `System.gc()`. Resident memory peaked at 8.0 to 9.6 GB.
- R2-2: `setup_server.py -y`, run as 8.3 runs it on a copy of the host's server folder, wrote
  `-Xms12G -Xmx12G` into `user_jvm_args.txt`, as it keeps the largest heap it finds (requested, old
  `-Xms`, old `-Xmx`). 8.3 step 4 now replaces the file with 8.5's lines after the script.

### 10.8 The new features

| Feature | What round 2 saw |
|---|---|
| Ports and havens (S8b) | The ships module is on and its `RTS4` pages were saved and read back; this world has no docks, lanes or havens, so nothing ran |
| TALK slices 1 to 3 | Needs a player talking to a villager or ribbit: not exercised |
| Mowzie M1 | Engine and JNI only at this commit, no plugin: nothing a server without players shows |
| DH far view | `far feed: 0 subscribers, 2 sources`: the server side is up, a DH client is needed to subscribe |
| syn200's commands (DS-12 to DS-16) | adopt, merge (twice) and rescan on the real main base, as 10.4 says; `split` not run, as the merge answered DR6's question |
| `RTS4`, `RTW4` | Written by r1-rtsim and read back by r1-reload, as 10.6 says |

### 10.9 Not checked

A client joining (the stretch goal) was skipped: it needs an offline-mode copy of the server, the pack's
client mod set, a GPU on a shared laptop and the heavy lock, a project of its own. So still unchecked,
as in round 1: a player login (inventories, ender chests, Cobblemon, a player aboard a Sable ship),
puppets, TALK, the far view, ports with players, and the host's own load. The first login after each
step of the runbook is the check.

The optional pair of the July pack beside today's (`campaign2.sh july`) was not run either: round 1
measured the July pack, and nothing round 2 found in today's pack needed it again.
