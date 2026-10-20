# Overnight choices: the Ice and Fire plugin (2026-10-20)

Reversible choices made on the night of 2026-10-20 while writing `mic_rtsim_iceandfire` (A19), the
Java side of phase 2's dragons (`neoforge/iceandfire`, docs/boundary.md §6). One line each with the
reason, numbered Z#. None is in DECISIONS.md; each can be undone or put to Ben. Numbers marked
PLACEHOLDER are not measured. Branch `overnight/iceandfire`.

The engine side was done and wired before this branch (`crates/rtsim-ecology`, `crates/rtsim-jni/src/engine/dragons.rs`,
choices R1 to R31 and S8 to S14); this branch adds one test order to it (Z2) and otherwise follows its
records and the fake plugin in `crates/rtsim-jni/tests/common` (`dragon_obs`, `dragon_command`).

## Ice and Fire facts checked in the jar (2026-10-20)

Checked with `javap -p -c` in the pack's `iceandfire-2.1-beta.1.jar` (and its data), the pack's
`config/iceandfire/iaf-common.json`, In Control 10.3.0 and NeoForge 21.1.249's sources. Everything here
was read in the bytecode or the data, not guessed; R#'s facts in docs/overnight-choices-dragons.md still
hold and are not repeated.

- **`DragonBaseEntity.addAdditionalSaveData`** writes `Hunger` (int, `getHunger`), `AgeTicks` (int,
  `getAgeInTicks`), `Gender` (bool, `isMale`), `Variant` (string, `getVariant`), `Sleeping` (bool,
  `isSleeping`), `TamedDragon`, `FireBreathing`, `AttackDecision`, `Hovering`, `Flying`, `DeathStage`,
  `ModelDead`, `DeadProg`, `Tackle`, `HasHomePosition`, `CustomPose`, the home through
  `HomePosition.write` (`HomeAreaX/Y/Z` ints, `HomeDimension` string), `AgingDisabled`, `Command`,
  `Items`, `CrystalBound`, `BrushedTime`; it also calls `removeParts()` (saving a live dragon drops its
  multipart parts until a later tick). **`readAdditionalSaveData`** reads each back through its setter
  (`Sleeping` goes to `setInSittingPose`), reads the home only when `HasHomePosition` is true and all
  three coordinates are non-zero, and ends with `setConfigurableAttributes()` and
  `refreshDirtyAttributes()`.
- **`Variant` is `DragonColor.getName()`, a plain colour name** (`red`), not a namespaced id: the
  `DragonColor` constructor appends itself to its `DragonType.colors()` list, and `IafDragonColors`
  constructs fire red, green, bronze, gray; ice blue, white, sapphire, silver; lightning electric,
  amethyst, copper, black, in that order. `finalizeSpawn` and both lair pieces pick
  `RandomHelper.randomOne(dragonType.colors()).getName()`. This settles boundary.md §6.5's UNVERIFIED
  item: the variant index is the position in the species' list.
- `setHunger` clamps to 0 to 100. `getDragonStage` is 1 below 25 days, then 2 to 5 at 25, 50, 75, 100;
  `isBaby` is stage below 2; `isAllowedToTriggerFlight` needs not a baby, not sleeping, no passengers.
- **Persistence:** `isPersistenceRequired()` and `requiresCustomPersistence()` both return true, and
  `checkDespawn()` calls the vanilla one only when the config's `canDespawn` is true: a dragon never
  despawns by distance whatever that key says. The plugin still sets persistence on its puppets.
- **`finalizeSpawn`**: random gender, `growDragon(1 to 80 days)`, a random colour, awake, healed, hunger 50.
- **Lair dragons never pass through `finalizeSpawn`** (the task's UNVERIFIED item): `DragonRoostPiece.spawnDragon`
  and `DragonCavePiece.createDragon` call `EntityType.create(level)`, `setGender`, `growDragon`
  (40 days for a roost), `setAgingDisabled(true)`, `setHealth(getMaxHealth())`, a random colour,
  `absMoveTo`, `homePos = new HomePosition(pos, level)` and `setHunger(50)`, then
  `WorldGenLevel.addFreshEntity`. The roost also sets `hasHomePosition = true`; the cave does not, and puts
  its dragon to sleep (`setInSittingPose(true)`). The roost spawns its dragon only in the chunk holding the
  start's pivot (the box's centre column), on the `WORLD_SURFACE_WG` height plus 1.5. NeoForge's
  `FinalizeSpawnEvent` is injected into vanilla's calls of `finalizeSpawn` only, so no spawn type is seen
  for a lair dragon: the lair's box decides (boundary.md §6.6's fallback).
- **Death**: `die()` is vanilla's (so `LivingDeathEvent` fires) and then sets hunger; `tickDeath()` sets
  `ModelDead`, ejects passengers, and removes the body (KILLED) only once its `DeathStage` reaches
  `ageInDays / 5`: an older dragon stays as a corpse to harvest. `isAlive()` is true for a corpse not yet
  removed; `hurt()` refuses every damage to a corpse but falling out of the world.
- `hurt()` wakes a sleeping dragon and, when it is not tame, targets the attacker; it ignores IN_WALL,
  FALLING_BLOCK and CRAMMING damage.
- **Flight**: `public final IafDragonFlightManager flightManager`; `setFlightTarget(Vec3)` only stores it.
  `update()` replaces it with the attack target's position while the dragon has a live one, picks a new
  random one when the dragon is within 2 blocks of it, and clamps y to `maxFlight`.
  `isTimeToWake()` is `level.isDay()` (or command 2): a wild dragon materialized asleep by day wakes on its
  next tick (seen in the GameTests).
- **Homes**: `public HomePosition homePos`, `public boolean hasHomePosition`; `HomePosition(BlockPos, Level)`
  computes the dimension name itself; `getPosition()`.
- **Hoard piles** `iceandfire:gold_pile`, `silver_pile`, `copper_pile` have a `layers` property 1 to 8;
  their loot tables drop 2 per layer of `minecraft:gold_nugget`, `iceandfire:silver_nugget`,
  `iceandfire:copper_nugget`. This settles boundary.md §6.3's UNVERIFIED pile count.
- `#iceandfire:dragon_targets` (entity types) lists cow, sheep, pig, chicken, fox, horse, mooshroom,
  ocelot, llama, panda, parrot, polar bear, rabbit, trader llama, wolf, cat, pillager, strider, piglin,
  hoglin, zoglin; per-species tags `fire_dragon_targets` and so on exist beside it.
- **In Control** 10.3.0's `SpawnerSystem` spawns through `EventHooks.finalizeMobSpawn(mob, level,
  difficulty, MobSpawnType.NATURAL, null)`: its fire dragons arrive as NATURAL, which the engine leaves alone (R18a: OG-1, P13; it discards them only with `adopt_natural_dragons`).
- **Dependencies**: Ice and Fire CE needs Uranus (which needs Architectury), Jupiter (its config library,
  reading `config/iceandfire/iaf-common.json`) and GeckoLib; `DragonBaseEntity` implements Uranus and
  GeckoLib interfaces, so the plugin compiles against those jars too.

## Choices

| # | Choice | Reason |
|---|---|---|
| Z1 | **A core extension point, `SessionPlugin`** (`api/SessionPlugin.java`, `RtsimApi.registerPlugin`): a plugin owns a command kind range (the dispatcher hands it 0x8500 to 0x85FF), an agent namespace (the core asks it, not its own registry, whether a tagged entity joining a level is live, and tells it when one leaves), and the session's lifecycle (started, end of tick before the hand-over, stopping, failed); `RtsimSession.spendSpawnBudget` and `entitiesTickAt` are public for it. Dragons are not the core's `PuppetGroup`s. | Dragons speak their own records (DRAGON_MATERIALIZED ..., boundary.md §6), not the core's MATERIALIZED and MEMBER_STATE, so the core's group machinery cannot drive them; they still carry `mic_rtsim:agent`, so the core's save mixin keeps vanilla from saving them with no new mixin. Additive: no core behaviour changes for core puppets. |
| Z2 | **A DRAGON_COMMAND test order** (opcode 25, `crates/rtsim-jni/src/orders.rs`, `NativeEngine.dragonCommand`): a dragon command record with Java's species and dimension ids, queued as the dragons' own (their sequences, their DICT records). TEST mode only. | The GameTests must run on the native engine too, and the dragons decide MATERIALIZE, STEER and DEPART only when a player meets a flight; W17's INDUSTRY_COMMAND is the same answer for industry. Test-only because the dragons know nothing of an ordered agent (they ignore its records). Untagged dragons are answered by the engine itself on the native engine. |
| Z3 | **MATERIALIZE writes the state through Ice and Fire's own NBT read** before the body joins: save, set `AgeTicks`, `AgingDisabled` = true, `Hunger`, `Gender`, `Variant` (the colour's name), `Sleeping`, `Flying` and `Hovering` false, `HasHomePosition` false, read back (attributes refresh from the age), then the home straight into `homePos` and `hasHomePosition`, then the health (clamped to the max). | The NBT path is what a dragon loaded from disk goes through, so the body is in exactly the state such a dragon would be. The home goes in directly so the read's "a coordinate of 0 drops the home" trap never applies (no nudging needed). |
| Z4 | **Lairs are found two ways**: when a chunk holding a lair structure start loads (`ChunkEvent.Load`, flag LATE when the chunk came from disk), and when an untagged dragon joins inside one (`StructureManager.getStructureAt` over the six lair structures, reported before the DRAGON_UNTAGGED that names it). Each lair once per session, not persisted. | No mixin (the core's start mixin is village-only). The engine treats a lair it knows as a no-op, and an engine that dropped its pages (S12) relearns every lair as its chunks load, which a persisted "already reported" set would prevent. |
| Z5 | A lair's home (LAIR_FOUND) is the start box's centre; for a roost, standing on the surface there (`WORLD_SURFACE`) when that chunk is loaded, else the box's floor; for a cave, the box's centre. PLACEHOLDER. | The roost piece spawns its dragon at the start's pivot (the box's centre column) on the surface heightmap (verified); the cave's dragon stands inside the cave around its centre. |
| Z6 | **A dead puppet's corpse** (Ice and Fire keeps it to harvest) stays tagged, so never saved, until the engine acknowledges its DRAGON_DIED (pages on disk include the death), then becomes an ordinary entity; at a session stop every corpse becomes ordinary (the journal holding the death goes into that world save). No journal record is carried on the corpse. | A crash before the acknowledgement then loses both the death and the corpse (consistent); after it, both are kept. A carried DIED replayed in a later session could hit a later materialization of the same epoch (S9: dragon epochs come back as saved). Harvest drops in that window are the core's known gap (boundary.md §3.1). |
| Z7 | **A puppet a player tames leaves the sim (A10) as a DRAGON_DIED with killer kind none**; its tag goes and the body is the player's. Seen at `AnimalTameEvent`, and by the puppet pass when a puppet turns up tame (a path that fired no event). | The dragon records have no TAKEN. A DEMATERIALIZED would leave the sim's dragon alive beside the player's (two dragons); a death with no killer removes it with no grudge. In Ice and Fire only hatchlings are tamed (R18's facts), so this is for other mods' taming. For the engine author: a DRAGON_TAKEN kind would say it exactly. |
| Z8 | **C25 for carried prey**: a dragon and the prey riding it are one unit. On capture the prey goes with it: reported eaten (DRAGON_ATE, against the village it was taken from, recorded at `EntityMountEvent`) and removed; any other passenger (a player, another mod's puppet) is set down. Meals are journaled before the DEMATERIALIZED or DRAGON_DIED that ends the embodiment. | Ice and Fire carries prey off to eat it (R31's facts), so the prey's fate is the dragon's meal; dropping a cow from cruise altitude would be neither. The engine ignores a meal of a dragon no longer embodied, so order matters (found by the GameTest). |
| Z9 | **Untagged dragons**: each entity is reported at most once per session; tamed ones are reported too (flag 1) and the engine leaves them alone; with the ECOLOGY module off nothing is reported (vanilla behaviour, C25's switch). Joins and chunk loads are queued even before the session exists (the spawn chunks load first) and handled once it does; entries from another server (singleplayer) are skipped. | boundary.md §6.6 item 2 asks for every untagged dragon; once per session bounds the journal for a player's tamed dragons that load and unload. |
| Z10 | Spawn kinds from NeoForge's `MobSpawnType` (carried on a never-saved attachment, `mic_rtsim_iceandfire:spawn_type`, set at `FinalizeSpawnEvent`): STRUCTURE 1; NATURAL, CHUNK_GENERATION, PATROL, EVENT 2; SPAWNER, TRIAL_SPAWNER 3; SPAWN_EGG, DISPENSER 4; COMMAND 5; BREEDING 6; any other 7; none seen: 1 inside a lair's box, else 7. | The attachment travels with the entity from a worldgen thread to the join without shared maps. In Control spawns as NATURAL (verified), so its dragons are discarded (reason 1). |
| Z11 | **DEPART**: steer to the target; capture (DRAGON_DEMATERIALIZED, reason 1) once no non-spectator player in the level has the dragon's chunk within the simulation distance, or the timeout passes (0: 6,000 ticks, PLACEHOLDER). A DEPART for a dragon not embodied is answered DRAGON_DEMATERIALIZED reason 6 at once (a STEER for one is ignored). | boundary.md §6.5; the core answers a DESPAWN_GROUP the same way so the engine never waits. |
| Z12 | **Steering**: flying and the flight target are set again every tick until the dragon is within 6 blocks of the target horizontally (PLACEHOLDER); a live attack target wins; flag 1 (settle) lands it on arrival (`setFlying(false)`, `setHovering(false)`); a STEER after a DEPART is ignored. | Ice and Fire replaces the target when the dragon reaches it or wanders (verified); setting it once would lose it. |
| Z13 | Other losses of a body: unloaded, or removed by another mod or a command: DRAGON_DEMATERIALIZED reason 3 with the last state read as it left; not entity-ticking: reason 2 (checked every tick); a dimension change: reason 3; server stop or bridge failure: reason 4. | §2.3's reasons, as the core uses them for its groups. |
| Z14 | **ADOPT** tags the body as it stands (persistence, aging disabled, the home when flag 1), whatever its chunk's ticking (the pass captures it at once if it is not ticking); a body gone, dead, tamed or of another server is DRAGON_SPAWN_REJECTED(5), one that is already a puppet (4). | A body must never be left beside an abstract dragon. Lair dragons join at the edge of the loaded area, so most are adopted and then captured right away; the engine materializes them again when a player comes near (by design: bodies only near players). |
| Z15 | **DISCARD** removes the body with no drops (passengers set down) and answers COMMAND_RESULT(applied, its sequence, op 0); a tamed body or a puppet is refused (FAILED); a body already gone is applied. | boundary.md §6.5 (a repeated discard is harmless); A10 for tamed ones. |
| Z16 | **HOARD_ROBBED**: hoard blocks are the block tag `#mic_rtsim_iceandfire:hoard` (the three piles); a broken pile counts what its loot table drops for that player (`Block.getDrops`); a container inside a lair counts what left it between `PlayerContainerEvent.Open` and `Close` (removals only; single block-entity containers, a double chest's halves are not counted); summed per (player, lair) per tick into one record with a Java op id. | The pile count is the loot table (2 a layer), not a guess; a tag lets the pack add other hoard blocks. |
| Z17 | DRAGON_ATE's prey is a living non-player, non-dragon entity in `#iceandfire:dragon_targets` or any `Animal`, killed by a puppet (the damage source's entity); its village is `RtsimApi.villageOf` at the kill, or at the pickup for carried prey. | Ice and Fire's own target tag plus farm animals; the village's herd is what R19 books. |
| Z18 | DRAGON_STATE every 100 ticks per dragon (boundary.md's PLACEHOLDER), flag ticking always set (the pass just checked); DRAGON_HURT summed per (dragon, attacker) over one tick with the health at the end of the tick. | As boundary.md §6.3. |
| Z19 | MATERIALIZE counts one body against the core's per-tick spawn quota; an adoption counts none. | An adoption makes no new body. |
| Z20 | The GameTest server runs with the pack's own `iaf-common.json` (copied into its config folder before each run: dragon griefing off, `canDespawn` true, `neutralToPlayer` false) and loads Architectury for Uranus. Each GameTest has its own batch; the flying test forces the chunks within 3 of its arena (a GameTest forces only its structure's own). | Tests run as the pack does; dragons of two tests never meet; a flying dragon leaving the forced chunks would stop ticking and be captured. |
| Z21 | **The In Control edits P13 needs are a disabled proposal**: `neoforge/iceandfire/pack-proposal/config/incontrol/spawner.json` and `spawn.json` are the pack's files with the fire dragon entry removed and `iceandfire:fire_dragon` taken out of both `onjoin` rules (the blaze rules stay). Outside the jar, not applied. | Outward-facing pack change: Ben's (task rule 3, boundary.md §6.6 item 1). Until applied, In Control's dragons are reported NATURAL and left alone by the engine (R18a: OG-1, P13); they stay the oil gate's desert danger until the sim version is measured (OILGATE.md OG-10). Applying this proposal is Ben's call and belongs after the gate passes (it removes In Control's dragons). |

## Tests

`./gametest.sh iceandfire` (`neoforge/iceandfire`, namespace `mic_rtsim_iceandfire`, each in its own batch):

| Test | What it shows |
|---|---|
| `adragonmaterializeswiththeenginesstateandsteerstoitstarget` | MATERIALIZE: AgeTicks 720,000 with aging disabled, Hunger 37, male, variant 2 = `bronze`, health 60 of 135, the engine's home, tagged, persistence-required, never saved; it flies 8+ blocks toward its first target; STEER sets a new flight target and it turns; DRAGON_STATE (flying, ticking); DEPART captures it (reason 1) with health 60, hunger 37, age and position, and removes the body |
| `aplayershitishurtandakillisdiedwiththekiller` | two hits in one tick are one DRAGON_HURT (hits 2, the player's UUID, health after); a kill is DRAGON_DIED naming the player, after the lethal blow's HURT; the corpse stays tagged waiting for the engine's save |
| `untaggedeggdragonisadoptedandnaturaloneleftalone` | a spawn-egg dragon and a NATURAL one are reported DRAGON_UNTAGGED (spawn kinds 4 and 2, species, age, hunger, gender, variant); the egg dragon is adopted (tagged, DRAGON_MATERIALIZED naming it), the natural one is left alone (R18a: alive, no tag, no agent, saved as before). Native: the engine decided both |
| `alairsdragonisadoptedanditshoardrobbed` | a lair (LAIR_FOUND, structure and box) and a dragon joining in it with no spawn type: STRUCTURE of that lair, after the lair; adopted with the lair's home (native: the engine's own answer); a 3-layer gold pile broken and 4 ingots taken from a chest in the lair are one HOARD_ROBBED (6 nuggets, 4 ingots, the player, a Java op id) |
| `apuppetdragonisneversavedandacopyneverjoins` | vanilla's own entity-chunk save and read-back: the pig reloads, the dragon does not; copies of the puppet built from its NBT (tag included) under a new UUID or an older epoch are discarded as they join; one body for the agent |
| `atamedorfarawaypuppetleavesthesim` | a puppet tamed by a player: DRAGON_DIED with no killer, untagged, the body kept as the player's; a puppet carried 20,000 blocks away: captured (DRAGON_DEMATERIALIZED, reason 3 as it left tracking) and removed |
| `adragonsmealinavillageisreported` | a puppet kills a cow in an adopted village: DRAGON_ATE (1 cow, the village); a cow it carries when it departs is eaten as part of the capture, against the village, before the DEMATERIALIZED, and removed |

## Results

- **GameTests, native engine** (fresh GameTest worlds, `run-gametest` moved aside first): core 37 of 37
  (with `:core:test`), iceandfire 7 of 7.
- **GameTests, stub engine** (fresh worlds): core 37 of 37 (with `:core:test`), iceandfire 7 of 7.
- **Rust**: `cargo fmt --all --check` clean; `cargo clippy -p rtsim-jni --all-targets -- -D warnings`
  clean; `cargo test -p rtsim-jni` 21 + 14 + 13 passed (the orders round trip includes DRAGON_COMMAND).

## For the engine author (not changed here)

- **No DRAGON_TAKEN** (Z7): a dragon a player takes is reported as a death with no killer.
- **The engine ignores DRAGON_ATE, HURT and DIED of a dragon that is no longer embodied** (by agent and
  epoch). The plugin orders meals and wounds before the record that ends an embodiment (Z8); a death that
  reaches the engine only in a later session (a corpse's record carried across a crash) would be ignored,
  which is why no record is carried (Z6).
- **S10 is only reachable across a session**: a DRAGON_DEMATERIALIZED makes the engine forget which body
  it adopted (`adopted.retain` in `Dragons::ingest`), and within a session `by_entity` still holds the body,
  so a body reported again in the same session is not answered at all. Both are right for a real crash and
  reload (a new session, no capture); they mean the plugin cannot test S10 inside one GameTest session.
- **Lair dragons are adopted and then captured at once** (Z14): they join where entities do not tick yet.
  The roost's own dragon (40 days, Ice and Fire's colour and gender) is replaced by the engine's next
  materialization at the lair, with the engine's state.

## Incomplete

- **The pack's In Control edits** are not applied (Z21, Ben's).
- **Not covered by a GameTest:** a real lair structure (a start generated by world generation: the
  GameTest world has none, and `/place` registers no start in the chunks), so lairs found through
  `ChunkEvent.Load` and `StructureManager` were not run (the test declares its lair); settling on
  STEER flag 1; the capture paths for not ticking (reason 2), dimension changes and the session's end; a corpse untagged on the engine's acknowledgement (no world save in a GameTest); S10's
  re-adoption across a crash (above); HOARD_ROBBED from a double chest (not counted, Z16).
- **The hostile flag** sets the attack target only when the player is online in the dragon's level.
- **Hoards stay abstract** (R23): nothing places the engine's hoard into lair chests.
