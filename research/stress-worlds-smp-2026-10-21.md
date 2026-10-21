# Stress-test worlds, SMP follow-up survey (DECISIONS.md row ST1)

2026-10-21. Research only, a follow-up to `stress-worlds-2026-10-21.md` (the first
survey, 7 worlds, not repeated here). Ben asked for ultra large Create worlds, SMPs
ideally, with public downloads. Nothing was booted or upgraded. Only this report, the
derived audit JSON and the tooling are committed. The world itself lives in
`~/mic-stress-worlds/tbsmp-season2-1.21.1/world/` (outside every git tree), private
local testing only, never redistributed.

Anything not checked against a file, a jar or a live page is marked UNVERIFIED.

## 1. Bottom line

1. **There is a real one: TBSMP Season 2.** A 144-mod, Create 6.0.6 NeoForge 1.21.1
   SMP world, 1,041 players over its life, public on archive.org (8.67 GB zip, 13.5 GB
   extracted, no login). It dwarfs everything in the first survey on every axis except
   train count (the PR #9513 test still has more trains):
   - 2,097,750 chunks in 11 dimensions, 3,454,117 block entities (138,125 Create,
     112,429 of them in the overworld),
   - 55 trains, 101 stations, 97 track graphs, 6,199 nodes, 140.4 km of track,
   - Create 6 logistics at SMP scale: 591 packagers, 450 package frogports, 21 stock
     tickers, 18 redstone requesters, 48 packager links, 9,193 item vaults, 1,339
     mechanical crafters, 12,835 fluid tanks, 17,675 belts, 301 stationary
     contraptions in the overworld.
   It is the only public world found that is both a multi-player SMP and a Create 6
   world. It is also on the right version (1.21.1, DataVersion 3955, Create 6.0.6, four
   patch releases behind the pack).
2. **The catch is the pack.** It is a Create plus horror-minigame pack (The Broken
   Script, a CoD-style dimension mod, FramedBlocks, Create Casing, Lootr, Sophisticated
   Storage). 9 of its 11 dimensions are mod dimensions (or old copies of them) the MIC pack does not have, and
   most of the overworld's 633,008 block entities are not Create (263,395 FramedBlocks,
   160,393 vanilla, 33,421 Lootr). On MIC's pack, unknown blocks are lost (missing
   registry entries), Create blocks survive. Import by deleting the foreign
   dimensions and loading with a minimal mod set, see section 5.
3. **Everything else found is much smaller.** Other big modded SMP worlds exist (ATM10
   Sen's Network S2 and S3, SkippyZii S1 and S2, The Funnier SMP, 5 more) but their
   Create networks are tiny (1 to 4 trains). They are large worlds, not Create worlds,
   see section 4.
4. **Nothing was gated that a payment would unlock** beyond what the first survey
   already listed. The Just Create SMP world and the Create-team seasons were looked
   for and no public world was found. Nothing was paid for or asked for, see section 3.

Recommendation: use TBSMP S2 as the "ultra large SMP world" fixture (ST1 second half),
with the PR #9513 world for train scheduling and Mysda for logistics, as in the first
survey. The thousands-of-machines half is now partly answered: TBSMP S2's overworld
has 112k Create BEs and 9k item vaults, which is real player-built industry.

## 2. Ranked table

Same shape as the first survey. Rank 1 here would also have been rank 1 or 2 in the
first survey's table on size.

| # | World | Source | MC / loader / Create | World size | Trains / carriages | Stations / signals | Track | Create BE / all BE | Upgrade cost |
|---|-------|--------|----------------------|-----------|--------------------|--------------------|-------|--------------------|--------------|
| 1 | TBSMP Season 2 (The Broken Script SMP) | archive.org `world_202606` | 1.21.1 / NeoForge / 6.0.6, 144 mods | 8.67 GB zip, 13.5 GB, 2.10M chunks | 55 / 85 | 101 / 12 signals (173 signal blocks) | 140.4 km | 138,125 / 3,454,117 (overworld 112,429 / 633,008) | medium: foreign dims and blocks dropped |
| 2 | ATM10 Season 2 (Sen's Network) | archive.org `world_20260120` | 1.21.1 / NeoForge / ATM10 pack, 504 mods | 12.1 GB zip (sampled, 8 chunks regions only) | 4 / 10 | 19 / 11 signals | 42.0 km | sample 1,554 / 26,505 | heavy: 504-mod pack, AE2, Mekanism |
| 3 | SkippyZii SMP S2 | archive.org `skippy-zii-smp-s-2` | not read (UNVERIFIED) | 23.3 GB zip | tracks file 6.8 KB zipped | not read | not read | not audited | not assessed |
| 4 | ATM10 Season 3 (Sen's Network) | archive.org `world_20260630` | 1.21.1 / NeoForge / 505 mods | 9.3 GB zip (sampled) | 1 / 5 | 1 / 0 | 0.6 km | sample 281 BE | not worth it |
| 5 | SkippyZii SMP S1 | archive.org `skippyzii-smp-minecraft-world` | not read | 36.6 GB zip (over the 25 GB cap) | tracks file 2.3 KB zipped | - | - | - | - |
| 6 | The Funnier SMP 1.3 to 1.6 (5 archives) | archive.org `the-funnier-smp-imastego` | not read | 2.6 to 2.9 GB each | tracks file about 11 KB zipped | - | - | - | - |

Rows 3, 5 and 6 are rejected on the size of `create_tracks.dat` alone (zipped sizes
read over HTTP range requests, no download). For comparison TBSMP S2's file is 483,734
bytes zipped and the ATM10 S2 file is 77,667. Rows 2, 4 are real SMP worlds with a
sampled audit only (the first 8 region files, not the whole world), shown for scale.

## 3. What was searched, and what was gated

### Searched

| Source | Method | Result |
|--------|--------|--------|
| archive.org | `advancedsearch.php` with 14 query shapes, about 2,023 Minecraft-related items; for every zip over 100 MB the central directory was read over HTTP range requests and scanned for `create_tracks.dat` / `create_logistics.dat` (no download) | 9 hits in 5 items, see section 4. TBSMP S2 is the only one with a large network |
| CurseForge Worlds | WebFetch of the search pages | The `search=` parameter is ignored (the page returned the same 20 generic worlds, 6,548 total, none Create). Not usable. UNVERIFIED that nothing relevant is listed |
| CurseForge modpack pages | web search: Just Create SMP, Just Create SMP 2, Create SMP, Create: The World SMP, The Create SMP (TCS), Create Above and Beyond, Create Astral, CABIN | modpacks and server packs only, no world saves. Above and Beyond has one single-player let's-play world on Dropbox (1.16.5, a one-player world, not an SMP) |
| GitHub | REST search API (unauthenticated): repos with world, SMP, Create terms; spot checks of `underscoreBon/MCSMPWorlds`, `Omegabird113/LlamaWorld`, `beastSMP-prominence2`, `0800hakari/dimora-minecraft-server` | two small personal worlds with no Create evidence, a modpack, no Create SMP world. The last two were not examined in depth (UNVERIFIED) |
| Hugging Face | datasets/models API search | nothing |
| YouTube | yt-dlp descriptions of Create SMP world and "world download" videos, scanned for archive/drive/mediafire/mega links | links either already in the first survey, gated, or to modpacks |
| Web search | Planet Minecraft, reddit, Lemmy, Mastodon, Bluesky, forum phrasing for "SMP ended world download" | no new Create SMP world. SMPEarth (2020, not Create) is the only server-archive-style hit |
| Create World SMP wiki | direct fetch | HTTP 503, no data |

### Gated or not found, nothing paid for, no sign-ups

| What | Why not obtained | Price / cost |
|------|------------------|--------------|
| Just Create SMP Season 1 world (MrBeardstone) | Only the modpack is public (CurseForge, NeoForge 1.21.1, Create 6.0). No public world link was found. Any world release would be in his Discord or Patreon, not requested. UNVERIFIED that one exists | unknown |
| Just Create SMP Season 2 | modpack only (CurseForge, updated 2026-08-23), season seems current | unknown |
| Create-team / Create Live style SMP seasons | no released world found anywhere searched | n/a |
| sbeev "Create'a Colony" 4000-day world | already in the first survey: Patreon only | Patreon tier, see first survey |
| Create SMP servers (create-smp.com, TCS, The World SMP) | live servers, no downloads offered. Server pack is mods only | n/a |
| polartt mega.nz link | not opened (mega needs a client for large files, owner and size unknown) | free if it works, UNVERIFIED |
| Skippy S1 (36.6 GB) | public, but over the 25 GB cap and the network is tiny | free |

## 4. The audited world, and rejected worlds

### 4.1 TBSMP Season 2 (downloaded, extracted, audited)

- Source: https://archive.org/details/world_202606, uploaded by a season 2 player, dated
  2025-12-18, description: "the world download I was given after season 2 of the
  broken script smp ended". Public, no login. `world.zip`, 8,673,941,831 bytes, sha1
  `5116c86c9c705df0c8f86e5a5516f53f341719d0` (the sha1 was not re-checked after
  download, but the extract finished with no CRC error). The zip was deleted after
  extraction to stay under the 25 GB cap; the extracted world is at
  `~/mic-stress-worlds/tbsmp-season2-1.21.1/world/world/`.
- Versions: DataVersion 3955 (1.21.1), NeoForge, WasModded, game time 215,764,499
  ticks (about 125 days of server uptime), 1,041 playerdata files. Create 6.0.6.
  144 mods in `level.dat`, relevant ones: createcasing 1.7.1-fix2, createframed 1.6.3,
  rechiseledcreate, sophisticatedstoragecreateintegration, sophisticatedbackpacks
  createintegration, createeggproduction, createfood, chunky 1.4.23, ftblibrary,
  ftbessentials. Also the horror dimension mods (The Broken Script, CoD mod).
- Audit: `research/stress-worlds-2026-10-21-data/audits/tbsmp-s2.json`, produced by
  `audit_world.py` (`python3 -I`, 10 workers, 127 s, 0 region read errors).
- Tracks (`create_tracks.dat`, 483,734 bytes, DataVersion 3955): 55 trains, 85
  carriages (111 small bogeys, 27 large), 5 trains with schedules (21 entries), 1
  moving at save, 7 derailed, 97 graphs, 6,199 nodes, 6,185 edges, 101 stations, 12
  signals, 173 signal blocks, 140,358 blocks of track, all `create:andesite`, no
  curved edges. Largest graph: 4,510 nodes, 66 stations, 11 signals, 101.0 km.
  The dimension palette includes 4 non-vanilla dimensions (codmod, thebrokenscript),
  which dimension each train sits in was not extracted (UNVERIFIED).
- Chunks and block entities per dimension:

  | Dimension | Chunks | BE total | Create BE |
  |-----------|--------|----------|-----------|
  | overworld | 427,130 | 633,008 | 112,429 |
  | the_nether | 274,037 | 23,841 | 1,192 |
  | codmod:backrooms | 280,196 | 2,208,794 (2.2M ceiling lights) | 698 |
  | codmod:entropy | 239,112 | 15,308 | 4,744 |
  | thebrokenscript:clan_void | 217,874 | 449,199 (chests, furnaces) | 1,817 |
  | thebrokenscript:day_a | 188,669 | 1,247 | 82 |
  | thebrokenscript:day_b | 254,319 | 116,905 | 17,163 |
  | thebrokenscript:null_torture | 154,750 | 1,611 | few |
  | 3 `OLD...` copies | 61,663 | 4,204 | tiny |
  | total | 2,097,750 | 3,454,117 | 138,125 |

  The player-built Create content is in the overworld, day_b and entropy; the rest is
  generated minigame terrain (UNVERIFIED, inferred from the block entity mix). Excluding the backrooms lights, Create is about 11% of
  the block entities.
- Create content (all dimensions): 17,675 belts, 12,835 fluid tanks, 9,193 item
  vaults (8,222 overworld), 9,970 fake tracks and 2,297 real track BEs, 3,846 drills,
  1,435 steam engines, 1,339 mechanical crafters, 1,267 deployers, 1,841 chassis,
  243 mechanical bearings. Logistics: 591 packagers, 450 package frogports, 48
  packager links, 21 stock tickers, 18 redstone requesters, 3 stockpile switches, 4
  track stations. `create_logistics.dat` is only 11 KB. Contraption entities: 301
  stationary in the overworld, 12 in entropy, 13 in day_b (one moving contraption).
  4,833 super glue entities.
- Entities: 185,892 in total, 103,311 items, 9,608 striders, 6,252 piglins, 6,120
  lootr minecarts. No live Create carriage entities were counted separately.
- Heavy non-Create content that would be lost on MIC's pack: 263,395 FramedBlocks BEs
  in the overworld, 2.2M `codmod:ceiling_light`, 34,571 `thebrokenscript:console`.

### 4.2 Rejected, with evidence

| World | Why rejected |
|-------|--------------|
| ATM10 S2 (12.1 GB, 504 mods) | Sampled (first 8 region files, 8,107 chunks, `audits/atm10-s2-sample-8k-chunks.json`): 4 trains, 19 stations, 1,372 nodes, 42 km, 1,554 Create of 26,505 BEs. A big-mod-pack SMP, mostly AE2, Mekanism and Domum Ornamentum. Only worth it as a generic "huge SMP chunk count" fixture. Not downloaded, 12 GB for little Create |
| ATM10 S3 (9.3 GB) | One train, one station. Not downloaded |
| SkippyZii S1, S2 | Create files of 2.3 KB and 6.8 KB zipped (S2 also has a 267-byte logistics file). Almost no Create. 36.6 GB and 23.3 GB |
| The Funnier SMP 1.3 to 1.6 | `create_tracks.dat` about 11 KB zipped in all five versions: a handful of tracks. 2.6 to 2.9 GB each. Not downloaded |

## 5. Using TBSMP S2 as a stress world

Suggested use, nothing here was done:

1. Copy the world into a run directory (reflink), never write into
   `~/mic-stress-worlds/`.
2. Only the overworld and nether can load on MIC's pack: the other 9 dimensions
   (codmod, thebrokenscript and the three `OLD*` copies) need their mods' dimension
   types. Delete or move them. That keeps 0.66M of the 3.45M block entities and about
   113.6k of the 138k Create ones, and drops the Create work in day_b (17.2k) and
   entropy (4.7k) unless those mods are kept. One variant with the full mod set and
   one minimal variant would be the useful pair.
3. Boot with Create 6.0.10 and the minimal mod set (FramedBlocks, Create Casing,
   Lootr optional). Unknown blocks become air with NeoForge's missing registry
   warning. Create tracks and trains persist because they are in `create_tracks.dat`,
   which has the Create DataVersion 3955. The graph palette entries for removed
   dimensions will be dropped, UNVERIFIED how Create 6.0.10 reacts to a palette entry
   without a dimension.
4. Expect the train count to survive only if their graph dimension is a kept one. Check which dimension holds each train before building a test
   around it (the audit does not say).
5. One chunk-load caveat: `data/chunks.dat` and `chunkloaders_loaded_chunks.dat` are
   present, which means FTB or Chunky style forced chunks may fire on load.

## 6. Items for Ben

- **Choices I made** (all undoable): downloaded TBSMP S2 only (8.7 GB, then the zip
  deleted); did not download any ATM10, Skippy or Funnier world; did not request
  Discord or Patreon access to Just Create SMP; counted ATM10 S2 from a 8-region sample
  instead of the whole world; kept the audit tooling and new probe scripts in the
  data folder.
- **Total on disk** from both surveys is still under the 25 GB cap: this survey added
  13.5 GB (the extracted world), the first survey about 2.3 GB.
- **Not yet done**: nothing was booted, so "does mic-rtsim cope with 2.1M chunks and 55
  trains" is not answered. The one thing that would add data without a download is
  asking the Just Create SMP author whether a Season 1 world is public.
- **Decision for Ben**: should the stress harness take TBSMP S2 as a fixture despite
  its mod set? The alternative is a generated world (first survey, section 7).

## 7. Method and tools

- Scripts in `research/stress-worlds-2026-10-21-data/`: `audit_world.py` and
  `summarize.py` from the first survey, plus new `rzip.py` (reads a remote zip's
  central directory over HTTP range requests, so `create_tracks.dat` can be probed
  without downloading), `iagather.py` (collects archive.org item ids), `iascan.py`
  (scans them for Create files, prints `HIT` lines), `brief.py` and `brief2.py`
  (condensed audit summaries). `iascan.py` reads `/tmp/ia_ids.json` written by
  `iagather.py`.
- Audit run: `python3 -I audit_world.py <world> out.json 10` via `py.sh`.
- Downloaded world data is untrusted and was only parsed, never executed.
