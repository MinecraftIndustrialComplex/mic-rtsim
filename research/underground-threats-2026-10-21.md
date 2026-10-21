# Persistent underground threats near oil (research, 2026-10-21)

Research only. Nothing was run, built or added to the pack; every cost is read from code, a config comment or
an issue tracker and is labelled. **V** means read in code or data, **U** means not verified, **E** means my
estimate. Written for Ben (OG-9d: "anything that could function as persistent threats underground which would
plausibly be more likely to happen near oil could work"). Follows `research/worm-mods-2026-10-20.md` and
`research/worm-mods-round2-2026-10-21.md`.

## 1. Short answer

- **No mod off the shelf fits.** About 38 candidates were read (section 4). None is bound to a place, tied to
  oil, hurts underground Create machinery on purpose and spares well pipes. The two real diggers, Arachnids
  and Zombie Break & Build (ZBB), chew through whatever is on their path, well pipes included, and need
  mixins before they are safe to put near a well (section 5.3).
- **The sim already supplies the hard parts**: persistence while chunks are unloaded, placement, escalation,
  and damage chosen by Rust from a surveyed set (a block the survey never lists can never be chosen, so
  sparing the well pipes is a survey exclusion). What it lacks is a threat class that a perimeter or a roof
  does not stop, a record for the thing that sends threats, a survey flag for "underground", and a reason
  for a district to be attacked from below (section 3.1).
- **Recommendation: the oil nest (section 5.1).** One small Rust record per drilled oil field. It escalates
  with time and with how much the field has been pumped out, and it sends a new `Burrowing` threat at
  underground works inside its territory. Unwatched, the existing bounded `damage()` breaks surveyed machine
  blocks. Watched, a few vanilla husks cut tunnels with a plugin goal that skips the well column. It adds no
  jar, entity, block or item (D3 clean), costs nothing for an unloaded field, and keeps mob count small
  (D1). **8 to 12 agent-days (E); a first slice in 2 to 3.**
- **Runners-up, in order:** the IaF Death Worm as the nest's body (the original worm image, same engine, +2 to
  4 days); a **blowout** hazard from pieces the pack already has (H2S gas plus ignition, 6 to 10 days); a
  plugin-owned **creep** of vanilla blocks (5 to 8 days); Arachnids or ZBB as bought parts (not recommended).
- **Three pack data bugs found on the way** (plural tag folders, an unprefixed `fuel_type` folder, the wrong
  crude id in OILGATE), described in section 8. They bite any oil-tied design, so they are worth fixing
  whichever option is taken.

### Top picks at a glance

| Rank | Pick | Footprint added | Persistence | Performance risk (U, reasoned) | Effort (E) |
|---|---|---|---|---|---|
| 1 | **Oil nest, vanilla husk body, plugin boring goal** (5.1) | 0 jars, 0 entities, 0 blocks, 0 items | Rust record; none needed from chunks | low: a few mobs, one record update per sim step | 8 to 12 days; slice 2 to 3 |
| 2 | **Death Worm body on the nest** (5.2) | 0 (IaF in pack) | same record | low to medium (IaF worm not profiled) | +2 to 4 days over rank 1 |
| 3 | **Blowout: H2S pocket and ignition** (5.4) | 0 | Rust record per field | low if rtsim-owned; polling must use the cheap getter | 6 to 10 days |
| 4 | **Creep of vanilla blocks** (5.5) | 0 | Rust record plus idempotent stamping | medium: stamp budget per tick | 5 to 8 days |
| 5 | **Arachnids or ZBB, bought** (5.3) | 1 mod, AzureLib (Arachnids) | colony NBT or none | high underground (Arachnids), unknown hook cost (ZBB) | 3 to 8 days and a fork |

## 2. How this was done, and the gaps

- **Seven reading agents, then three verifiers.** Seven Sonnet agents swept: in-pack mobs (Cataclysm,
  Mowzie's, Bosses of Mass Destruction, Ice and Fire CE, CreateDragonsPlus; then Born in Chaos, Mutant
  Monsters, Nyf's Spiders, L2 Hostility and vanilla), colony and digging mods, burrowers and tar creatures,
  spreading infections, oil-tied hazards already in the pack, and gas, cave-in and collapse mods. They read
  decompiled jars (`cfr`, `javap`, `jadx`) and source clones; nothing was run. Three Opus agents then
  re-read the load-bearing claims independently (section 7 lists what changed).
- **Catalogs:** the Modrinth catalog without a version or loader filter (the round 2 dump of 77,405 mods and
  15,245 datapacks, plus about 4,500 hazard-sweep hits and about 2,200 infection-sweep hits), GitHub
  clones and short code searches, a few web searches. Jars of 40 or so projects were unzipped and read.
- **Downloads** are in `/tmp/underground-threats/` (untrusted, read only; `/tmp` is not durable).
- **Gaps:** CurseForge-only mods are mostly invisible (no API; web snippets only). PlanetMinecraft, the
  MCreator gallery, Reddit, Bukkit and Paper plugins (the pack is NeoForge) were not searched beyond
  names. Cataclysm was decompiled partly (Maledictus, Harbinger, Leviathan, Ignis, Wadjet, Coralssus and
  the Ancient Factory mobs were not read in detail), Mowzie's Naga, Bluff, Elokosa, Umvuthana, Foliaath,
  Wroughtnaut and Sculptor were only grepped, the eight IaF mixins were not read. **No performance was
  measured anywhere.** Every cost statement below is read from code, a config comment or an issue
  tracker, and says so. No candidate was run, built or put in the pack.
- **Not examined:** Midnight Lurker NF (246 MB jar), Mob Block Breaking (1.20.1 Forge, 10 KB), Caves &
  Creatures, There's something in the caves, Cave Dweller NeoForged, Mutationcraft, The Hordes, Immortuos
  Calyx, Wild Aside, Slime Minibiome, Gigeresque. Leads seen only as search results: The Locust, Scarab,
  Scarab Family Origins, Your Ant Friend, Realistic Ants, Companions antlion, Groth Mobs scarabs, Bugs
  Aplenty, Wilder Wild termites.

## 3. Facts that shape every option

All V unless marked.

### 3.1 What the sim already gives, and what it lacks

- **Threats are plugin-supplied.** INDUSTRY.md §13.2: a threat is a sim agent with a class, a strength,
  booked legs and a target rule; "plugins supply the bodies and the reasons". `Threat{source, class,
  count, strength, target, at, loads}` in `crates/rtsim-industry/src/threat.rs`. Sources today:
  `Natural, Dragon, Bandit, Boss, Synthetic`. Classes today: `GroundMelee, GroundRanged, Flying, Siege`.
- **Damage is the sim's, chosen by Rust from the surveyed set.** `damage()` (threat.rs, around line 447)
  picks blocks only from `def.exposed`, ordered by a gate flag (`PERIMETER` for walkers, `ROOFLESS` for
  flyers), capped by `destroy_cap_blocks`, with natural mobs breaking nothing unless `mobGriefing`. The
  survey (`SurveyJob.readBlockEntities`, Java) builds that set from the district's machine and inventory
  **block entities** only. Java then breaks each chosen block, if its state still matches, with
  `level.destroyBlock(pos, true)` (`Reconciler.java:314`), so contents drop. Any lost machine stops the
  abstract run (I23). Fire is never placed. A per-class cooldown paces attacks (I25).
  **Consequence for pipes: a block the survey never lists can never be chosen.** Sparing well pipes is a
  survey exclusion, not a filter on a foreign mod.
- **What is missing for an underground threat.** (a) A class that is not stopped by a perimeter or a roof
  (`walks()` is true for everything but `Flying`; a burrower would be neither). Classes are array indices
  (`ThreatClass::COUNT`, `cooldown_until`, `carry_per_attacker`, `blocks_per_attacker`, and
  `IndRecords.ThreatClass.COUNT` on the Java side), so a fifth class touches the wire fixtures. (b) A
  source variant. (c) A survey flag for "underground, no sky" blocks. (d) A persistent record for the thing
  that sends the threats (a nest, or a pocket). Nothing in INDUSTRY.md §13 gives a district a *reason* to be
  attacked from below; §13.4 natural pressure counts dark spawnable surfaces. (e) A defence measure
  against burrowers; today's measures (perimeter, roof cover, turrets, kill machines at gaps, guards) do not
  apply (U on which would; see section 5.1).
- **Persistence is already solved for creatures the sim owns.** The Ice and Fire dragon plugin keeps an
  abstract agent and materialises a puppet when chunks load (DragonBridge pattern). A nest or pocket would
  be the same kind of record. It would need no forced chunk loading and no SavedData in a mod.

### 3.2 The well column (what must never be hit)

- **How a well is checked (V, CDG 1.3.15, read twice).** `PumpjackHoleBlockEntity.tick` runs every 20
  ticks and scans straight down from the hole. A block counts as pipe if it is a subclass of vanilla
  `PipeBlock` with both `UP` and `DOWN` set (Create's `FluidPipeBlock` is one, and so is vanilla
  `ChorusPlantBlock`), an `EncasedPipeBlock` or `ConcreteEncasedFluidPipeBlock` with `UP` and `DOWN`, a
  `GlassFluidPipeBlock` on the Y axis, or a block in `#createdieselgenerators:pumpjack_pipe`. That tag holds
  only an optional `tfmg:industrial_pipe`, and TFMG is not in the pack, so it is effectively empty. The
  first non-pipe block must be in `#createdieselgenerators:oil_deposit`, which is `minecraft:bedrock` only.
- **The protected set** is the hole, plus `below(1)` to `below(pipeLength - 1)`, plus the bedrock under it.
  The hole can be anywhere in the chunk; the oil amount comes from the chunk, not the hole.
- **A cut pipe is cheap for the player.** The next scan marks the well invalid. No oil is lost and the chunk
  is not reset. That is why sparing the column is a courtesy to the design, not a safety need: a threat that
  cuts it only costs the player a repair.
- **Pipes cannot be told apart by id.** The well pipe is `create:fluid_pipe`, `create:encased_fluid_pipe`,
  `create:glass_fluid_pipe` or one of 16 `createdieselgenerators:*_concrete_encased_fluid_pipe`. Every other
  Create pipe is the same block, so a rule keyed on block id (ZBB's `blockHealthOverrideMap`, a tag) cannot
  spare well pipes without sparing all pipes. **The column test has to be positional**: walk down from each
  `pumpjack_hole`, or read `pipeLength` from the hole.
- Every well passes through the deepest layer (OILGATE §3.1), which is why the oil is reachable from below
  at all, and why a deep nest is *under* every well.

### 3.3 Oil and fire in this pack

- **Which oil comes out (V, corrected in the second pass).** The CDG pumpjack takes the first fluid of
  `#createdieselgenerators:pumpjack_output`, which CDG defines as `createdieselgenerators:crude_oil`. The
  pack's override of that tag sits in `pack/kubejs/data/createdieselgenerators/tags/fluids/` (plural), and
  tags load only from `tags/fluid` since 1.21, so **the override never loads** and the pumpjack gives CDG's
  crude. mic_petrochem's distillation takes `#c:crude_oil` (CDG ships that tag with CDG crude), so the chain
  still works. Destroy's crude is a `VirtualFluid` with no world block, and
  Destroy's pumpjack and seismometer are voided (`voided_content.js`).
- **Oil is desert-only** (`#c:is_desert`, kubejs `oil_biomes` replace). CDG's crude is a real fluid with a
  block (`blockstates/crude_oil.json` in the jar) but is not one of CDG's five fuel types (biodiesel,
  gasoline, diesel, ethanol, plantoil), so **crude is probably not part of the chain explosion** (V on the
  listing, U on the chain's exact rule). Whether vanilla fire ignites it is U (not read).
- **Refined fuels can burn.** CDG's own `GameEvents.onExplosion` chains blasts through any placed fuel block
  (config "Combustibles blow up", default true, kept by the pack). mic_petrochem's fuel definitions are in
  `data/createdieselgenerators/fuel_type/`, but CDG's own are in `data/createdieselgenerators/createdieselgenerators/fuel_type/`
  and a modded registry's data loads from a folder prefixed with the registry's namespace. So petrochem's
  kerosene, premium and cheap fuels **probably do not load as CDG fuels** (V on the folders and the loading
  rule, U at runtime); CDG's diesel and gasoline do. Create's open pipe ends place source blocks
  (`pipesPlaceFluidSourceBlocks`, default true), so a player's own fuel line can make a pool underground.
- **In Control spawns nothing below y 15** (OILGATE §3.2), and dragons do not fit in tunnels, so a player who
  drills from below meets none of the desert gate. Ice and Fire's griefing is off in the pack, so dragons
  break nothing. Whether In Control or L2 Hostility touch a plugin's own spawns depends on the spawn reason
  the plugin uses (U; vanilla's warden uses `MobSpawnType.TRIGGERED`).

### 3.4 What the pack has for underground hazards: nothing

24 keywords (subsidence, sinkhole, firedamp, cave-in, blackdamp, landslide, rockfall, fire spread and
others) matched against every entry of all 166 pack jars returned nothing real (the hits were Cobblemon
particle names, a Lithium mixin and Cataclysm's `EarthQuake_Entity` boss attack). There is no gas pocket,
no collapse, no underground fire, no brine. The pieces it does have, usable by a plugin:

| Piece | What it gives | Public seam (V) |
|---|---|---|
| Destroy pollution | smog, greenhouse, acid rain, radioactivity per chunk; a tell on the Pollutometer | `PollutionHelper.changePollution(Level, BlockPos, PollutionType, int)`, `isPollutionEnabled()` |
| Destroy chemical hazard | poison, nausea, crying, with gas-mask protection | `ChemistryHazardHelper.damage(Level, LivingEntity, FluidStack mixture, boolean skin)` |
| mic-petrochem H2S | `mic_petrochem:hydrogen_sulfide`, tags `acutely_toxic`, `smog`, `acid_rain`, in the heavy crude cut | data only, no Java hazard code |
| CDG fuel chain | one blast converts one placed fuel block into the next blast | any explosion within 2 blocks of a fuel block |
| CDG oil chunks | per-chunk oil amount in mB, depletion stored (`Integer.MAX_VALUE` is infinite) | `OilChunksSavedData.getChunkOilAmount / setChunkOilAmount` (cheap); `getBaseOilAmount` builds a `PerlinNoise` per call (never scan with it); **no extraction event** (mixin on `pumpjackRotation`, or poll the hole's `oilAmount`, `started`, `pipeLength`) |
| CBC block damage | persistent crack overlays, destroyed at 10 scaled parts | `CreateBigCannons.BLOCK_DAMAGE.damageBlock(...)`, saved in `SavedData` |
| Power Grid overheat | power-1 explosion of an over-temperature electrical device | `ThermalBehaviour.setTemperature`, electrical machines only |
| Explosion filter | `ExplosionEvent.Detonate` hands out the **live** block list and entity list before any damage; removing entries spares them (also from the client packet). Destroy `SmartExplosion`, CDG, Create and CBC explosions all pass through it | `getAffectedBlocks()`, `getAffectedEntities()` |
| Vanilla | `Level.explode` (fires `Start` and `Detonate`), `FallingBlockEntity` (hurts living entities only, never replaces a block or block entity), `minecraft:light` markers, fluid ticking | NeoForge events |

## 4. Candidates, all families

Columns: **Fit** is my one-word read against the target (persistent, underground, oil-tied, hurts player
works, spares pipes, small, potato-safe). "Filter" says whether block damage can be bounded by tag or id.
Estimates are labelled E. Nothing here was run. Licences: code / assets where they differ.

### 4.1 Family 1: mobs that live underground or burrow

| # | Candidate | Platform, licence | Footprint | Persist, steer | Hurts machinery; filter | Performance | Fit |
|---|---|---|---|---|---|---|---|
| 1 | Vanilla spawner block (`BaseSpawner`) | vanilla | one block entity | NBT saved; place from code (`SpawnerBlockEntity.getSpawner`, or a full NBT load); `custom_spawn_rules` skips the light check; `MobSpawnEvent.PositionCheck`, `FinalizeSpawnEvent` | no; depends on the mob | **no spawn counter** (V), and the nearby cap counts the exact class in a 9x9x9 box, so a digger that leaves the box is replaced: cap with `FinalizeSpawnEvent.setSpawnCancelled`. Needs air in its 9x3x9 spawn box. Freezes while unloaded | **optional building block** (a visible "heart" to destroy) |
| 2 | Vanilla trial spawner | vanilla | one block entity | config is **inline NBT** in 1.21.1 (no setter; set by NBT load); counters `total_mobs_spawned`, `current_mobs` | no | ignores light but **needs line of sight to the player, so one sealed in rock never activates** (V) | unusable for a nest |
| 3 | L2 Hostility 3.0.18 section levels | LGPL-2.1, in pack | 0 added | `SectionDifficulty.getLevelEditor(level,pos).setBase(n)`, per 16^3 section, saved with the chunk | no (arena trait is an anti-build aura, not breakage) | interval-gated | **amplifier** |
| 4 | Born in Chaos 1.7.6 cave mobs (Mother Spider, Nightmare Stalker, hounds) | ARR | in pack | global spawn gamerules; a plugin can `EntityType.spawn` any of them | doors only | MCreator spawn procedures per check, U | no |
| 5 | BIC Diamond Termite + infected ore | ARR | in pack | trap bound to an ore block | no (does not tunnel) | tiny | no |
| 6 | Mutant Monsters 21.1.1 | AGPL code, assets ARR | in pack | 5% of vanilla weight | creeper `MOB` explosion, enderman block grab (block-entity blocks spared) | spawn event loops all entities | no |
| 7 | Nyf's Spiders 3.0.1 | ARR | in pack | modifier on vanilla spiders | breaks nothing | 2,683-line custom pathing, U | **multiplier** only |
| 8 | Cataclysm 3.33 Amethyst Crab | LGPL-3.0, assets ARR | in pack | lush caves | no block damage | modest | no |
| 9 | Cataclysm block-breaking bosses | as above | in pack | arena or surface; most structures disabled | yes, `ChargeBlockBreaking`, only tag-immune blocks spared | heavy | no |
| 10 | BOMD Void Blossom | LGPL-3.0 | in pack | stationary 350 HP boss, never despawns, cavern at about y -29; placeable via summon block or `/summon` | places moss and vine walls with **no replaceability check**, so it would overwrite a pipe in its arena (radius U) | one boss, Light block per tick | no |
| 11 | IaF dragon cave and sleeping dragon | LGPL-3.0, in pack | in pack | `underground_structures` y -24 to +5, home-bound, persistent | pack griefing false, so none | heaviest entity (multipart, 5,000-node pathing) | no |
| 12 | Mowzie's Grottol | custom, in pack | in pack | cave prey | none | trivial | no |
| 13 | IaF Death Worm (round 2) | LGPL-3.0, in pack | 0 added | plugin-owned puppet with `noPhysics` (round 2) | sim-owned damage | 0.03 to 0.05 ms per worm for Terra Entity; IaF U | **creature body** (section 5.2) |

Verdict for the family: no mob here tunnels (V), none breaks Create machinery on purpose (V), none is tied
to oil, none is bound to a place. The vanilla spawners and L2 Hostility are the useful parts.

### 4.2 Family 1 and 2 outside the pack: diggers and colonies

| # | Candidate | Platform, licence | Footprint | Persist, steer | Hurts machinery; filter | Performance | Fit |
|---|---|---|---|---|---|---|---|
| 14 | **Arachnids** (AzureDoom) | 1.21.1 NeoForge 0.3.0 (2026-05-08, untouched since; 0.1.0 to 0.3.0 in one week); MIT | 5 entities, 0 blocks, 1 mixin (adds a method only), AzureLib dep (not in the pack), 4 biome modifiers | colony in the `BrainBug` NBT, in-memory manager rebuilt on load; a plugin-placed underground brain works only with hand-written `Colony` NBT | **warriors break almost any solid block on their chase path with no tag, mobGriefing or claim check** (`MoveToTargetAction.tryBreakBlockingPathBlock`, drops items; pipes and machines included); workers dig only `#arachnids:weak_blocks` (no deepslate, no Create); no `BreakEvent` fires | A* capped at 2,000 nodes (hard-coded), repath every 10 ticks, per-tick squad scans; **colony layer is largely broken** (phase bug, self-match discard, worker stall); natural spawns in deserts, i.e. on the oil fields | **needs a fork** (section 5.3) |
| 15 | Zombie Break & Build (ZBB) | 1.21.1 NeoForge 1.7.0; LGPL-3.0 | 0 entities, 0 blocks, 4 mixins | goals added to every `PathfinderMob`, gated by `affectedEntityIdList` (config, whole-segment wildcards); **nothing per mob is saved**, only block storages (off by default) | breaks any block under a hardness cap and drops it; spares only **exact ids** (no tags, no block-entity filter), and well pipes share ids with all Create pipes; fires no cancellable event | `LevelChunk.setBlockState` hook runs on every block-type change server-wide, a through-blocks ray with no length cap that reads blocks synchronously; default notice limit is 3 blocks, so a mob 20 blocks down does **not** dig up | **behaviour layer, needs 2 mixins** (section 5.3) |
| 16 | Myrmex in "Ice and Fire: Dragons Port" (ExtraTortik) | 1.21.1 NeoForge 2.1.13 port.1; LGPL-3.0 | 48 classes, same modid as the pack's IaF CE | `SavedData` hives, persistent | foragers take only plants (`myrmex_harvestables`); no | `MyrmexAIForage` scans about 18,000 positions per search | no (does not wreck machines; modid clash) |
| 17 | Orpiment Tar Cube | 1.20.1 Forge only; MIT code, asset licence U | slim fork: 1 entity, 1 fluid, 1 effect | size in NBT, plugin persists | no; applies TARRED | `Monster`, 700 lines | **tar-themed body**, needs a port (E: 2 to 4 days) |
| 18 | Silent Caves golem | 1.21.1 NeoForge 1.1.0, 50 KB; jar Unlicense vs store ARR | 2 entities, 0 data | no spawn rules; plugin spawns; looks like the block under it | no | one `Monster` | body (licence conflict) |
| 19 | Zombie Tactics | NeoForge 1.21.1, 24 KB; GPL-3.0+ | 1 mixin | zombies only | hardness check only, no block-entity filter | goal per zombie | pattern |
| 20 | The Apocalyptic Zombies | ARR, 45 KB | small | zombie horde | `CaveTunnelGoal` skips block entities, hardness cap 3.0 | goal | pattern |
| 21 | Alex's Caves Corrodent | 1.20.1; LGPL-3.0-only, 75 MB | huge; excluded by Ben at P8 | digs by phasing (`DiggingMoveControl`), removes nothing | no | 25 mixins | record only |
| 22 | The Mole (horror), Spooki's Burrowing Creeper, Kreebles, The Hive, ArPhEx, Termites plugin, Antarchy, Cave Dweller ReEvolved | various | various | collapse timers, teleport-under-ground fakery, ambush blocks, cosmetic tunnelling | mostly no | various | **patterns**, not parts |
| 23 | Digger Mobs datapack, Hole Diggers, Sulfur Slimes | ARR | | `fill air destroy` around every entity every tick; self-discarding toys; power-20 TNT explosions | unfiltered | bad | rejected |

### 4.3 Family 2: spreading and colony threats

| # | Candidate | Platform, licence | Footprint | Persist | Hurts machinery; filter | Performance and griefing history | Fit |
|---|---|---|---|---|---|---|---|
| 24 | Sculk Horde, branch `1.21.1` | NeoForge beta, no release jar; Apache-2.0 | 562 Java files, up to 200 mobs, GeckoLib | `ModSavedData` nodes, real blocks, real/virtual cursors | skips any block entity (`isExplicitlyNotInfectable`); cannot damage machines without a fork | closed issues: TPS lag, corruption crash; **forces chunk loading** (radius 3 by default); fix "nodes building supremely fast" on 2026-10-09 | rejected (sculk look, weight) |
| 25 | Spore (Fungal Infection) | 1.21.1 NeoForge 2.2.0j; ARR | large, mob-heavy | Mound NBT, `SporeSavedData` chunk tickets | whitelist conversion only | Protos created permanent forced chunks (fixed in 2.0.6); "Lagless" build exists elsewhere | rejected |
| 26 | Nexus Infection | 1.21.1 NeoForge 1.6.4; store ARR vs jar AFL-3.0 | 847 classes, 215 blocks, 89 entities | blocks plus one global flag; no forced loading | **no block-entity guard**, immune tag lists no Create block, so it overwrites machines | up to 4 `getEntitiesOfClass` of 100-block boxes per spread attempt | rejected (D3) |
| 27 | Crimson Curse | 1.21 datapack and mod; ARR | 941 functions, reskins zombies | scoreboards, biome data; markers killed when chunks unload | tag-listed blocks | **about 30 unconditional global `@e` selectors every tick** | rejected |
| 28 | Desertification, Brainium, The Corruption (Nider001), SRP | various | | forced-chunk tickers; Brainium converts blocks under hardness 2.5 incl. Create; roots force-load (cap 32); SRP 1.12 only | | forced chunk loading is the common grief source | reference only |
| 29 | **Plugin-owned creep** (design, no mod) | vanilla blocks | 0 | Rust record plus idempotent stamping on chunk load | `mic:colony_replaceable`, `mic:colony_immune` (pipes), `mic:colony_eats` | stamp budget per tick | **option**, section 5.5 |

### 4.4 Family 3: hazards that need no creature

| # | Candidate | Platform, licence | Footprint | Persist, steer | Hurts what; filter | Performance | Fit |
|---|---|---|---|---|---|---|---|
| 30 | **Collapsing Caves 1.7** (CCDelic) | 1.21.1 NeoForge, Fabric, Forge; **CC0**, 205 KB | 3 server mixins, 0 blocks, 0 entities, 1 enchantment, 4 commands; **required on clients** (required payloads, a synced sound registry, a synced entity data field) | in-flight cave-ins live in static maps, **cleared on server stop**; the only creator is a player breaking a block (`CaveInManager.onBlockBroken`); a plugin can build and tick its own `CaveInEvent` but skips the concurrency cap, cooldown and warning | falling blocks hurt entities (2 per block fallen, cap 6) and never drop; **only natural stone and soil fall, never a Create block or pipe** | BFS budgeted per tick; caps 3 events and 600 falling blocks; 9 closed issues, none about lag | **adds little** (section 5.4): rocks that hurt players, nothing else |
| 31 | Sable: Collapsing Caves | NeoForge 1.21.1, requires Sable (in pack); CC0 | 2 mixins | debris becomes Sable sub-levels | as above | sub-level physics cost U; the pack already pays for Sable | variant |
| 32 | MineCollapse 2.0.0 | NeoForge; EUPL-1.2 | own blocks, items, recipe types, 3 mixins | per-level attachment | no | `NeighborNotifyEvent` hook on every block update | rejected (D3, cost) |
| 33 | EnviroMine Lite 1.1.3.1 | NeoForge; store CC-BY-NC-SA vs jar ARR | 107 classes | burning coal ore seam fire, depth toxicity | entities only | U | rejected (licence); idea reusable |
| 34 | Pollution of the Realms 9.1.12.0 | NeoForge; store CC-BY-ND vs jar ARR | 873 KB, own pollution system | persistent gas blocks, explode on ignition (`GasExplosion.in(level)...tryExplode()`) | explosions break blocks; `maxChainLength` bounds | every gas block random-ticks, ignition check scans 26 neighbours, U | rejected (licence, scope) |
| 35 | GasCraft 1.0.4 | NeoForge source only (repo created 2026-10-15); LGPL-3.0 | 80 Java files, weapon gases | cloud entity, data-driven gases | entities only | BFS per cloud | rejected (unreleased) |
| 36 | Seismica 1.7.0 | NeoForge; store MIT vs jar ARR | 1.2 MB | `SeismicaAPI.startEarthquake`, saved quakes | surface blocks, non-container machines near surface | **force-loads chunks on the server thread** (`forceLoadDamage`, default true) | rejected |
| 37 | mge, Plenum, Hazardous, Simple Block Physics, Cave-Ins datapack, Thick Air family, Create: Smog and others | various | | | | | patterns or rejected |
| 38 | **Pack parts: Destroy pollution, chemical hazard + mic-petrochem H2S, CDG chain explosion and oil chunks, CBC cracks, Power Grid overheat** | in pack | 0 added | plugin-owned record | explosions break blocks; poison hurts players | cheap, U measured | **hazard part** (section 5.4) |

### 4.5 Family 4: combinations

| # | Combination | Parts | Footprint | Persist, steer | Hurts what; filter | Performance | Fit |
|---|---|---|---|---|---|---|---|
| 39 | **Nest, husk body, plugin boring goal** | rtsim record, vanilla `Husk`, plugin goal | 0 | Rust `Nest` record, sim-placed | surveyed machine and inventory blocks via `damage()`; pipes excluded from the survey and from the goal | a few mobs | **rank 1** (5.1) |
| 40 | Nest with the IaF Death Worm body | rows 13 and 39 | 0 (IaF in pack) | as 39; puppet with `noPhysics` | as 39 | 1 worm, multipart (U) | **rank 2** (5.2) |
| 41 | Nest with amplifiers | rows 1, 3 and ETF reskin on 39 | 0 | spawner as a visible heart; L2 section level; reskin by height or name | as 39 | cheap | **add-ons to 5.1** |
| 42 | Nest and blowout on one drainage clock | rows 38 and 39 | 0 | one `Field` record, stage raises both | explosion plus tunnelling | sum of both | **later**, shares 5.1's record |
| 43 | Nest plus a plugin cave-in on tunnel collapse | row 39 and vanilla `FallingBlockEntity` | 0 | plugin-owned, idempotent | falling stone hurts players, never machinery | bounded falling-block count | **cheap add-on** (E 1 to 2 days) |
| 44 | Nest with Arachnids as the body | rows 14 and 39 | 1 mod, AzureLib, a mixin addon | colony NBT plus the nest record | needs 6 mixin injections to spare pipes | heavy underground | **not recommended** (5.3) |
| 45 | Seep creep from the nest chamber | rows 29 and 39 | 0 | one record, two behaviours | machines "eaten", pipes immune | stamp budget | **later** |


## 5. The ranked options

Ranked on Ben's own rule (D1: late-stage runtime on a weak PC), then on fit to the ask (persists, lives
underground, more likely near oil, hurts underground Create works, spares well pipes), then on D3 (no bespoke
content). Estimates are in agent-days (E) and cover engine, plugin and GameTests, not review. Subsection numbers
are not ranks; the rank is in each title.

### 5.1 The oil nest, with a plugin-owned digging body (rank 1, recommended)

**What it is.** A small persistent record in the sim, one per oil field, that owns everything the
candidates above could not give: where the threat lives, how it grows, when it attacks and what it may
break. The *body* is replaceable (a vanilla husk first, a desert mob that does not burn; the Death Worm later, 5.2).
It adds no jar, no entity type, no block and no item (D3 clean). Block vocabulary is vanilla.

**How it works as an rtsim threat near oil.**

- **Persistence.** Rust owns `Nest { id, site, stage, strength, regrow_at, last_attack, seed }`. It is
  placed deterministically on the first load of an oil chunk cluster (the same once-per-chunk-ever pattern as
  OILGATE §4.6's placed feature; CDG's oil amount is computable for any chunk, OILGATE §3.1), 20 to 60
  blocks below the surface and inside the rock under the field. It sleeps at stage 0 until a pumpjack hole
  exists in the cluster.
  It keeps existing while chunks are unloaded, advances in simulated time, and needs no forced chunk
  loading and no mod SavedData (the common grief source in Sculk Horde, Spore, Desertification and
  Nider001, rows 24 to 28 in section 4.3).
- **Oil tie.** Escalation reads two things: time since the nest was last cleared, and how much of the
  field has been pumped out (`OilChunksSavedData.getChunkOilAmount` against `getBaseOilAmount`, read
  while the chunk is loaded and stored in the record). Stage rises with drainage, so the more a player
  pumps the angrier the field gets; a field nobody drills sleeps. A stage 0 field is invisible.
- **Source of threats.** A nest of stage n sends `Burrowing` threats (a new fifth class and a new
  `ThreatSource`) at any district whose surveyed works come within its territory, at a rate and strength
  from the stage, paced by the existing per-class cooldown (I25).
- **Unwatched, offline.** The sim resolves the attack (hazard or Lanchester, I22) and chooses blocks with the
  existing `damage()`: from the survey, ordered by a new `SUBTERRANEAN` flag (a machine or inventory
  block entity below the local surface with no sky access) and then `MACHINE`, capped by
  `destroy_cap_blocks`. Carry is zero (burrowers steal nothing; natural mobs already take none). On the next
  load the Java side breaks each chosen block whose state still matches (`level.destroyBlock(pos, true)`),
  so vaults and tanks drop their contents. Any lost machine stops the run (I23).
- **Watched.** When a player is in or near the district, the plugin materialises puppets from the record with
  `EntityType.spawn` (the DragonBridge pattern), so **no spawner is needed**. Husks get
  `PersistenceRequired`, a tag and a follow-range attribute, and are capped by the plugin. L2 Hostility's
  section level (`SectionDifficulty.getLevelEditor(level,pos).setBase(n)`, per 16x16x16 section, saved with
  the chunk) scales their stats, and ETF can reskin them below a height or by name (both in the pack, V).
  A plugin goal then cuts a tunnel toward the target a few blocks at a time: every 5 ticks, if the path is
  blocked, it takes the next 1x2 cell on the line, removes it only if it is in a `#mic_rtsim:burrow_diggable`
  natural-rock tag, is not a block entity and is not in the protected well column, and never more than a
  few blocks per second. (Patterns: the Apocalyptic Zombies `CaveTunnelGoal` skips block entities and
  caps hardness at 3.0; ZBB's `ClearObstaclesToTargetTactic`; both read, neither used.) Real damage while
  watched is the same bounded breach the abstract path would have chosen, applied as the puppet reaches it.
  Mob griefing stays off. This is the pattern of `worm-mods-2026-10-20.md` section 4.2 (encroachment, puppet, steer, bounded
  damage, retreat).
- **An optional, visible heart.** A vanilla monster spawner stamped in a small chamber gives the player a
  block to find and destroy (a `BlockEvent.BreakEvent` on it clears the nest). It must be capped, because
  a monster spawner **has no spawn counter** and counts only the same class inside its own 9x9x9 box, so
  anything that tunnels out of the box is replaced: `FinalizeSpawnEvent.setSpawnCancelled` is the clean cap
  (V). A **trial spawner is no use here**: it needs line of sight to a player, so one sealed in rock never
  starts (V).
- **Escalation and retreat.** Kills while watched lower the nest's strength (observed fights calibrate, I22).
  A cleared nest regrows after `regrow_at` (TUNABLE), at stage 0 if the field was drained to nothing. No
  global spread; a hard cap on nests per world.

**What it breaks.** Machine and inventory block entities of the district below the surface: pumps, tanks,
shafts and cogs with block entities, drills, vaults, belts with block entities (U which Create blocks the
survey classes as `MACHINE`). Plain casings and blocks without block entities are not in the surveyed set today (whether to widen it is
section 8.2, item 3).

**How pipes are spared.** Three layers, each cheap. (1) The survey never lists blocks of the well column
(computed from every `pumpjack_hole` down to bedrock, section 3.2), so `damage()` cannot choose them. (2) The
plugin's tunnelling goal skips that set and any block outside the diggable tag. (3) The mob's own
`mobGriefing` stays off. A cut pipe only invalidates the well until it is repaired (section 3.2), so the exclusion is a courtesy,
not a safety need. It also reads right: the nest is after the *machines*, and the well is what feeds it.

**Cost and risk.**

| Item | Estimate (E) |
|---|---|
| Engine: fifth class, source variant, survey bit, tunables, wire fixtures, tests | 2 to 3 agent-days |
| Nest record, placement, stage and regrow rules, boundary kinds, oil-drain read | 2 to 3 |
| Plugin: chamber stamp, puppets and caps, boring goal, survey exclusion, kill hooks | 3 to 4 |
| GameTests (placement, survey exclusion, abstract and watched damage, pipe safety, persistence across reload) | 2 |
| **Total** | **8 to 12 agent-days** |
| Smallest first slice: a loaded-only chamber, husk puppets, boring goal and column exclusion in a GameTest, no sim | 2 to 3 |

Performance (U, reasoned, not measured): no pathfinding beyond vanilla's for a few mobs; a spawner is one
block entity; the boring goal does a bounded box test every few ticks per puppet; the survey is the existing
job plus one `canSeeSky` per block entity it already visits; nothing runs for an unloaded field except
one record update per sim step. Risks: (a) a vanilla mob in a freshly cut tunnel can path badly or get stuck
(U; needs the GameTest), and In Control or L2 Hostility may act on a plugin's own spawns depending on the
spawn reason used (U), (b) the survey today lists only block entities, so a base made of casings and
conveyors without block entities is unharmed (design choice), (c) new engine class touches the wire
format and fixtures (E included), (d) no defence measure for burrowers exists yet (section 8.2, item 4).

### 5.2 The Ice and Fire Death Worm as the nest's body (rank 2)

**What it is.** Round 2's pick, now with something to be the body *of*. The IaF Death Worm (LGPL-3.0, already
in the pack, so 0 added) is driven by a plugin as a `noPhysics` puppet: it moves through rock without removing
it, and the damage is the sim's, chosen from the survey as in 5.1. It is the nearest thing to the Terraria
worm Ben first asked for. Round 2 sized the plugin at 400 to 700 lines and 2 to 4 days; the fallback there was
a slim Terra Entity fork with new art (rejected in OG-9c, so only if IaF's worm fails).

**What the nest changes for it.** On its own the worm had no reason to be near oil, no persistence and no
bound (OILGATE §4.6: pack stats are 10 health and 3 attack, sand-only swimming, desert-only world generation).
On the nest it gets all three: the record places it, escalates it and bounds what it may break. The husk and
the worm are interchangeable bodies behind the same `Burrowing` threat.

**What it costs beyond 5.1.** E: +2 to 4 days (plugin body, attribute scaling, ETF or model tweaks if the
look needs it, GameTests). Performance is U: round 2's 0.03 to 0.05 ms per worm was measured-by-code for Terra
Entity, not for IaF's multipart entity, and the eight IaF mixins were not read (section 2). **Why not first:**
it adds a dependency of the plugin on a mod's internals (allowed by D4) and a cost nobody has measured, for a
look the husk body can stand in for during the first slice.

### 5.3 Bought diggers, Arachnids or ZBB (rank 5, not recommended)

Both are real digging engines and both fail the same test: they do not hunt machines and they do not know
what a well pipe is.

**Arachnids 0.3.0** (MIT, NeoForge 1.21.1). What a plugin would get: a persistent colony in NBT, workers that
dig shafts, warriors that chase through rock, a boss-bar, and a virtual population while unwatched.
**What the second pass found:**
- Warriors break almost any solid block on their chase path with no tag, mobGriefing or claim check
  (`MoveToTargetAction.tryBreakBlockingPathBlock`, `CrawlToTargetAction`; only negative hardness and
  collision-free blocks are safe). Well pipes and machines go as readily as dirt, as items.
- A plugin-placed underground brain is deleted on its first tick once any other colony exists (a self-match in
  `isTooCloseToAnotherColony`), its workers stall after one batch (the dome builder never finishes), its
  tunnel plan points up toward daylight, and the 1 Hz colony phase check mostly never passes, so
  defence and regen rarely run. All verified in code.
- Its A* is capped at 2,000 nodes per search, hard-coded, with a repath every 10 ticks and per-tick squad
  scans; underground, targets are usually unreachable, so capped searches are the normal case (cost per
  search U, my estimate 1 to 3 million block-state reads). It is a week-old rewrite untouched since
  2026-05-08, depends on AzureLib (not in the pack), carries Starship Troopers names in its metadata and
  spawns brains and warriors in desert and badlands, which is where the oil is.
- **To use it:** a mixin addon with about six injections (a `mic:bug_proof` check at the head of
  `canBreakPathBlock` and `canBreakDownPathBlock` in two classes, a phase fix on `now` in `BugColony.tick`,
  `@ModifyConstant` on the 2,000 cap and the 10-tick interval), a datapack (more entries in
  `#arachnids:weak_blocks`, the biome modifiers set to `neoforge:none`), a carved 9x6x9 chamber, and `Colony`
  NBT written by hand. E: 5 to 8 days and ownership of a fork. It would still not seek out machines.

**ZBB 1.7.0** (LGPL-3.0, NeoForge 1.21.1). A sound digging engine for a few mobs (goals on every
`PathfinderMob`, filtered by `affectedEntityIdList`; health is `hardness^0.85 x 4`). **What the second pass
found:** it cannot be limited to one mob or to categories of blocks by config alone (whole-segment wildcards
only, exact ids only for blocks); it saves nothing per mob; its `LevelChunk.setBlockState` hook runs on every
block-type change on the server whether or not a digger exists, and its through-blocks ray has no length cap
and reads blocks synchronously. By default a mob 20 blocks down does **not** dig up (the notice limit is 3
blocks). To use it: config (`affectedEntityIdList`, notice limit 0), a head inject on
`matchesFullZbbMobFilter` for a `mic_digger` tag, a `@ModifyReturnValue` on `getBlockHealth` for the well
column, and plugin code. E: 3 to 5 days plus a profile of the hook. **That is about what the plugin boring
goal in 5.1 costs, with a global hook and a dependency added, so ZBB buys nothing.**

### 5.4 The blowout, a hazard with no creature (rank 3)

**What it is.** A gas pocket under each drilled field, owned by the sim: `Pocket{field, pressure, stage,
last_vent}`. Pressure rises as the field is pumped. When it crosses a stage, the field **blows out**: a bounded
H2S plume around the well head and the connected tunnels, with Destroy's own poisoning and the pack's gas
masks as the counter, and an explosion if anything in the plume is a flame (fire, lava, blaze burner, a lit
lighter, an overheated Power Grid device). After a vent the pressure resets and rebuilds. It is the nearest
thing the pack's pieces allow to a persistent environmental threat, and the only candidate in family 3 that
is tied to oil by construction.

**Buildable from seams already in the pack (V, section 3.4):** SMOG per chunk through
`PollutionHelper.changePollution`; poison through `ChemistryHazardHelper.damage` with a pure
`mic_petrochem:hydrogen_sulfide` mixture (runtime id U); the explosion through `Level.explode` or
`SmartExplosion`; the pipe filter through an `ExplosionEvent.Detonate` handler that removes the hole,
`below(1)` to `below(pipeLength - 1)` and the bedrock from the block list; the oil amount through
`OilChunksSavedData.getChunkOilAmount`.

**What is missing, and is the work (all V):**
1. No extraction event: a mixin at the return of `pumpjackRotation`, or poll the hole's `oilAmount`,
   `started` and `pipeLength`, or compare per-chunk amounts over time. Never call `getBaseOilAmount` in a
   scan (a fresh `PerlinNoise` per call).
2. No gas in the world: Destroy has none that spreads through blocks, so the plume is the sim's own bounded
   cell set (a BFS of connected air capped at N cells, cached per hole).
3. No ignition hook: the model must sample for flames itself.
4. No persistence: Collapsing Caves-style state is lost on restart, so the record lives in rtsim.
5. **Hurting machinery is the weak part.** Create blocks have resistance 6 (casings, andesite, gold and
   copper templates), a power-1 blast peaks at 1.3 and cannot break one, and breaking blocks 2 to 3 away takes
   power 3 to 4, which also craters stone and kills players. The clean way is a **direct damage pass**: the
   same surveyed, bounded `damage()` choice as 5.1, applied by the sim when the pocket ignites, with the
   explosion as the show. That is design work, not an existing seam.
6. The tell is weak: SMOG is chunk-wide and normal Destroy industry makes it too; H2S has no smell tag.

**Cost.** E: record and pressure rules 1 to 2, extraction poll 1, plume and poison and tell 1 to 2, ignition
sampling plus the direct damage pass plus the Detonate filter 2 to 3, GameTests 1 to 2: **6 to 10 days.**
Performance (U, reasoned): cheap when rtsim-owned, with polling at a few seconds per loaded hole, a capped
cell set, and the poison check only for entities in a plume. **Why rank 3:** it is an event, not a presence,
a player with a gas mask and no flame loses nothing, and machinery is hurt only with ignition.

**Collapsing Caves (CC0) as a part.** Not worth it: it must be installed on every client, cannot be fed a
cave-in from outside (the only creator is a player breaking a block; a plugin can build and tick its own
`CaveInEvent`, skipping its caps), forgets everything on restart, and never touches a Create block or pipe.
A plugin-owned falling-stone event using vanilla `FallingBlockEntity` gives the same "rocks hurt you" with no
mod and with persistence in the sim (E: 1 to 2 days; row 43).

### 5.5 A creep of vanilla blocks (rank 4)

**What it is.** A tar seep: the sim owns a bounded set of cells per field (`Seep{cells, stage}`), and a plugin
stamps them as ordinary vanilla blocks on chunk load, idempotently, from a per-tick budget. Three tags give it
rules: `mic:seep_replaceable` (what it may grow over: natural rock and soil), `mic:seep_eats` (surveyed
machine blocks it destroys with drops), `mic:seep_immune` (the well column and bedrock). A resource-pack
reskin gives it a tar look on a vanilla block, so D3 holds. No mod is added.

**Why it is here and low.** It solves persistence, oil tie and pipe sparing in the same way as the nest, and
it is potato-safe if the cell count has a hard cap and there is no chunk forcing (the common grief source in
the spreading mods of section 4.3). But a creep is slow, has no moments of danger, and the player's counter
is "dig it out", so it reads as a nuisance rather than a threat. It is best as the *terrain* of a nest
(row 45). E: 5 to 8 days (record and cells 1 to 2, stamping with chunk-load idempotence and a budget 2 to 3,
tags and survey exclusion 1, GameTests 1 to 2). Performance: medium, from the stamp budget (U).

### 5.6 Everything else, and why it is out

- **Sculk Horde, Spore, Nexus Infection, Crimson Curse** (rows 24 to 27): forced chunk loading, 100-block
  entity scans, 30 global `@e` selectors per tick or no block-entity guard; all weigh far more than the
  need (D1, D3).
- **Orpiment Tar Cube** (row 17): a good oil-themed body, but 1.20.1 Forge only, E 2 to 4 days to port, and
  the nest does not need one more body yet. First candidate if a tar look is wanted on the worm slot.
- **Silent Caves golem, Zombie Tactics, Apocalyptic Zombies** (rows 18 to 20): patterns for the boring
  goal, not parts; Silent Caves has a licence conflict between its jar and its store page.
- **Alex's Caves Corrodent** (row 21): excluded by Ben (P8).
- **EnviroMine Lite, Pollution of the Realms, GasCraft, Seismica, MineCollapse** (rows 32 to 36): licence
  conflicts, no release, forced chunk loading, or hooks on every block update; EnviroMine's burning coal seam
  is a good idea for a later "underground fire" hazard (row 33).

## 6. Verified and unverified, side by side

| Verified (V) in code or data | Not verified (U) |
|---|---|
| **Sim:** `damage()` picks only from the surveyed `exposed` set; Java breaks the chosen block with `destroyBlock(pos, true)` when its state still matches; arrays are sized by `ThreatClass::COUNT` on both sides | Which Create blocks the survey classes as `MACHINE` or `INVENTORY`; whether a district covers underground works (the nest design assumes yes) |
| **Well:** column test, protected set, no id-based sparing possible, cut pipe costs nothing lasting | Whether a digger blocked by a spared pipe stalls or routes round it (needs a GameTest) |
| **Explosions:** `Detonate` lists are live and final; falling blocks hurt living entities only; Create resistance 6 needs power 3 to 4 | The dose and balance of H2S in play; the runtime id `mic_petrochem:hydrogen_sulfide` |
| **Pack data:** pumpjack gives CDG crude; the override tag is in a plural folder 1.21 never reads; petrochem `fuel_type` JSON is in an unprefixed folder | Whether petrochem fuels really fail to load at runtime; whether vanilla fire ignites CDG crude; whether every Create loot table drops the machine |
| **Spawners:** no counter on monster spawners, 9x9x9 same-class cap, trial spawners need line of sight, hooks and NBT keys | How In Control or L2 Hostility treat a plugin's own spawns |
| **Arachnids:** ungated warrior path-breaking, colony bugs, A* cap, persistence model; **ZBB:** filter, health formula, no per-mob state, server-wide hook; **Collapsing Caves:** no public start, in-memory, client-required | **All performance.** No candidate was run or profiled; every cost is read from code, a config comment or an issue |
| **Licences and dates** as stated in the tables | IaF worm cost and the eight IaF mixins; Sable ships against any block-reading mob; CurseForge-only mods |

## 7. What the second pass changed

Three Opus agents re-read the load-bearing claims after the first sweep. The corrections, so a reader of the
tables can trust what is left:

- **Arachnids**: wave 1 said every removal is gated by `weak_blocks` and that pipes are spared by omission. **Wrong
  for warriors**, which break anything on their path. The colony layer was also found largely broken. It moved
  from "best existing digger" to "needs a fork".
- **ZBB**: wave 1 said well pipes could be spared by config alone and that a zombie 20 blocks down digs up to the
  player. **Both wrong** (exact ids only, and the default notice limit is 3 blocks). Its server-wide hook was added to
  the cost.
- **Vanilla spawners**: wave 1 said code can read a spawned count and that the trial spawner config is a registry
  entry. **Both wrong** (no counter on monster spawners; trial config is inline NBT). New: a trial spawner sealed in
  rock never starts; a plugin does not need a spawner at all.
- **Collapsing Caves**: wave 1 said it offered an API and was server-only. **Wrong**: no public way to start a
  cave-in, state is lost on restart, and it is required on clients.
- **CDG and the pack**: wave 1 said the pumpjack outputs Destroy's crude and that petrochem fuels burn as
  combustibles. **Wrong**: CDG crude is what comes out, the override is dead, and the petrochem fuel folder
  is probably not read. The well-pipe check is wider than thought (includes `ChorusPlantBlock`) and its extra
  tag is effectively empty.
- **Confirmed unchanged:** `Detonate` can filter blocks and entities, falling blocks hurt only living
  entities, Create resistance 6, the oil getters, the Destroy poison path.

## 8. For Ben

Nothing here was asked while you slept (D10). Each item has the recommended option first and says whether the
choice can be undone.

### 8.1 The recommendation

**Take the oil nest (5.1), starting with the first slice** (a loaded-only chamber, a husk, a boring goal and the
well-column exclusion, in a GameTest, no sim changes yet; 2 to 3 agent-days). It is the only option that
gives you a threat that persists, lives underground, gets worse where the oil is being pumped, hurts
underground Create works and spares well pipes, with no new jar, entity, block or item and a cost that is
zero for an unloaded field. If the slice feels right, the engine work (8 to 12 days in all) follows.

### 8.2 Decisions that are yours (architecture, public interface, data format)

Each is a choice I would make as written; none was acted on.

1. **A fifth threat class, `Burrowing`, and a `Nest` record in the engine.** Recommended: yes, with a new
   class (clean per-class cooldown, carry and blocks tunables; +1 day for the wire fixtures). Alternative:
   reuse `GroundMelee` and key the gate on a new `ThreatSource`, which avoids the array resize but shares
   the cooldown with bandits (I25). *Public interface and wire format, so yours.*
2. **First body: the vanilla husk, then the Death Worm.** Recommended: husk first (nothing to measure, in the
   pack, reskinnable), worm second (5.2, +2 to 4 days) once the nest works. The worm is the picture you
   described, so say if you want it first; the cost is a dependency on IaF internals and an unmeasured
   multipart entity. *Reversible: the body is behind the same threat.*
3. **What counts as "industry".** Today's survey lists machine and inventory block entities only, so casings,
   plain conveyors and shafts without block entities are not damaged. Recommended: leave it, because it keeps
   damage bounded and the survey cheap. Alternative: widen to every non-diggable block in the district at a
   cost in survey time and in how much a nest can break. *Reversible, but it changes the feel.*
4. **How a player defends against burrowers.** Today's measures (perimeter, roof cover, turrets, guards) do
   not apply underground. Recommended: a physical heart (a vanilla spawner in the chamber; breaking it clears
   the nest) plus an "armoured floor" tag (a layer of non-diggable blocks under works that the boring goal
   and the abstract path both respect). That is design, not an existing seam, and I did not choose the tag's
   contents. *Public rule, so yours.*
5. **Where nests exist.** Recommended: placed on first load of oil chunks, asleep until a pumpjack hole is
   drilled, escalating with drainage. Alternative: nests under every oil chunk whether or not drilled
   (undrilled fields are invisible to the engine today, OILGATE §3.1). *Reversible.*
6. **Do not buy Arachnids or ZBB.** Recommended: no (5.3). If you want a colony look later, the creep (5.5)
   or the nest's chamber carry it without a mod.
7. **The blowout (5.4) as a second threat.** Recommended: later, sharing the nest's field record. It is
   worth doing once the nest exists because it uses the pack's poison, gas masks and pollution tell.

### 8.3 Pack data bugs I found and did not fix (research only)

Small, reversible, and they bite any oil-tied design, so I recommend a short fix task whichever option you take.

1. **Dead tag overrides.** `pack/kubejs/data/createdieselgenerators/tags/fluids/pumpjack_output.json`,
   `pack/kubejs/data/forge/tags/fluids/*` and mic_petrochem's `data/c/tags/fluids/` are in plural `fluids`
   folders. 1.21 loads tags only from `tags/fluid`, so none of them takes effect. The visible result: the
   CDG pumpjack gives `createdieselgenerators:crude_oil`, not `destroy:crude_oil` as the files intend.
2. **Petrochem fuels probably not loaded.** `mods/mic-petrochem/src/main/resources/data/createdieselgenerators/fuel_type/`
   should be `.../createdieselgenerators/createdieselgenerators/fuel_type/`. The runtime check is U (nobody
   started the game), but the folder rule is V.
3. **OILGATE's boundary goods** (line 64) name crude oil, diesel and gasoline as the stolen goods; the crude
   id has to be `createdieselgenerators:crude_oil`, not Destroy's.

### 8.4 D3 gray areas

None in the recommended path. The nest stamps a vanilla spawner and vanilla blocks, reskins a vanilla mob by
resource pack (content, not a new entity), and uses IaF's worm (already in the pack) if you take 5.2. The
blowout uses `mic_petrochem`'s H2S molecule (in the pack). Collapsing Caves, Arachnids, Orpiment Tar Cube and
ZBB would each add a mod; I did not recommend any.

### 8.5 Choices I made

- Ranked on D1 (late-stage performance on a weak PC) first, then fit to the ask, then D3.
- Spent wave 1 on seven Sonnet reading agents and wave 2 on three Opus verifiers; read only, nothing run,
  built or added to the pack; downloads in `/tmp/underground-threats/` (not durable).
- Kept Terra Entity (OG-9c), Alex's Caves (P8) and licence-conflicted mods (Silent Caves, Nexus Infection,
  Pollution of the Realms, EnviroMine) out of the recommendation.
- Labelled all agent-day numbers E; they are mine, not measured.
- Wrote this document and landed it on `main` in `mods/mic-rtsim`; made no change to code, the pack or
  DECISIONS.md (OG-9d already records the widening; a recommended rank 1 is not a decision until you take it).
- Asked no questions; everything that needs your answer is in 8.2 and 8.3.
