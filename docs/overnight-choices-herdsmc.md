# Overnight choices: the herds in the game (2026-10-20)

**Overnight agent 22.** Reversible choices made while Ben was asleep, one line each with the reason,
numbered Y#. None is in DECISIONS.md, PLAN.md or OPEN-QUESTIONS.md; each can be undone or put to Ben.
Branch `overnight/herds-mc`. The work: rtsim-ecology's migrating herds (choices H2-1 to H2-35 in
docs/overnight-choices-herds.md) wired into the native engine (`crates/rtsim-jni/src/engine/herds.rs`),
and the core's vanilla herd adapter (`neoforge/core`, package `herd`) that embodies them as sheep,
cows, pigs, horses and goats (docs/boundary.md §8). Numbers are PLACEHOLDER unless marked measured.

## Vanilla facts checked in the 1.21.1 sources (2026-10-20)

Checked in NeoForm's decompiled 1.21.1 sources with NeoForge 21.1.249's patches
(`sourcesAndCompiledWithNeoForge` in the Gradle cache). The herds' own facts (entity type ids, sheep
colour odds, horse `Variant`, goat screaming, `Age`) are in docs/overnight-choices-herds.md and still
hold; these are the ones the adapter adds.

- `Mob.goalSelector` is a public final field; `Mob.serverAiStep` ticks it for every mob, goats
  included. Sheep, cows, pigs and horses steer by goals (sheep: float 0, panic 1, breed 2, tempt 3,
  follow parent 4, eat block 5, stroll 6, look 7 and 8; cows: panic 1, breed 2, tempt 3, follow
  parent 4, stroll 5). Goats steer by a brain (`GoatAi`), whose memories include `WALK_TARGET`;
  `WalkTarget(Vec3, float, int)` is public.
- `Horse.finalizeSpawn` and `AbstractHorse.finalizeSpawn` draw the coat, markings and attributes from
  `level.getRandom()`, not the horse's own random; `AbstractHorse.randomizeAttributes(RandomSource)` is
  protected and `Horse` overrides it: max health `15 + nextInt(8) + nextInt(9)`, speed
  `(0.45F + 3 × nextDouble() × 0.3) × 0.25`, jump `0.4F + 3 × nextDouble() × 0.2`, in that order.
- `Horse.setTypeVariant` and `setVariantAndMarkings` are private; the NBT key `Variant` (int) is read by
  the public `readAdditionalSaveData`, as are a sheep's `Color` (byte) and `Sheared`, a goat's
  `IsScreamingGoat`, `HasLeftHorn` and `HasRightHorn`, and `AgeableMob`'s `Age` and `ForcedAge`.
- `Goat.finalizeSpawn`: `GoatAi.initMemories`, screaming with chance 0.02, and an adult loses one horn
  (left or right on a coin) with chance 0.1; `ageBoundaryReached` takes a baby's horns and gives an
  adult both.
- `Mob.finalizeSpawn` is `@Deprecated` and `@ApiStatus.OverrideOnly` in NeoForge (it wants
  `EventHooks.finalizeMobSpawn`, which posts `FinalizeSpawnEvent`); calling it directly is allowed and
  posts nothing.
- NeoForge disables `Sheep.mobInteract`'s own shearing (`if (false && ...)`) and shears through
  `IShearable` in `ShearsItem.interactLivingEntity`, which `Player.interactOn` reaches after
  `CommonHooks.onInteractEntity` (`PlayerInteractEvent.EntityInteract`).
- `Leashable.isLeashed()` and `getLeashHolder()`, `AbstractHorse.isTamed()` and `tameWithName(Player)`
  are public; `AbstractHorse` is an `OwnableEntity`.
- `Mob.createMobAttributes` gives every mob a follow range of 16: a path to a slot farther than that is
  a partial one, toward it.

## The engine side (rtsim-jni)

| # | Choice | Why |
|---|---|---|
| Y1 | **`crates/rtsim-jni/src/engine/herds.rs`**, beside `dragons.rs` and `ships.rs`, which it mirrors: one-line hooks in `engine.rs` at observe (SESSION_START, the latest-wins arm, the `Replay::Apply` arm, `op_of`), simulate, write (after the ships' queue), save (its page rides with the industry, dragon and ship pages, deleted by table when gone), start and orders. The herds join the shared services with one line each in `shared::with_viewers` and `shared::run_to` (their `Viewer` implementation was the herds branch's, H2-16), plus `shared::introduce_herds`: when the herds start running in the middle of a session (a first herd founded, the switch turned on), every area booked meanwhile is routed to them alone, so a player standing still sees a herd that is near. | The brief: wired the way the others are. Without the introduction a herd founded beside a player who does not move is never seen: areas are routed only when booked or replanned. |
| Y2 | **The switch is `Module.ECOLOGY`** (bit 4), the dragons' bit, with U10's rule unchanged (U10 is open): off, the herds ignore their records (still admitted by `Replay` for their sequence and op id, then counted), run nothing, see no area, write nothing (their queue is dropped) and keep the page they had; on again, their bodies of before are gone (a session reset) and they run on from where they stopped, and meet the areas booked meanwhile (Y1). | PLAN.md §10.2 puts herds in "Ecology and herds"; a new `Module` would be a new server-config switch (outward-facing). Both rules stay one rule. |
| Y3 | **The engine's climate is `Uniform`** (grassland at 15 °C everywhere, every cell passable) and its seed a constant, `HERD_SEED` (PLACEHOLDER, as the dragons' S14 and the ships' U1). | H2-8: no climate provider exists yet (Serene Seasons and Deep Time are later plugins); the synthetic valley is a test world. A herd in the native engine may therefore walk onto water. |
| Y4 | **Herds are founded only by a test order, HERD_FOUND (order 28, TEST mode)**: a species, a dimension (Java's id, the herds' frame), a position, a count and a seed; applied when the engine takes the unit, its first decision at once. Nothing in the game founds a herd yet. | No provider tells the engine where grassland or which biome is; founding herds blind would put sheep in oceans. **Recommendation for Ben (open):** a habitat record from the bridge (biome and surface samples per region as chunks generate, in the herds' latest-wins range) and founding per region by a hash of the region, when first sampled; or adopting vanilla's chunk-generation animals as a herd's seed (Y20). |
| Y5 | **HERD_COMMAND, order 27 (TEST mode)**: a herd command record in Java's ids (species, dimension) with the species' dictionary entries, queued as the herds' own (their sequences and DICT records). The herds know nothing of a scripted agent: Java's records about it are counted late and ignored. | As DRAGON_COMMAND (Z2) and SHIP_COMMAND (U15): the adapter's GameTests drive MATERIALIZE, STEER and DESPAWN on the native engine. |
| Y6 | **Pages: table 0x4800 ('H')**, one page (`RTHL` version 1, the state hash, `Herds::save`), checked on load; dropped loudly (the herds start afresh) when unreadable or without the phase 1 world it was saved with, whose leg index holds the herds' legs and the players' areas. While herds exist the phase 1 link snapshots at every save (the herds taint it when they run an event or take a record). **Every herd command is per session** (puppets never outlive one), so none is kept or resent, and none carries an op id; Java's op ids on HERD_KILLED and HERD_TAKEN go through `Replay::admit` (`herds::op_of`) and the herds' own op set (H2-21). | The dragons' and the ships' pattern (S8 to S14, U1, U4). Restore gives the saved state hash (the protocol test checks it after a crash and after a clean close). |
| Y7 | **With the dragons running, they hunt the herds**: `dragons::run_engine` runs `Dragons::run_until_with_herds` when the herds run too (the herds' events inside the dragons' time order), and the herds' own round of the shared clock then finds nothing left. | H2-23 to H2-25: the herds were written as the dragons' prey; leaving them apart in the native engine would make every herd safe from every dragon. Neither module's protocol tests have the other, so neither changes. |
| Y22 | **`nHerdInfo(handle, agent, long[])`** (JNI, `Engine::herd_info`): a herd's alive count, kills, takings, losses, embodied epoch and members, dead-set size and last epoch, by its group's agent id; `NativeEngine.herdInfo`. | The native GameTest checks that a kill drops the engine's own count; `nPhase1Stats` is the precedent. |

## The vanilla herd adapter (Java, the core's `herd` package)

| # | Choice | Why |
|---|---|---|
| Y8 | **The adapter is a `SessionPlugin` the core registers itself** (`HerdBridge`, `HerdEvents.register()` in `MicRtsim`): it owns the command range 0x8700 to 0x87FF and agent namespace 0x48, and its bodies carry the core's `mic_rtsim:agent` tag (so the core's save mixin keeps vanilla from saving them, and the core asks the plugin whether a tagged copy joining is live). `SessionPlugin` gains `take(session, entity, tag, how, player)` (default false), and `RtsimSession.takeEntity` asks the owning plugin first: taming (`AnimalTameEvent`, the core's hook) and Carry On's pickup (`RtsimApi.takeFromSim`, `mic_rtsim_carryon`) reach herd animals with no new hook. | Herd records are the herds' own (HERD_MATERIALIZED and so on, §8), not the core's MATERIALIZED and MEMBER_STATE, so the core's group machinery cannot drive them, as with the dragons (Z1). "Vanilla is built in": no plugin project. Additive: no core behaviour changes for the core's own puppets. |
| Y9 | **With `Module.ECOLOGY` off in Java, every herd group is captured** (reason 4, session end) at the end of the tick. | The engine writes no herd command while off (Y2), so nothing would ever steer or take back a group left standing; the engine takes its herds back as they were when switched on. |
| Y10 | **A body is made as vanilla makes an animal, then given the engine's look**: `EntityType.create`, `finalizeSpawn` called directly with spawn type EVENT (no `FinalizeSpawnEvent`, so no other mod re-rolls the look), then the entity's own NBT read (`addAdditionalSaveData`, the keys set, `readAdditionalSaveData`): `Color` and `Sheared` (never sheared as a lamb), `Variant`, `IsScreamingGoat`, `HasLeftHorn` and `HasRightHorn`, `Age` (`vanillaAge`) and `ForcedAge` 0; then full health. | docs/boundary.md §8.5's "its variant NBT, `Age` = vanillaAge, `Sheared` for flag 1"; the NBT path is vanilla's own, so the body is what such an animal loaded from disk would be (the dragons' Z3). EVENT is the spawn type that claims least about where an animal came from. |
| Y11 | **No y in HERD_MATERIALIZE**: every member stands on the highest motion-blocking, non-leaf block of its own column (the core's SURFACE rule), whatever flag 1 says; the ticking test reads the column's chunk at sea level, before the surface is read. | The record has no y (§8.4); the engine sets flag 1 always. The height map is read only once the column is known to be entity-ticking. |
| Y12 | **The identity seed** (§8.2): a horse's max health, speed and jump come from `AbstractHorse.randomizeAttributes` fed `RandomSource.create(seed)` (an invoker mixin, PATCHES.md row 21); a goat's lost horn from the same random (one adult in ten, as vanilla rolls it); the body's yaw from it. | Vanilla draws these from the level's random (verified), so the same horse would come back with other stats; the invoker runs vanilla's own roll, not a copy of it. |
| Y13 | **The spawn quota**: a materialization spends the group's size (a group bigger than the quota goes alone at the start of a tick, as the core's own); despawns and captures spend none. | §2.4; the dragons count spawns only (Z19), and a capture is the cheap direction. |
| Y14 | **Steering**: the group's anchor walks in a straight line from where it is to the waypoint, at the command's speed or, for a STEER with an arrive tick still ahead, so as to reach it then; each member keeps its place around the anchor (its spawn offset from the herd's centre). A member more than 8 blocks from its place (PLACEHOLDER) is walked back by `HerdFollowGoal` (priority 4, flag MOVE, a new path every 20 ticks, until within 4); otherwise vanilla's goals run (grazing, strolling, following a parent; panic, breeding and tempting win over it). Goats, which run a brain, get their place as their `WALK_TARGET` every 20 ticks while they stray. | §8.5's "a leader with the others following" would drag one animal through the others; places around a moving anchor keep the herd's shape and pace (sheep walk 400 blocks a day, H2-6, a block a minute), and a goal keeps vanilla's own priorities. |
| Y15 | **The obstacle rule** (§8.3's PLACEHOLDER made concrete): a member more than 14 blocks (8 + 6) behind the anchor's progress toward the waypoint that has gained less than a block for `stallTicks` (600 ticks, 30 s, PLACEHOLDER) has failed to pass; when at least half the members present have, HERD_OBSTACLE goes once per waypoint, with the failing count, the members present, where the failing members stand on average, and the waypoint. Checked every 20 ticks. | A fenced animal paces along the fence, so "made no progress toward the waypoint" is what tells it from a grazing one; a new waypoint (a STEER after the engine's replan) arms it again. |
| Y16 | **Takings (A10)**: a lead held by anyone (a player, or the fence knot a player tied it to) is TAKEN(lead) with the holder if a player; a tamed horse TAKEN(taming) with its owner; Carry On's pickup TAKEN(pickup); a body another mod or a command discards TAKEN(removed by another mod). The animal loses its tag and goal, keeps persistence (it is the player's now), and carries its HERD_TAKEN (`mic_rtsim:pending`) until the engine has it; the rest stays one group, and a group with no member left is answered HERD_DEMATERIALIZED(5) with no capture. | The core's rules for its own puppets (PLAN.md §6.2, §3.1), so a taken animal saved on a chunk unload before the journal never comes back as a herd member too. |
| Y17 | **Deaths**: `LivingDeathEvent` (lowest priority, not cancelled so far) marks the member; at the end of the tick, if it is really dead, HERD_KILLED with a Java op id and the killer: a player (1), a mob (2), another entity (3), or none (0, a fall, a `/kill` with no source entity); its drops carry the record (§3.1). A capture in the same tick settles the group's deaths first, so a dead member is never captured alive. | §2.3's "deaths are journaled at the end of the tick once certain"; found while writing the capture: a despawn applied in the tick of a death would have lost the death. |
| Y18 | **Shearing**: the pass sees a member's `isSheared()` turn true and reports HERD_SHEARED, naming the player who used shears on it this tick (`PlayerInteractEvent.EntityInteract`), else no player (a dispenser). | NeoForge shears through `IShearable` with no event of its own; the state change is certain, the interaction names who. |
| Y19 | **HERD_GROUP_STATE** every 100 ticks per group (the group's centroid, members present, flag ticking); the engine reads nothing from it yet (counted). | §8.3's PLACEHOLDER interval; the herd's position is its closed form, and the record is there for a later drift correction. |
| Y20 | **Vanilla's own animals are left alone (H2-28 stays open)**: the adapter neither suppresses nor adopts vanilla's natural or chunk-generation spawns; herd animals are extra bodies. **Recommendation:** keep it so until a habitat provider exists (Y4); then adopt, not suppress: chunk-generation animals of the five species become the founders of a herd for their region, as the dragons adopt lair dragons (P13), since suppressing would empty every place no herd passes. | The brief: leave vanilla spawning alone and log the recommendation. Adopting needs the founding rule Y4 lacks. |
| Y21 | **GameTests** (core, each in its own batch): six scripted on both engines (HERD_COMMAND on the native one), and one on the native engine only (skipped on the stub), whose herd is founded with HERD_FOUND at a random place 3,000 km out with its 7 × 7 chunks forced, so an earlier run's herds (the GameTest world and pages are reused, B37) are never there; the test writes PLAYER records for its player itself (GameTest players are not in the server's player list). The fence test cuts `stallTicks` to 60. | The native engine decides materializations only for a player near a herd; the stub decides nothing. |
| Y23 | **Breeding is vanilla's**: two herd puppets a player feeds breed, and the young is an ordinary animal, not in the herd and not reported. | The young is the player's doing, as a bred dragon's hatchling is not the sim's; no record for it exists in §8. |

## Results

See the reply that merged this branch for the final counts; the GameTests' `[gametest]` lines are in
`neoforge/core/build/gametest.log`.

## Not done

- **No herd is ever founded in the game** (Y4): the module runs only after a test's HERD_FOUND. The
  climate is `Uniform` (Y3).
- **HERD_GROUP_STATE is not read** by the engine (Y19); a body that drifts far from the herd's closed
  form is not corrected.
- **Not covered by a GameTest**: goats' brain steering over a long walk; a dimension change of a member
  (captured, by inspection); the session-end and switch-off captures; a dispenser shearing; vanilla's
  natural spawns beside a herd.
- **Vanilla adoption or suppression** (H2-28, Y20) stays open.
