# Worm mods, round 2: the smallest footprint (research, 2026-10-21)

Context: OG-9c in DECISIONS.md. Ben rejected Terra Entity ("a mod that adds so much when we need so
little") and asked for more investigation, including mods for other Minecraft versions and loaders,
because "getting an agent to translate a mod between versions is basically free". Round 1 is
`research/worm-mods-2026-10-20.md`; this document builds on it and does not repeat it.

Target: Minecraft 1.21.1, NeoForge 21.1.249, Create 6. The ideal is one worm entity (or a small family)
and nothing else: no items, enchantments, loot changes, curios, recipes, worldgen, biomes or NPCs.
Rules in force: D1 (judge on late-stage runtime performance on a weak PC), D3 (no bespoke entities,
blocks or items unless absolutely necessary; a port or slim fork of another mod's entity is a gray
area, flagged below), D4 (plugins per mod, mixins allowed).

"V" below means verified from a jar, source file, licence file or API response. "U" means not verified.
Nothing was run in game and no spike was built; this is research only. The pack and its jars are unchanged.

## 1. Short answer

1. **The best candidate is already in the pack: the Ice and Fire CE Death Worm, used as a body and
   driven by an rtsim plugin.** Footprint added: nothing (no jar, entity type, item or block; zero
   mixins in the recommended variant). **But round 1's tag-swap idea does not work as stated.** Round 1
   (section 3 row 4 and section 4.4) took the worm to swim through sand. In the pack's build it cannot
   pass through any block, sand included: Ice and Fire CE dropped the collision hook that let the
   original worm sink into sand (section 4.1). The working route is a plugin goal that sets `noPhysics`
   on the puppet while its head is in rock, steers it through its move control, removes IaF's own goals
   and cancels IaF's block-grief event so rtsim owns all damage. Cost: about 400 to 700 lines of plugin
   Java, 2 to 4 days with GameTests (estimate). Open risk: how it looks (the body is a rigid rod behind
   the head that does not pitch, section 4.1). Nobody has run it.
2. **Fallback: a worms-only slim fork of Terra Entity.** It is a trim, not a port: 23 source files,
   about 2,240 lines, **0 of the 36 mixins**, GeckoLib as the only dependency, 2 to 3 entity types, 0
   items. It is the best-proven worm (round 1 GameTest: 30 stone blocks in 80 ticks, 0.03 to 0.05 ms per
   worm, broke 0 blocks). The catch is the art licence: the code is LGPL-3.0 but the jar ships a CC
   BY-NC-SA 4.0 licence file for the assets (9 models, 9 textures, 5 sounds), so a fork needs new art to
   shed the NonCommercial and ShareAlike terms. D3 gray area. About 6 to 12 agent-hours (estimate) plus
   the art.
3. **Third: a plugin-owned worm made of vanilla entities** (display entities for the body, hidden hitbox
   mobs), copying the design of the MC Calamity datapack (CC-BY-4.0). Registers nothing. 400 to 800
   lines, 3 to 5 days (estimate), a D3 gray area, and a real packet cost.
4. **Nothing else found is small, openly licensed, on the right platform and tunnelling.** Besides IaF
   and Terra Entity, the things that really move through rock are Aberrant Mobs, Sculk Infection,
   TerraMine's Devourer and the MC Calamity datapack. Each fails on shape, size, licence, platform or
   cost (sections 3 and 4). The four mods round 1 dropped for version or loader (Tremors: Graboids and
   More, Burrowers, Earthworms!, Ore Lizards) are now checked: none tunnels and none is a worm worth
   porting.

Recommended path: spike option 1 for one day with a GameTest (cheap, undoable, adds nothing to the
pack); if the look or the behaviour is unacceptable, take option 2 with new art.

## 2. How this was done, and the gaps

- **Modrinth API, no version or loader filter:** about 164 distinct queries (worm, sandworm, graboid,
  burrow, tunnel, serpent, wyrm, centipede, devourer, eater of worlds, terraria, calamity, multipart,
  segmented and more), plus a keyword crawl of the whole catalog (77,405 of 77,496 mods and 15,245
  datapacks, by the sweep's own count), hand triage, 46 projects fetched in full and 15 jars downloaded
  and unzipped.
- **GitHub:** repository and code searches for worm, segment, part-entity and burrow classes; build files
  read for version and loader; a number of repositories cloned and read.
- **Jars and sources** read with `javap -p -c`, `cfr` and `jadx`. The pack's Ice and Fire CE jar was
  checked against the sha512 in `pack/mods/iceandfire-ce.pw.toml` (it matches). Downloads are in
  `/tmp/worm-round2/` (untrusted, read only, nothing was run; `/tmp` is not durable).
- **Gaps:** about 90 mods were lost to rank shift while paging; a worm whose description never says worm,
  burrow or segment is invisible to the keyword filter; PlanetMinecraft, the MCreator gallery and the FTB
  wiki were not searched; some GitHub code searches hit the rate limit and were not rerun. Not inspected:
  Divine RPG's `EntitySpinebackWormBody`, Embers Rekindled's `EntityMagmaWorm`, `the-mole-reimagined`,
  `in-the-depths`, `weirdo-mod` (needs 1.21.8 or later), and MC Calamity beyond its `wormai` engine.
  Hearsay of worms in Grimoire of Gaia, DivineRPG, Primitive Mobs and Mo' Creatures was not confirmed (U).

## 3. Candidates

### 3.1 What each one is

| # | Candidate | Code licence / art licence | MC, loader | Shape | Stone and deepslate | Breaks blocks itself | Steering |
|---|---|---|---|---|---|---|---|
| 1 | **Ice and Fire CE Death Worm** (in pack, 2.1-beta.1) | LGPL-3.0 (V, jar mods.toml); nothing new is shipped | 1.21.1 NeoForge, in pack | Head entity drawn with a rigid rod body; 7 invisible `SlowPartEntity` hitbox segments (V) | **Not as shipped** (cannot enter any block, V). With a plugin setting `noPhysics` it passes through anything | Yes, crudely: 1 in 3 when ageScale is above 1 and mobGriefing is on, a `BlockLaunchExplosion` that sets blocks to air and spares nothing; cancellable via `GriefBreakBlockEvent` (V) | `setTarget`; `MoveControl.setWantedPosition`; two of its goals are non-interruptable and must be removed (V) |
| 2 | **Terra Entity slim fork**, worms only | LGPL-3.0 (V) / a CC BY-NC-SA 4.0 licence file ships in the jar's assets (V) | 1.21.1 NeoForge already | `PartEntity` segments, 13 to 19 small models per worm, GeckoLib (V) | Yes: `noPhysics` set in the constructor for life (V); round 1 GameTest: 30 stone blocks in 80 ticks | No (V; round 1: 0 blocks broken) | `Mob.setTarget`, GameTest-verified; must be set every tick (round 1) |
| 3 | **Vanilla-entity worm in a plugin**, design from MC Calamity | own code; MC Calamity is CC-BY-4.0 (V) | 1.21.1 NeoForge | `item_display` or `block_display` segments plus hidden NoAI hitbox mobs | Yes (the plugin moves it; segments invulnerable while buried) | No (the plugin owns damage) | Fully the plugin's |
| 4 | **Aberrant Mobs 1.4.0** | AGPL-3.0-or-later (V) | 1.21.1 NeoForge | One head plus up to 16 parts; the shipped creature is a legged centipede (V) | Yes: A* through rock, digs and leaves permanent tunnels (V) | Yes, through one choke point, `DigWorld.dig` (air, no drops); never digs any block-entity block and plans 2 cells clear of them (V) | `setCrawlTarget`, NoAI, custom verbs (V) |
| 5 | **Sculk Infection / Sculk Worm** | jar says CC0-1.0, both store pages say All Rights Reserved; treat as ARR (V) | Fabric; reportedly runs via Sinytra Connector (not run) | Head plus 10 separate `PathfinderMob` segments (V) | Yes: tunnel mode with `noPhysics` (V) | Only decoration, glass, rails and wiring (V) | public `setCurrentTarget(BlockPos)` and `alertToLocation` (V) |
| 6 | **TerraMine Devourer** | LGPL-3.0-or-later / Terraria likeness (V) | Fabric 1.21.4 | Head plus separate Mobs (11 to 15 tracked entities, V) | Yes: `noPhysics` in the constructor, speed 0.75 blocks per tick (V) | No (V) | **`setTarget` does nothing**: a private `target` field is overwritten every tick (V) |
| 7 | **Apophis** (what-the-ashes-remember) | GPL-3.0 (V) | NeoForge **26.1.2** | 83 `PartEntity` parts, 1 tracked entity, body derived on the client from head history (V) | It breaches and dives rather than hunting caves (V) | No (V) | goals would need replacing |
| 8 | **Calamos Worm** | MIT / placeholder art (V) | 1.21.1 NeoForge | Head plus `LivingEntity` parts that duplicate on reload and die on chunk unload (V) | **No AI at all** (velocity zeroed each tick) (V) | No | Teleport only |
| 9 | **Desert Scourge datapack** (UkrainenBimba) | MIT / Calamity fan art, likely Calamity's music (V) | 1.20.6 to 1.21.1 | Head `item_display`, 16 bodies of `item_display` plus a NoAI vex (scale 8), tail pig, sand tornado; about 42 entities per worm (V) | **No as shipped**: head snaps to the world surface (V) | not seen | Scoreboard values |
| 10 | **MC Calamity datapack** (`wormai` engine) | CC-BY-4.0 (V) | 1.20.2 to 1.21.4 | Armor stand, cloud and ravager per segment; tomb crawler 7 vindicators, Desert Scourge about 80 entities (V) | Yes: 3D underground chase, segments invulnerable inside stone (V) | No (V) | Head steers through an aim-point `area_effect_cloud`; the logic is copyable |
| 11 | **Twilight Forest Naga** | LGPL-2.1 code / CC BY-NC-SA assets, ARR sounds (V) | NeoForge (version not checked) | 12 `PartEntity` | No: surface slither | Yes: smashes blocks via `destroyBlock` (V) | `setTarget` |
| 12 | **Tremors: Graboids and More** | All Rights Reserved (V) | Forge 1.20.1 | Single body (MCreator) | **No**; `GraboidAvoidRocksProcedure` calls `level.explode` (MOB interaction, 0.5) on the stone below it (V) | by explosion | vanilla goals |
| 13 | **Burrowers** | LGPL-2.1-or-later (V) | Forge 1.20 to 1.20.4 | **Not worms**: 4 entities, 21 items, 33 blocks (V) | No | No | n/a |
| 14 | **Earthworms!** | MIT (V) | Fabric 1.21.1 | Harmless 40 HP critter; "burrowing" is an invisibility flag (V) | No | No | n/a |
| 15 | **Ore Lizards** | All Rights Reserved (V) | Fabric | One passive mob with a `FleeAndBurrowGoal` (V) | No | No | n/a |
| 16 | **Lycanites Mobs** (Crusk, worms) | custom "Lycanites Mob Public License" (allows partial borrowing), Modrinth says LGPL-3.0: conflicting (V) | the only build is a 120 MB 1.20.1 alpha (V) | Single bodies; Concapede is a surface multipart insect | Crusk and worm creatures hide only in `#lycanitesmobs:worm_burrowable` soft blocks (V) | No | n/a |
| 17 | **Magworm datapack** | CC-BY-4.0 (V) | 1.21.6 and later | Magma-cube chain, not a tunneller (V) | No | No | n/a |

Also seen and set aside, one line each:
- **mcterra** (MIT): no worm (verified).
- **TerrariaMod** Devourer (CC0, Fabric 1.20.1): a broken flying stub.
- **SandStorm** (CC0, MC 26.3 Fabric): a 734-line `SandwormEntity` in a roughly 25k-line mod.
- **Modular Bosses** (MIT, 1.8.9): a `noClip` SandWorm with 10 segments, but only a wandering swallower.
- **StoneWorm** (ARR, 1.12.2); **Better Beach Biomes** sand worm (ARR, surface); **Bridge Worm: Reborn**
  (ARR, a leaper); **Antarchy** (ARR); **hole-diggers** (ARR).
- **Digger Mobs** (ARR datapack that makes vanilla mobs dig): the pattern only.
- **OreSpawn** Large Worm (1.7.10): no source. **Kelvin285/trewrite** (1.13 era): licence unclear.
  **Tyopo131/minecraft-worms**: a stub.
- No worm: **minecraft-calamity**, **terramity**, **terrariacraft**, **TerraVermis**, **the-mole-horror**.
- **Calamity mod** (CC-BY-NC, 256 MB, 1.20.1): not a worm source for us.
- **CustomHitboxLib** (MIT, v2.0.9): a library, not a worm; rigid offset hitboxes.

Already covered in round 1 and not repeated: Terra Entity full mod (and its silenced cost, section 7),
Aftershock (MIT in the jar vs CC-BY-NC-4.0 on Modrinth, single body, soil only), Desert Behemoths
Sandworms port, Enter Sandworm, Devourer of Gods (ARR, a boss), Alex's Mobs and Bosses'Rise.

### 3.2 What using each one costs

| # | Candidate | Footprint added to the pack | Port or patch effort to 1.21.1 NeoForge | Client render cost risk |
|---|---|---|---|---|
| 1 | IaF Death Worm + plugin | **0** jars, entities, items, blocks. 0 mixins (variant B) or 2 mixin classes of about 60 lines (variant A) | Plugin only: 400 to 700 lines, 2 to 4 days with GameTests (est.) | Existing model and invisible parts. `getWormBrightness` scans up to y 256 twice per frame (V); each of 7 segments marks itself hurt every tick, so 7 motion packets per tick per worm (V) |
| 2 | Terra Entity slim fork | 2 to 3 entity types, 0 items, 0 blocks, 0 mixins; one jar of about 150 to 200 KB (est.); about 108 KB of raw assets (V) | 23 files and 2,240 lines kept, glue 150 to 250 lines; 6 to 12 h (est.) plus art | 9 small geo files (19 KB) parsed per reload; 13 to 19 tiny models per worm; no per-frame hooks (counts only, U on cost) |
| 3 | Vanilla-entity worm | 0 registered content; about 4 custom item models as `custom_model_data` overrides in the plugin's assets, or none with plain block displays | 400 to 800 lines, 3 to 5 days (est.) | about 400 move packets per second for a 20-segment worm per watching player, thinned with `teleport_duration` (est.) |
| 4 | Aberrant Mobs | stock: 1 entity, 8 items, 4 armour recipes, 11 sounds, a datapack registry, a biome modifier, **17 mixins** (16 for chitin-armour wall-walk on `Entity.move`, `handleMovePlayer` and `Camera.setup` among others, 1 for a hint), a synced byte on every player and one payload | stock jar plus 1 or 2 mixins plus a datapack; or a slim fork, 2.5 to 4 agent-days (est.) plus a Blockbench worm | about 6.9k vertices per frame; server A* replans every 20 ticks (its own note says 0.2 to 1.5 ms per path, not measured); a spawn check loops all entities; an Ears query over a 1024-wide box |
| 5 | Sculk Infection | 12 entity types, 28 mixins, runs under Connector | native fork 30 to 50 h (est.) | 11 entities per worm |
| 6 | TerraMine Devourer | the whole Terraria mod: 27 entities, 218 items, 113 blocks, 71 mixins (452 classes) | 6 to 16 h (est.) | 11 to 15 separate entities per worm |
| 7 | Apophis | a boss mod | 16 to 24 h from 26.x (est.) | 83 parts, 1024 px textures, post-processing |
| 8 | Calamos | a whole boss mod: 11 entity types, hard dependencies on fusion, terrablender and curios, 12 mixins | not worth it: contributes about 4 lines of hurt forwarding | trivial |
| 9, 10 | Datapack worms | forceloads, about 45 objectives (Scourge), a 3.5 MB resource pack with core shaders (Calamity) | fork 2 to 3 days; fails D1 as shipped | custom item models; Calamity needs replacement core shaders |

## 4. Verified facts and how they were checked

### 4.1 Ice and Fire CE Death Worm (zero footprint), the question the brief asked

Sources: the pack jar `iceandfire-2.1-beta.1.jar` (sha512 prefix 8e196439f3a709d65648989a matches the
pack), decompiled with cfr, plus Uranus 2.4.1, Jupiter and the CE source on GitHub.

1. **The worm cannot burrow in this build, so a tag swap alone does nothing.** The original IaF worm
   overrode `collide(Vec3)` to call `ICustomCollisions.getAllowedMovementForEntity` from Uranus, which let
   it sink into sand. The CE port dropped that override. I checked: `DeathWormEntity.move` is a plain
   `super.move`; a binary-safe grep finds `getAllowedMovementForEntity` only in its declaring class, with
   no caller in Uranus 2.4.1, the IaF jar or Jupiter; Uranus's `EntityMixin` only hooks `spawnAtLocation`.
   CE issue #185 reports this, and the 2.1.4 branch still lacks the override.
2. **17 reads of `BlockTags.SAND` in 10 classes.** The key ones are `isInSandStrict`, `canPassThrough`,
   `DeathWormNodeMaker`, `DeathWormSandNavigation`, `DeathWormAIGetInSandGoal`,
   `DeathWormAIAttackGoal.start` and `DeathWormAIJumpGoal`.
3. **Variant A, two mixin classes, about 60 lines:** `Entity.collide(Vec3)` as an `@Inject` at HEAD,
   cancellable, gated to puppet worms and returning `ICustomCollisions.getAllowedMovementForEntity`; plus
   `@ModifyExpressionValue` (MixinExtras 0.5.3 is bundled with NeoForge 21.1.249) on the `BlockTags.SAND`
   reads in `isInSandStrict` and `canPassThrough`, returning a `mic_rtsim:worm_burrowable` tag. Do not
   hook the `move` to `collide` call site: that conflicts with Sable 2.0.5. Lithium 0.15.4 only redirects
   inside `collide`, so it coexists. Caveat: with the tag swapped, `getSurface()` scans through the whole
   rock column and gets unbounded. Whether this could also stop the worm at pipes is U.
4. **Variant B, no mixins (recommended):** a plugin goal sets `noPhysics` and `setNoGravity` while the
   head is in a burrowable block; steers through `MoveControl.setWantedPosition`; clamps velocity; bites
   through `doHurtTarget`; strips IaF's goals with `removeAllGoals` (the attack and jump goals are
   non-interruptable, so a higher-priority goal cannot displace them); cancels `GriefBreakBlockEvent`;
   and does its own bounded breaking against `#mic_rtsim:worm_immune` (round 1 section 4.2). The cost:
   pipes are not physically stopped, so the goal must steer around them.
5. **Targeting and lifetime (V):** IaF's own `DeathWormAITargetGoal` reaches only 6 blocks vertically, so
   the plugin must set targets. A worm needs persistence and never despawns, so the plugin must discard
   its puppets. Segments are real level entities with empty save data, rebuilt on the next tick.
6. **Damage, scale, look (V):** health is `max(6, 10*ageScale)` and bite damage `max(1, 3*ageScale)` with
   `ageScale = min(scale*wormAge/5, 7)`; bite reach is `min(4, 4*ageScale)`. The body is a rigid rod
   behind the head along `yBodyRot` and pitches only while `wormJumping`, so a worm climbing or diving
   through rock would render level. The 7 segments render with `NothingEntityRenderer` (invisible).
7. **Block breaking:** inside `doHurtTarget`, 1 in 3 times when `ageScale` is above 1 and mobGriefing is
   on, a `BlockLaunchExplosion` sets blocks to air with no drops and spares nothing, pipes included. A
   TNT `willExplode` bug exists. IaF has no selective wrecking.
8. **Spawning:** only through a worldgen feature in `#iceandfire:entity_gen/deathworm` (desert only per
   OILGATE, chance 0.0333, pack health 10 and attack 3). Puppets can follow the DragonBridge pattern.
9. **Licence:** IaF CE is LGPL v3 or later (jar mods.toml says LGPL-3.0); Uranus is LGPL-3.0. A plugin
   that only compiles against the jar and mixes in at runtime copies no code. Do not copy IaF source.
10. **Datapack alternative (extend `#minecraft:sand`): rejected.** It gives no tunnelling (item 1) and has
    a large blast radius: cactus and sugar cane survival, turtle eggs, the bamboo and dead bush tags, LSO
    plants, Mowzie's, the IaF ice-dragon, Cobblemon spawn pools and others.

### 4.2 Terra Entity slim fork

Source: commit `4b5522e3` (the 1.2.3 tag). Upstream HEAD `dacb8cac` moved attributes into a library
(magic lib 0.0.9) that is not in the repo, so a fork starts from the tag. The correct source repo per
the sweep is `Magic-team-jvav/TerraEntity`, branch `neoforge-dev/1.21.1`; the URL round 1 used
(`EDGtheXu/...`) is stale or a fork.

- **Class closure (V):** 23 files, 2,240 lines (1,442 of code), 30 class files, 131 KB; 1,880 lines
  without bone_serpent. Entities: `BaseWorm` 226, `BaseWormPart` 207, `SurefaceWorm` (the devourer) 36,
  `BoneSerpent` 72. Base and attribute helpers: `AbstractMonster` 245, `AttributeBuilder` 127,
  `AttBuilder` 107. Goals: `ComeAndBackDashAttackGoal` 90, `WormRandomWanderGoal` 127,
  `AccelerateOnSeeingGoal` 34, `MutableRangeNearestAttackableTargetGoal` 37; Wyvern's nested goals; the
  curve classes; `ICollisionAttackEntity` 113; `IHeightControlMob` 28; client `GeoNormalRenderer` 153,
  `GeoWormRenderer` 96, `GeoWormSegmentRenderer` 76, `GeoNormalModel` 61. Cut and replaced by new glue
  (150 to 250 lines, est.): `TEMonsterEntities`, `TESounds`, `TEUtils` (1,097 lines; only `angleBetween`
  and `circle` are needed), `AbstractTerraBossBase`, `IDiscardWhenRespawnEntity` and others.
- **Libraries:** all 4 jar-in-jar libraries can go. GeckoLib becomes a declared dependency. The `javap`
  signatures of the 17 GeckoLib classes used are identical in the bundled 4.8.4 and the pack's 4.9.3
  (not run in game).
- **Mixins: 0 of 36 needed (V).** One fork fix: add a `setId` override as `EnderDragon` does; whether the
  `recreateFromPacket` id mismatch matters in play is U.
- **Assets (V):** 9 geo files (19 KB, plain JSON, none encrypted), 9 PNGs (14 KB), 5 sounds (75 KB), no
  animation files; 3 entity names and 5 subtitles in lang. No particles, packets or config. The 35
  encrypted models are not worm models.
- **Behaviour traps (V):** `giant_worm` has hard-coded Y limits: it wanders at Y 20 or below and attacks
  only while its own Y is below 50. The devourer flies (`canFly`, wanders at `max(y,63)+2`). Body parts'
  attack filter only checks `parent.canAttack`.
- **Correction to round 1:** `noPhysics` is set in the `BaseWorm` constructor (I confirmed it from the
  class), not from `isInWall`.
- **Licence, code (V):** `LICENSE.txt` is the LGPL-3.0 text only (no GPL text). Modrinth says "or later",
  the repo does not. Treat as LGPL-3.0-only.
- **Licence, assets (V):** the jar ships `assets/terra_entity/LICENSE-CC-BY-NC-SA-4.0.txt` (I opened it
  from the jar). Git history credits the worm models and sounds to MagicHarp. NonCommercial and ShareAlike
  therefore apply to the forked art: if MIC is ever monetized the art cannot go with it, and adapted art
  must be released under CC BY-NC-SA. New art removes both; the code stays LGPL.
- **What LGPL-3.0 needs for a modified fork in a distributed pack (my reading of the text, not legal
  advice):** keep the fork under LGPL-3.0; publish its full source (a public repository at the release
  tag is the practical offer); ship the LGPL-3.0 and GPL-3.0 texts (upstream omits the GPL text); mark
  the files as modified; keep the credits; rename the mod id so it is not mistaken for upstream. AGPL-3.0
  (rtsim) is compatible with LGPL-3.0. Terraria is Re-Logic's IP: the names Devourer, Giant Worm and Bone
  Serpent and their designs come from it, so rename and redraw. Whether Re-Logic tolerates fan mods is U.
- **Clean-room variant:** 450 to 600 lines plus new art (est.); no LGPL, no NC art, no Terraria names.
  Equally a bespoke entity under D3.
- **Build:** upstream's Gradle cannot build standalone, so the fork needs its own (ModDevGradle 2.0.x,
  NeoForge 21.1.249, GeckoLib compileOnly). Maintenance would be ours permanently.
- **Server cost:** round 1 measured 0.03 to 0.05 ms per worm and, for the full mod silenced, about +3.6 s
  boot and +20 MB heap. The fork drops the global listeners. Client cost is U.

### 4.3 Aberrant Mobs 1.4.0 (the only stone-tunnelling mod found that is already on NeoForge 1.21.1)

`github.com/the-rusty-shackleford/minecraft-aberrant-mobs`, AGPL-3.0-or-later (compatible with rtsim,
which is AGPL-3.0). The repository was created 2026-09-15, has 7 releases up to 1.4.0 (2026-09-27), one
contributor, 0 stars and 37 downloads, and is not on Modrinth or CurseForge. Its `PROJECT.md` says it was
written by Claude Code with a human reviewer.

- **Size (V):** jar 842 KB, 170 classes, about 13.7k lines (domain 7.1k, main 6.6k) plus 7.8k lines of
  tests.
- **Footprint (V):** 1 entity type (`aberrantmobs:aberrant`), 8 items, 4 armour recipes, 1 loot table, 11
  sounds, 2 damage types, a datapack registry `aberrantmobs:creature`, 1 biome modifier (`#is_overworld`,
  weight 2), 17 mixins (16 for chitin-armour wall-walk, 1 for a hint), a synced player data byte and one
  payload. Their cost and their conflicts with Sable and Create are untested (U).
- **Movement (V):** one head plus up to 16 parts. `D/Crawl.java` clings to block faces, turns at most 25
  degrees per tick, and digs a section 2.0 blocks ahead (up to 82 cells per strike); permanent tunnels
  remain. `D/Burrow.java` is a 6-connected A* with a 4,000-expansion budget.
- **What it spares (V):** every block-entity block is undiggable, with a 2-cell keep-out for paths, so
  Create shafts, pipes, belts and the pumpjack are spared as shipped (U: which Create blocks have block
  entities is from the agent's reading, not rechecked). `#aberrantmobs:undiggable` ships empty and makes
  blocks undiggable for digging and pathing. So it cannot "wreck industry" by itself; that would have to
  come from rtsim.
- **Steering (V):** `Aberrant.create`, `setCrawlTarget(Vec, dig, speed)` (line 981) and `Verbs.register`;
  NoAI plus `crawlTick` works. All digging goes through the one static `DigWorld.dig` (sets air with
  flags 3, no drops, no BreakEvent, no mobGriefing check), so one mixin could hand it to rtsim. Damage is
  players only: pinch 2.0 and a `devoured` type of 1e6.
- **Shape (V, with one U):** the profile schema allows a legless undulating chain in JSON, but animation
  clips are hard-coded (`FaceStealerClips`). The shipped Face-Stealer is a legged centipede with a stolen
  player face (288 cubes). `RigStore` may not see third-party models (U), so a worm shape practically
  means a fork.
- **Slim fork effort (est.):** cut about 3.0k lines of wall-walk: 2.5 to 4 agent-days plus a Blockbench
  worm, leaving a fork of roughly 10.7k lines of a one-month-old, single-maintainer, AI-written mod.
- **Verdict:** second tier: young single-maintainer mod, wrong shape, hot-path mixins.

### 4.4 Worms built from vanilla entities (datapacks, and a plugin that copies the design)

Both datapacks were read, and the vanilla 1.21.1 sources were consulted for what their commands cost.

- **Desert Scourge (MIT), reject:** the head snaps to `world_surface`, so it does not tunnel as shipped.
  It runs 6 global `@e` scans per tick even with no worm present, about 45 objectives, leaves three
  forceloads at 0,0 in the world and a leaked tail pig. The art is Calamity fan art and the music is
  likely Calamity's soundtrack (an IP risk). `@e` scans cost in proportion to every entity in the level,
  not just matching ones.
- **MC Calamity (CC-BY-4.0, NotGleam/TheScourgeNerds, project y7VxdN7W, 49k downloads), reject as
  shipped, keep the design:** the `wormai` engine has tiny files, steers through an aim-point
  `area_effect_cloud`, links segments through a passenger-cloud chain and makes segments invulnerable
  inside stone. It does 3D underground chase and breaks no blocks. It needs an always-on manager doing
  NBT scans, a 3.5 MB resource pack (core shaders, objmc), and has no licence file in the resource pack.
- **Java plugin using the same design (option 3):** segments as `item_display` or `block_display`
  entities, hitboxes as `LivingEntity` NoAI invisible mobs. An `Interaction` entity cannot be the hitbox
  because projectiles and explosions cannot hit it, so the hitbox must be a living entity. The plugin owns
  damage and the `#mic_rtsim:worm_immune` exclusion. About 4 custom item models as `custom_model_data`
  overrides in a jar's assets; no new item or entity is registered. A `block_display` look is "a string
  of tumbling blocks". Network: about one move packet per segment per tick per viewer (20 segments is
  about 400 packets per second), thinned with `teleport_duration`. Server cost 0.02 to 0.06 ms per worm
  (U, estimate). Unchecked risks: whether TaCZ bullets hit invisible mobs, whether iron golems and In
  Control react to hidden hitbox mobs, and how buried hitboxes avoid in-wall damage. D3 gray area:
  bespoke worm code and hidden vanilla mobs that other mods may see.
- **Magworm (CC-BY-4.0):** 1.21.6 or later, a magma-cube chain and not a tunneller; not useful.

### 4.5 The other skeleton codebases, in short

- **Calamos (MIT, `LopyLuna/Calamos`):** worm of 487 lines with placeholder art and no AI; parts are
  separate `LivingEntity` objects. Reject.
- **TerraMine Devourer (LGPL-3.0-or-later, `jakeb072001/TerraMine`, Fabric 1.21.4):** about 820 lines;
  the follow code is adapted from the Naga; it overwrites a private `target` field every tick, so
  `setTarget` does nothing; bodies churn on reload; the art is a Terraria likeness. Port 6 to 16 h. Skip.
- **Apophis (GPL-3.0, `frozenhamburgers/what-the-ashes-remember`, NeoForge 26.1.2, package `net.jelly`):**
  83 `PartEntity` parts, 1 tracked entity, about 1,600 lines stripped (the package is 3,078, plus
  marionette 606 and physics 195). It breaches and dives; textures are 1024 px and it uses
  post-processing; port from 26.x 16 to 24 h. Optional borrow: `ApophisChainAnimator` and the
  `WormControlPoint` embedded rule (about 250 lines), which would make the worm mod GPL-3 (compatible
  with AGPL).
- **Sculk Infection (Fabric; authors dinohunter0727 and makeitcasual):** head plus 10 `PathfinderMob`
  segments, tunnel mode with `noPhysics`, about 6,500 lines, 12 entities, 28 mixins, intermediary names,
  runs through Sinytra Connector, breaks only decoration, glass, rails and wiring. The jar says CC0-1.0
  with a `LICENSE_sculk_worm`, but Modrinth and CurseForge say ARR: treat as ARR and ask the authors.
  Port 30 to 50 h. Not advised; a behaviour reference only.
- **Twilight Forest Naga (LGPL-2.1 code, CC BY-NC-SA assets, ARR sounds):** 12 `PartEntity`, surface
  slither, smashes blocks via `destroyBlock`; `moveSegments` uses straightenForce `0.05 + 1/(i+1)*0.5`.
  Reference only.

### 4.6 The four round 1 drops, now read

- **Tremors: Graboids and More** (ARR, Forge 1.20.1 only, 292 downloads, MCreator): single body, no
  tunnelling, and `GraboidAvoidRocksProcedure` calls `level.explode` on the stone below it. Needs
  permission and is the wrong shape and behaviour.
- **Burrowers** (LGPL-2.1-or-later, Forge 1.20 to 1.20.4): not worms (4 entities, 21 items, 33 blocks).
- **Earthworms!** (`orca-gamedev/earthworm-mod`, MIT, Fabric 1.21.1): harmless 40 HP; "burrowing" is an
  invisibility flag. It has no Modrinth project.
- **Ore Lizards** (ARR, Fabric): one passive mob with a `FleeAndBurrowGoal`.
- **Lycanites Mobs** (checked because the brief named it): the licence is conflicting and the only build
  is a 120 MB 1.20.1 alpha; Crusk and the worm creatures are single-bodied and hide only in
  `#lycanitesmobs:worm_burrowable` soft blocks.

### 4.7 Where oil fields are, and why it matters

OILGATE: oil exists only in `#c:is_desert` biomes, so the threat is a desert-surface and below-desert
one. The IaF Death Worm is already a desert creature (it spawns only in `minecraft:desert`), a fit with
that rule. Terra Entity's `giant_worm` limits (Y 20 or below, attack only below Y 50) need changing for
raids on desert fields.

## 5. Unverified (nothing here was run)

- How the IaF Death Worm looks and behaves with `noPhysics` through rock: whether a level rod reads as a
  diving worm, whether the parts keep up at speed, whether 7 invisible parts churn the tracker. Its speed
  and drift (terminal speed about 0.5 blocks per tick; a lunge in rock could coast 25 to 65 blocks
  upward) and the exact public setter for worm scale are also U. A GameTest spike decides (section 7).
- Every effort figure in hours and days, and the line counts of code not yet written.
- Whether the Terra Entity slim fork's GeckoLib 4.8.4 to 4.9.3 swap and `setId` behave in game.
- Whether Re-Logic tolerates Terraria-named or Terraria-looking fan mods, and whether CC BY-NC-SA blocks
  distribution through platforms that pay rewards (a question for Ben).
- Which Create blocks have block entities, which decides what Aberrant would spare.
- The cost and conflicts of Aberrant's 16 wall-walk mixins against Sable and Create contraptions.
- Per-frame client costs of every renderer (counts of boxes, entities and vertices only, no profiles).
- Whether Sculk Infection's authors would grant permission, and its stability under Connector.
- Whether TaCZ bullets, iron golems and In Control interact badly with hidden hitbox mobs (option 3).
- Whether variant A's collision hook could also physically stop the worm at pipes.

## 6. Ranked recommendation

Every option still needs the rtsim plugin work that round 1 section 4.2 describes: the trigger for works
underground at a field, spawning the puppet from a sim agent, owning the target, bounded damage with
`#mic_rtsim:worm_immune` sparing pipes, and retreat. The efforts below are the worm-specific part
(movement, spawn, body, damage seam).

**1. Ice and Fire CE Death Worm, driven by an rtsim plugin (variant B, no mixins).**
- Licence: nothing redistributed; IaF is LGPL-3.0 and only referenced.
- Footprint: zero added content, zero mixins; meets D3 and D4 outright (a plugin on one mod).
- Patch effort: 400 to 700 lines and 2 to 4 days with GameTests (est.). Variant A (2 mixin classes,
  about 60 lines) is available if Ben wants a collision hook.
- Weak points: round 1's tag-swap idea is dead (it needs the collision hook anyway); the body is a rigid
  level rod with invisible parts; 7 motion packets per tick per worm; the plugin owns targeting because
  the AI reaches 6 blocks vertically; natural desert worms (which never despawn) stay as they are; a
  400 to 700 line plugin that fights IaF's goals; unspiked.

**2. Terra Entity slim fork, worms only, with new art.**
- Licence: code LGPL-3.0 (publish source, ship the LGPL and GPL texts, mark as modified, rename); art
  must be replaced or the pack stays non-commercial and adapted art becomes CC BY-NC-SA.
- Footprint: 2 to 3 entity types, 0 items, 0 blocks, 0 mixins, one jar.
- Effort: 6 to 12 h trim (est.) plus art (9 models, 9 textures, 5 sounds). Proven movement and steering.
  D3 gray area (a bespoke entity from another mod's code, flagged).

**3. A plugin-owned worm from vanilla entities, design from MC Calamity (CC-BY-4.0, attribution).**
- Footprint: registers nothing; assets only if it should look like a worm.
- Effort: 400 to 800 lines, 3 to 5 days (est.). Weak points: network and tracker cost on a weak PC,
  hitbox behaviour with other mods, D3 gray area.

Not recommended: Aberrant Mobs (wrong shape, 17 hot-path mixins, one month old, spares all block
entities; useful only as a reference for tunnelling and the single `DigWorld.dig` choke point), Sculk
Infection (licence unclear), TerraMine, Apophis, Calamos and the Naga (references), both datapacks as
shipped, and the round 1 drops.

## 7. For Ben

**Recommended: use the Ice and Fire Death Worm that is already in the pack, driven by an rtsim plugin.**
It adds nothing to the pack (no jar, no entity, no item) and meets D3 and D4 directly. Unless told
otherwise I would spend one day on a GameTest spike in the Ice and Fire plugin (a puppet worm with
`noPhysics` in rock, steered through a stone slab, counting blocks changed and measuring ms per worm),
because that is cheap and undoable. If it works and you accept how it looks, build it. If the look or the
behaviour fails, fall back to the Terra Entity worms-only fork (option 2).

Things that need your decision (outward-facing or yours to bless), recommended answer first:

1. **How the worm looks.** Recommended: accept the Death Worm's model (a rigid body that does not bend
   while diving) if the spike looks fine. Alternative: a worm-only fork with a bending model (option 2) or
   the vanilla-entity worm (option 3).
2. **Pipes: steer around or physically block.** Recommended: variant B, the worm passes through
   everything and rtsim steers it away from pipes and filters what it breaks (no mixins). Alternative:
   variant A, two mixins that add a collision hook (whether pipes would then stop it is U).
3. **D3, only if the fork or the vanilla-entity worm is chosen.** A slim fork of Terra Entity's worm or a
   plugin-owned worm of vanilla entities is a bespoke-entity gray area. Do you bless it as "absolutely
   necessary"? Not needed for option 1.
4. **Art licence, only if option 2.** Recommended: author new art, so the pack stays free of the
   NonCommercial and ShareAlike terms. Alternative: keep the CC BY-NC-SA art, fine for a free pack but it
   forbids ever monetizing it and makes adapted art CC BY-NC-SA.
5. **Terraria names and looks.** Recommended: neutral names and original designs for anything new,
   whatever the option (Re-Logic's stance is unknown).
6. **Open licence conflicts, only if the mod is used.** Sculk Infection (jar CC0, stores ARR) and, from
   round 1, Aftershock (MIT in the jar, CC-BY-NC-4.0 on Modrinth): both need the authors to settle them.
   Aberrant Mobs is AGPL-3.0, compatible with rtsim.

## 8. Corrections to round 1

- The Ice and Fire Death Worm does not swim through sand in this pack: it cannot enter blocks at all
  (section 4.1). Round 1 section 3 row 4 ("Sand only: hard-coded `BlockTags.SAND`") and section 4.4
  option 2 rested on that.
- Terra Entity's `noPhysics` is set permanently in the constructor, not from `isInWall` (section 4.2).
- Terra Entity's repository of record is `Magic-team-jvav/TerraEntity`; the URL round 1 used is stale or a
  fork.
- CustomHitboxLib (round 1 section 4.4 option 3) would not shorten a worm: it gives rigid offset
  hitboxes, while NeoForge `PartEntity` already does that job.
