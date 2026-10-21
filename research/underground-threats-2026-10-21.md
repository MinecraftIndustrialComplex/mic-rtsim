# Persistent underground threats near oil (research, 2026-10-21)

WIP draft: sections 2 to 4 only; the answer, ranking and For Ben follow.

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

- Verified in `PumpjackHoleBlockEntity.tick` (CDG 1.3.15; re-verified by section 7): every 20 ticks the
  hole checks a vertical column below itself. Each block must be a Create `PipeBlock`, `EncasedPipeBlock` or
  `ConcreteEncasedFluidPipeBlock` connected both up and down, a `GlassFluidPipeBlock` on the Y axis, or a
  block in `#createdieselgenerators:pumpjack_pipe`; the column must end at a block in
  `#createdieselgenerators:oil_deposit`, which is `minecraft:bedrock` only. A cut pipe makes the well
  invalid. Create fluid pipes are block entities (`FluidPipeBlock implements IBE<FluidPipeBlockEntity>`).
- So the **protected set is the column from each `pumpjack_hole` down to bedrock**, cheap to compute
  whenever a district is surveyed. Every well passes through the deepest layer (OILGATE §3.1), which is why
  the oil is reachable from below at all.

### 3.3 Oil and fire in this pack

- Oil is desert-only (`#c:is_desert`, kubejs tag replace) and found with the CDG pumpjack; Destroy's own
  pumpjack and seismometer are voided (`voided_content.js`), and Destroy crude is a virtual fluid with no
  world block. CDG crude is voided by the pack. **Crude cannot flood or burn as a block.**
- **Refined fuels can.** mic-petrochem registers placeable `LiquidBlock` fluids with CDG fuel types
  (kerosene, gasoline, premium, cheap, diesel). CDG's own `GameEvents.onExplosion` chains blasts through
  any of them (config "Combustibles blow up", default true). Create's open pipe ends place source blocks
  (`pipesPlaceFluidSourceBlocks`, default true), so a player's own fuel line can make a pool underground.
- **In Control spawns nothing below y 15** (OILGATE §3.2), and dragons do not fit in tunnels, so a player who
  drills from below meets none of the desert gate. Ice and Fire's griefing is off in the pack, so dragons
  break nothing.

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
| CDG oil chunks | per-chunk oil amount, depletion stored | `OilChunksSavedData.getChunkOilAmount / setChunkOilAmount / getBaseOilAmount` |
| CBC block damage | persistent crack overlays, destroyed at 10 scaled parts | `CreateBigCannons.BLOCK_DAMAGE.damageBlock(...)`, saved in `SavedData` |
| Power Grid overheat | power-1 explosion of an over-temperature electrical device | `ThermalBehaviour.setTemperature`, electrical machines only |
| Vanilla | `Level.explode` (fires `ExplosionEvent.Start` and `Detonate`), `FallingBlockEntity`, `minecraft:light` marker blocks, fluid ticking | NeoForge events |

## 4. Candidates, all families

Columns: **Fit** is my one-word read against the target (persistent, underground, oil-tied, hurts player
works, spares pipes, small, potato-safe). "Filter" says whether block damage can be bounded by tag or id.
Estimates are labelled E. Nothing here was run. Licences: code / assets where they differ.

### 4.1 Family 1: mobs that live underground or burrow

| # | Candidate | Platform, licence | Footprint | Persist, steer | Hurts machinery; filter | Performance | Fit |
|---|---|---|---|---|---|---|---|
| 1 | Vanilla spawner block (`BaseSpawner`) | vanilla | one block entity | NBT saved; place from code; `custom_spawn_rules`; NeoForge hooks `checkSpawnPositionSpawner`, `finalizeMobSpawnSpawner` | no; depends on the mob | one box count per spawn, player-range check per tick | **building block** |
| 2 | Vanilla trial spawner | vanilla | one block entity | state machine saved; re-arms; waves scale per player; ignores light; needs line of sight to the player | no | 20-tick player scan | **building block** |
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
| 14 | **Arachnids** (AzureDoom) | 1.21.1 NeoForge 0.3.0 (2026-05-08); MIT | 5 entities, 0 blocks, 1 mixin, AzureLib dep, 4 biome modifiers | colony in the `BrainBug` NBT; static map rebuilt on load; virtual population while unwatched; place a brain anywhere | workers break only blocks in `#arachnids:weak_blocks` (about 220 vanilla blocks); **pipes spared by omission**; Create blocks added to the tag are removed with no drops | A* up to 2,000 nodes per warrior repath; colony tick every 20 ticks; max 25 entities per colony | **best existing digger** (section 5.3) |
| 15 | Zombie Break & Build (ZBB) | 1.21.1 NeoForge 1.7.0; LGPL-3.0 | 0 entities, 0 blocks, 4 mixins | selects mobs by entity id only; no per-mob state saved | any block under a hardness cap; **no block-entity or tag filter** (one mixin point) | per-block-change hook, "may impact performance" config comment | behaviour layer |
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
| 30 | **Collapsing Caves 1.7** (CCDelic) | 1.21.1 NeoForge, Fabric, Forge; **CC0**, 205 KB | 3 server mixins, 0 blocks, 0 entities, 1 enchantment, 4 commands | in-flight events held in memory, U across restart; trigger is a player mining natural stone | falling blocks hurt entities; **vanilla landing never damages a machine or pipe** (cites vanilla `FallingBlockEntity`) | BFS budgeted per tick; caps 3 events and 600 falling blocks; 9 closed issues, none about lag | **hazard part** (section 5.4) |
| 31 | Sable: Collapsing Caves | NeoForge 1.21.1, requires Sable (in pack); CC0 | 2 mixins | debris becomes Sable sub-levels | as above | sub-level physics cost U; the pack already pays for Sable | variant |
| 32 | MineCollapse 2.0.0 | NeoForge; EUPL-1.2 | own blocks, items, recipe types, 3 mixins | per-level attachment | no | `NeighborNotifyEvent` hook on every block update | rejected (D3, cost) |
| 33 | EnviroMine Lite 1.1.3.1 | NeoForge; store CC-BY-NC-SA vs jar ARR | 107 classes | burning coal ore seam fire, depth toxicity | entities only | U | rejected (licence); idea reusable |
| 34 | Pollution of the Realms 9.1.12.0 | NeoForge; store CC-BY-ND vs jar ARR | 873 KB, own pollution system | persistent gas blocks, explode on ignition (`GasExplosion.in(level)...tryExplode()`) | explosions break blocks; `maxChainLength` bounds | every gas block random-ticks, ignition check scans 26 neighbours, U | rejected (licence, scope) |
| 35 | GasCraft 1.0.4 | NeoForge source only (repo created 2026-10-15); LGPL-3.0 | 80 Java files, weapon gases | cloud entity, data-driven gases | entities only | BFS per cloud | rejected (unreleased) |
| 36 | Seismica 1.7.0 | NeoForge; store MIT vs jar ARR | 1.2 MB | `SeismicaAPI.startEarthquake`, saved quakes | surface blocks, non-container machines near surface | **force-loads chunks on the server thread** (`forceLoadDamage`, default true) | rejected |
| 37 | mge, Plenum, Hazardous, Simple Block Physics, Cave-Ins datapack, Thick Air family, Create: Smog and others | various | | | | | patterns or rejected |
| 38 | **Pack parts: Destroy pollution, chemical hazard + mic-petrochem H2S, CDG chain explosion and oil chunks, CBC cracks, Power Grid overheat** | in pack | 0 added | plugin-owned record | explosions break blocks; poison hurts players | cheap, U measured | **hazard part** (section 5.4) |
