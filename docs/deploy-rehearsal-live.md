# Deploy rehearsal: mic-rtsim on a copy of LiveSrv

Rehearsal of DECISIONS.md SRV1: copy Ben's live MIC server (LiveSrv) read-only, prune the world to what
players built, boot it locally on the old pack, the current pack, and the current pack with mic-rtsim, and
write down what a real deploy needs. Run 2026-10-20 and 21 on the laptop against a snapshot taken
2026-10-20.

Private data stays private: the world copies, logs and player files live under `~/mic-livesrv/` on the
laptop and are not in this repository. The tools are in `tools/deploy-rehearsal/` (stdlib Python, no
private data in them; the base coordinates and the seed are read from private files).

STATUS: DRAFT, measurements pending.

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
argument fix in the runbook (section 6) is worth doing whether or not rtsim goes in.

The 3,199 chunks that still carry `eclipticseasons:biome_holder` are the pattern any removed mod with
chunk attachments leaves behind: an ERROR line each time such a chunk loads, until the chunk is
saved again. rtsim's uninstall leaves the same kind of trace (section 4).

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

The rehearsal therefore ran on a synthesized `level.dat` (the server writes a fresh one from
`level-seed`, recovered read-only as the seed the chunks were generated with). What that costs:

- game time and day time restart at 0, world spawn is recomputed, game rules and the datapack list are
  defaults, seasons restart;
- trains: the carriage entities are in the world but Create's train registry is not, so trains do not
  come back, and their absence says nothing about the upgrade;
- Sable sub-levels: the `.slvlr` region files are present, but the occupancy, tracking and ticket
  files are not, so a Sable failure in the rehearsal may be an artifact of the copy;
- no player inventories, so nothing in a player's inventory or ender chest was checked, and
  Cobblemon's party and PC stores (present in `world/pokemon/`) only load when their player joins,
  which nobody did.

A tarball of those files made by the owner (`/tmp/mic-private.tgz`) never appeared, so these gaps stand.

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

<!-- RUNBOOK DRAFT START -->
## 8. Runbook

For Ben, or an agent Ben gives sudo on LiveSrv (the rehearsal had read access only). Facts it relies on:
the service is `minecraft-server`, runs as `mcserver` in `/var/lib/mc/server`, has `Restart=always`
with `RestartSec=10`, and the world's `level.dat`, `playerdata/` and `data/*.dat` are readable by
`mcserver` only. The disk had 347 GB free on 2026-10-21. Never stop the server with `/stop` in
its console for maintenance: systemd starts it again 10 seconds later. Use `systemctl`.

### 8.1 Build on the laptop

1. Pack: export the mrpack from a pack checkout that has the raw jars `index.toml` lists (Destroy among
   them; check each with `sha256sum` against `index.toml`), and copy `scripts/setup_server.py` with it.
2. rtsim: in the mic-rtsim repository, `tools/natives/build-natives.sh --only linux-x86_64-gnu` in
   mic-native's dev shell, then `cd neoforge && ./gradlew -Prelease :core:jar` plus one `:<plugin>:jar`
   for each of the eleven plugins (carryon, climate, create, destroy, dieselgenerators,
   endertransmission, iceandfire, oreexcavation, powergrid, radar, ribbits). Terra Entity and Sandworms
   are not in the pack: leave them out.
3. mic_native: its jar from mic-native (`mic_native-1.21.1-0.1.0.jar`); it carries baseline and
   x86-64-v3 builds and picks v3 on the host's AVX2 CPU.
4. Copy the mrpack, `setup_server.py` and the 13 jars to the host (`scp -P 23 ... carol@host:/tmp/`).

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
4. JVM arguments: replace `user_jvm_args.txt` (section 8.5). The installer leaves it alone.
5. `sudo systemctl start minecraft-server`; follow `logs/latest.log` to `Done`, then compare it with
   section 6.2's list of expected errors. Let one player log in and check an inventory, an ender
   chest, the Cobblemon party and PC, a train and a Sable ship: the rehearsal could not check any of
   them (section 2).

### 8.4 Install rtsim

1. `sudo systemctl stop minecraft-server`.
2. Copy the 13 jars into `mods/` as `mcserver`.
3. Start. In `logs/latest.log` look for `mic_native natives: status=OK ... build=x86-64-v3`,
   `plugin mic_rtsim 0.1.0 loaded`, `session started: native engine`, `6 structures are villages the
   sim adopts`, and `the sim owns wandering traders now: doTraderSpawning false`. No line may say
   `Mixin apply` or `InvalidInjectionException` (every rtsim mixin is required, so a failed one stops
   the server with a crash report).
4. Districts: from the console (`sudo -u mcserver screen -r minecraft`, detach with Ctrl-A D),
   `execute positioned <x> <y> <z> in minecraft:overworld run rtsim district adopt 16` at each factory.
5. Watch for a day: `/neoforge tps`, `/rtsim perf`, the size of `world/rtsim/`.

### 8.5 JVM arguments

JVMARGS-PLACEHOLDER

### 8.6 Rollback

- **rtsim only** (keep the new pack): stop; delete `mods/mic_rtsim*.jar` and `mods/mic_native-*.jar`;
  start; run `gamerule doTraderSpawning true` (rtsim turned it off and nothing turns it back on once
  rtsim is gone); `world/rtsim/` can stay (nothing reads it) or go. Expect section 6.6's one-off
  `unknown or non-serializable data attachment mic_rtsim:*` errors.
- **Everything**: stop; `sudo mv /var/lib/mc/server /var/lib/mc/server-failed`; `sudo mv
  /var/lib/mc/server-backup-<date> /var/lib/mc/server`; start.
- **Sim reset** (SRV2, when a save format change has no migration): stop; delete `world/rtsim/`;
  start. Section 6.7 says what a reset leaves behind.
<!-- RUNBOOK DRAFT END -->
