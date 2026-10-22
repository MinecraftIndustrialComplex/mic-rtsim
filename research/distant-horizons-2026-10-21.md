# The sim in Distant Horizons: a far view (research, 2026-10-21)

Research only. Nothing was built, run or added to the pack. **V** means read in bytecode, code or data.
**U** means not verified. **E** means my estimate. Written for Ben's ask:

> "any way we could get our sim hooked up so we can see stuff via distant horizons. (Dragons flying around
> outside of render distance, seeing trains, and airships move between bases, something resembling the
> factories still whirring I.E. Massive windmills still spinning. Seeing a train / base / village go up in
> smoke when war is occurring etc."

Sources: the pack's DH jar (`DistantHorizons-3.3.1-1.21.1-fabric-neoforge.jar`, API 7.1.0, network protocol
16) read with `javap`, its shaders read as text; Create 6.0.10 and Sable 2.0.5 read with `javap`; this
repository's code; a web search for prior art, DH's changelogs and source, and Iris's source (section 9).
No jar was executed.

## 1. Short answer

**Yes.** DH 3.3.1 has a public API for drawing coloured boxes in its far view, and the boxes can move every
frame. It is enough for everything in the ask, with one hard limit: **boxes cannot rotate**. Anything that
turns (sails, a dragon changing course, a ship yawing) is drawn as a few pre-built poses that swap, like a
flipbook, or is rebuilt when its heading changes.

| Item | Verdict | What you would see |
|---|---|---|
| **Trains** | **Doable now.** No sim work. | Each carriage as a dark block about 3 wide and 3 tall, gliding along the track the far terrain already shows. On a diagonal it is a short staircase of boxes. Near you it becomes the real Create train. |
| **Dragons** | **Doable with sim work** (one new engine record). Embodied dragons seen from far away: doable now. | A dragon-shaped cluster of about 16 boxes in its species colour, flying in a straight line at cruise height, wings swapping between three poses. It circles while hunting and sits on its lair while roosting. |
| **Airships** | Loaded player ships: **doable now** (plugin work). Ships flying between bases while nobody is near: **doable with sim work** (VOYAGE). NPC fleets: **not until they have bodies** (P8). | The ship's blocky silhouette at 2-block resolution, moving smoothly, re-cut when it turns. Inside Sable's 320-block range it becomes the real ship. |
| **Windmills and factories** | **Doable with plugin work.** Spinning at the last real speed needs no sim work. Stopping when the sim says the factory stopped needs your call on I18. | The sails, which the far view does not show at all today, turning in 22.5 degree steps. Lit windows at night already come from the far terrain itself (U). |
| **Smoke and fire under attack** | **Doable with sim work** (one new engine record for war events). | Orange glowing specks over the raided base or village, flickering, under a dark column of rising puffs that thins out. Real damage (broken machines) appears when the chunk next loads. Villages get smoke but no real scars unless you decide otherwise. |
| **War bands on the march** | **Doable with sim work** (one route record per march; the engine already knows each march's route and times). | A column two abreast in its culture's colours, creeping along the road from its camp, bobbing as it walks; an Umvuthana Invasion led by a taller figure in sun-mask gold. Visible within about a kilometre. Near you it becomes the real band where the column was. |

**First visible slice:** trains, client side only, from Create's own map sync (`TrainMapSyncClient`), which
already sends every train's carriage positions to any client that asks. No server change, no sim change.

**What you decide (section 15.1):** whether the server may stream sim state to opted-in clients (PLAN §4.10
says no today); how I18 reads for a far view; moving to DH 3.3.3 so clients have far terrain at all (the
Chunky trap); whether raids leave real scars; how the Umvuthi is shown when its band is met on the
march.

**Limits worth knowing up front:**

- **Mac players see nothing.** DH skips all box rendering on macOS (V).
- **Players who turn off DH's "generic rendering" see nothing** (V). The plugin should notice and not ask
  the server for data.
- **Today, clients in this pack probably have no far terrain at all** because Chunky is installed on the
  client (section 10). Proxies would float over a void until that is fixed.

## 2. How this was done, and the gaps

- **Bytecode first.** Every DH claim in sections 3, 7 and 8 is from `javap -c` on the pack's jar or from its
  bundled GLSL. The Create train sync, windmill bearing and Sable tracking facts are from `javap` on the
  jars in the Deep Time run directory and the pack.
- **No spike.** A throwaway mod was allowed. I did not build one, because the bytecode answered the open
  questions (how to get the register, what material does, whether rotation exists, what costs a frame). What
  a spike would add is frame-time numbers, which belong in phase 1 (section 13).
- **Gaps.** No frame time, bandwidth or GPU cost was measured. Translucency and draw order are read from the
  shader and the GL state, not seen. Sable accessor names for a ship's pose and bounds are U.

## 3. What DH 3.3.1 gives (V unless marked)

### 3.1 Getting the register

- `DhApi.Delayed` holds static handles once DH is up: `configs`, `terrainRepo`, `worldProxy`, `renderProxy`,
  `wrapperFactory`, `customRenderObjectFactory`.
- A mod binds `DhApi.events.bind(DhApiLevelLoadEvent.class, ...)`. The event's `levelWrapper`
  (`IDhApiLevelWrapper`) has `getRenderRegister()`, which returns `IDhApiCustomRenderRegister` (`add`,
  `remove(long)`).
- **Only client levels have one.** `ClientLevelWrapper.getRenderRegister()` and the server wrapper both
  return `dhLevel.getGenericRenderer()`, and that is `null` for `DhServerLevel` (a dedicated server), and the
  client module's renderer for `DhClientLevel` (multiplayer) and `DhClientServerLevel` (single player).
- **Groups die with the level.** `ClientLevelModule.close()` closes the generic renderer, so a mod re-adds
  its groups on every `DhApiLevelLoadEvent` and drops its handles on `DhApiLevelUnloadEvent`.
- **The safest route to the register** is the render parameter: `DhApiRenderParam.clientLevelWrapper` (API
  5.1.0 and newer) reaches the controller every frame (6.4), so it always names the level being drawn. The
  Javadoc says the level load event may fire before the player is in, and `getSinglePlayerLevel()` is null
  in multiplayer.

### 3.2 Boxes and groups

- `IDhApiCustomRenderObjectFactory` makes groups: `createForSingleBox`,
  `createRelativePositionedGroup(String "ns:path", DhApiVec3d origin, List<DhApiRenderableBox>)`,
  `createAbsolutePositionedGroup`. The name must contain exactly one colon or the constructor throws.
- `DhApiRenderableBox` has public `minPos`, `maxPos`, `java.awt.Color color`, `byte material`. Constructors:
  `(min, float width, Color, EDhApiBlockMaterial)` for a cube, `(min, max, Color, material)` for a cuboid.
- `IDhApiRenderableBoxGroup` (core class `RenderableBoxGroup`, a `List` of boxes) has `setActive`,
  `setSsaoEnabled`, `setOriginBlockPos(DhApiVec3d)`, `setSkyLight` and `setBlockLight` (0 to 15),
  `setShading`, `setPreRenderFunc` and `setPostRenderFunc` (`Consumer<DhApiRenderParam>`), and
  `triggerBoxChange`. Defaults: active, SSAO on, sky light 15, block light 0, default shading.
- **Material does nothing in the built-in shader.** `EDhApiBlockMaterial` (UNKNOWN, LEAVES, STONE, WOOD,
  METAL, DIRT, LAVA, DEEPSLATE, SNOW, SAND, TERRACOTTA, NETHER_STONE, WATER, GRASS, AIR, ILLUMINATED) reaches
  the vertex shader as `aMaterial` and is never read. It exists for Iris shader packs.
- **Light is per group, through the lightmap.** The vertex shader samples `uLightMap` at
  `(uBlockLight, uSkyLight)` and multiplies the box colour. So a group with block light 15 glows at night,
  and dropping it to 12 dims it. That is how fire flickers cheaply.
- **Shading** is six per-face multipliers (`DhApiRenderableBoxGroupShading`, `getDefaultShaded()`,
  `getUnshaded()`).
- **Alpha is kept.** The fragment shader writes `lightColor * aColor` with the box's alpha, and the renderer
  turns on blending with `glBlendFuncSeparate(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, ONE, ONE_MINUS_SRC_ALPHA)`.
  Translucent boxes are possible. Whether depth writes and unsorted draw order make them look wrong is U.

### 3.3 What moves cheaply, and what costs

- **No rotation.** The instanced vertex shader builds a scale and translate matrix per box from `aScale` and
  `aTranslate*`, and nothing else. Boxes are always axis-aligned.
- **Cheap every frame:** `setOriginBlockPos` (copies three doubles; the shader splits them into chunk and
  sub-chunk parts, so motion is smooth far from 0,0), `setActive`, `setSkyLight`, `setBlockLight`. These are
  uniforms.
- **Costly:** changing the box list. `triggerBoxChange` marks the group dirty;
  `tryUpdateInstancedDataAsync` rebuilds the instance buffer on DH's render-loading executor and swaps it in
  on a later frame. Fine a few times a second across all groups, wrong every frame.
- **Per frame, per group:** the renderer walks its `ConcurrentHashMap` of groups twice (SSAO groups, then the
  rest). For every group it calls the pre-render function **before** checking `active`, fires
  `DhApiBeforeGenericObjectRenderEvent` (which can cancel it), and issues one instanced draw if active. There
  is **no frustum or distance culling**. `DhApiBeforeGenericRenderSetupEvent` fires once per pass, which is the
  place for one controller to update every proxy.
- **Flipbooks** therefore cost one inactive group per extra pose (a map entry and an optional lambda call) and
  one draw for the active pose.

### 3.4 Where boxes draw, and the near clip

- `LodRenderer.renderTerrain` order: LOD opaque, custom objects with SSAO, SSAO, custom objects without
  SSAO, LOD transparent, DH fog. DH fog is a later pass, so proxies fade into the distance with the terrain
  (reasoned from the order; not seen).
- **Proxies are not hidden near the player.** The generic shader uses DH's projection, whose near plane is
  `min(nearClip, 7.5)` blocks (`RenderUtil.setDhProjectionMatrix`). Unlike LOD terrain, a proxy 50 blocks away
  is drawn. Vanilla draws over it, so it would show against the sky and vanish behind any nearby block, in
  front or not (U, reasoned). **The mod must hide proxies inside the vanilla view itself.** DH's own beacons
  do exactly that (3.6).

### 3.5 Settings, platforms and fallbacks

- `IDhApiGenericRenderingConfig` at `DhApi.Delayed.configs.graphics().genericRendering()`:
  `renderingEnabled()`, `beaconRenderingEnabled()`, `cloudRenderingEnabled()`, all default true (config
  `Client.Advanced.Graphics.GenericRendering.enableGenericRendering`; comment: "If true non terrain objects
  will be rendered by DH. i.e. beacon beams and clouds."). Off means `LodRenderer` skips custom objects.
- `graphics().chunkRenderDistance()` gives the LOD radius in chunks; the plugin subscribes for that radius.
- **macOS:** `GlGenericObjectRenderer.render` returns at once on `EPlatform.MACOS`; the log says "Generic
  rendering not supported by Mac. Clouds, beacons, and some other effects will be disabled".
- **No instancing:** it falls back to direct drawing and logs "Generic object rendering will be slow".
- **Iris:** a shader pack can replace the program through `OverrideInjector` and
  `IDhApiGenericObjectShaderProgram` (`overrideThisFrame`). Whether boxes then show, and how material is
  used, depends on the pack (section 8).

### 3.6 DH's beacons, the model to copy

`BeaconRenderHandler` builds one absolute group, `DistantHorizons:Beacons`, of ILLUMINATED boxes at light 15,
unshaded, SSAO off. Its pre-render function turns it on or off from config. An async culler, about once a
second, removes beams within the near-clip horizontal distance. A far view does the same: light and shading
per group, a cheap pre-render toggle, and a slow cull of whatever is near.

### 3.7 The terrain repo

`IDhApiTerrainDataRepo` reads (`getSingleDataPointAtBlockPos`, `getColumnDataAtBlockPos`,
`getAllTerrainDataAt...`, `raycast`) and writes (`overwriteChunkDataAsync(levelWrapper, Object[])`). The
write wants a chunk wrapper built by `WrapperFactory.createChunkWrapper` from a real Minecraft `ChunkAccess`
plus `Level`, and goes through `SharedApi.applyChunkUpdate` into the client's DH database.

**Recommendation: never write it.** It needs a fabricated chunk, it persists in each client's own database,
and it would show damage the world does not have until the chunk loads. **Reading it is useful:** a column
query gives the far ground height where fire and smoke should sit.

## 4. Per item

### 4.1 Trains

- **What the sim knows:** almost nothing. `TrainWatch` (mixins on `Train.arriveAt` and `leaveStation`) sees
  station arrivals and departures. `CreateIds.of(UUID)` is a one-way hash. The sim has no track geometry.
- **What Create knows:** everything, all the time. Create trains move on the server whether or not their
  chunks are loaded (`Create.RAILWAYS.trains`), so the position of every carriage is always real.
- **Create already ships it to clients (V, `com.simibubi.create.compat.trainmap`):**
  - `TrainMapSyncClient.requestData()` sends `TrainMapSyncRequestPacket`, at most every 5 client ticks.
  - The server keeps requesters in a `TickBasedCache` that expires after 20 ticks, and every 5 ticks sends
    `TrainMapSyncPacket` (a light flag plus `List<Pair<UUID, TrainMapSyncEntry>>`) to them only.
  - Each entry: `Float[] positions` (6 per carriage: leading x, y, z, trailing x, y, z, from
    `TravellingPoint.getPosition(graph)`), dimensions, `state`, `signalState`, `fueled`, `backwards`,
    `targetStationDistance`, `ownerName`, `targetStationName`, `waitingForTrain`.
  - The client keeps `public static Map<UUID, TrainMapSyncEntry> currentData` with
    `getPosition(i, leading, partialTicks)`, which interpolates. `stopRequesting()` clears it.
  - No server config gates it. Normally only JourneyMap, FTB Chunks or Xaero full-screen maps call it, and
    the pack has none of them, so today nobody asks.
  - **Size:** about 25 + 45 bytes per carriage per train (E), so a 4-carriage train is about 225 bytes, and
    50 such trains about 11 KB per packet, about 45 KB/s per requesting player. It sends every train in the
    world, near or far.
- **What is missing:** looks. The entry has no colour or shape. `CreateClient.RAILWAYS.trains` on the client
  has each train's name and map colour, but no positions; the client never ticks trains.
- **As boxes:** one relative group per carriage. The box set is a straight run from trailing to leading point:
  one cuboid on straight axis-aligned track, a staircase of 1-block-long cuboids on a diagonal or a slope
  (about 8 boxes for an 8-block carriage, E). Default size 3 wide by 3 tall (E). Colour: the train's map
  colour or a dark grey. Each frame the controller moves the group's origin to `getPosition`; the box set is
  rebuilt only when the carriage's heading changes by more than about 10 degrees (E), which happens on curves.
- **Terrain repo:** no.
- **Verdict: doable now, no sim work.** This is the first slice. Phase 2 replaces Create's all-trains stream
  with our own interest-filtered one at 1 Hz (section 6).
- **War:** I24 says raids loot carriage cargo and never cut track. When the conflict engine records such a
  raid, the war-event effect (4.5) can sit on the carriage for a while. Whether today's records carry the
  train's position is U.

### 4.2 Dragons

- **What the sim knows (V, `crates/rtsim-ecology`):** every dragon's exact position at any tick.
  - States: `Roosting`, `Flying(Purpose)`, `Hunting { target, until }`, `Embodied { .. }`, `Leaving`.
  - A flight is a `FlightLeg { path, y, stamp }` (`dragon.rs:242`). `path` is the shared straight leg
    `Path { frame, x0, z0, x1, z1, t0, t1, radius }` and `Path::at(t)` is a lerp (`rtsim-travel/src/legs.rs:44,
    58`); `y` is a constant cruise height, 180 by default (`species.rs:145`). Speeds 0.45 to 0.6 blocks a
    tick by species. Hunting is a `flight::stay` path: somewhere within a radius until a booked end.
  - Legs are booked on events, not stepped, so a far view needs a message per leg, not per tick.
  - Looks: `species` (Fire, Ice, Lightning), `variant` 0 to 3 (the colour), age stages. How big each stage is
    drawn by Ice and Fire is U.
  - A dragon becomes `Embodied` (a real Ice and Fire entity) within `view_radius` 160 of a player, spawning at
    the edge plus 32 (`tunables.rs:188`). `DragonBridge` in the Ice and Fire plugin reports embodied state
    to the engine every 100 ticks.
- **What is missing:** the engine never tells Java about a leg. The 0x85xx dragon commands
  (`docs/boundary.md`: MATERIALIZE, STEER, DEPART, ADOPT, DISCARD, PLACE_LAIR, OIL_SURVEY) all concern
  embodiment. A far view needs one more engine-to-Java record: the leg (id, species, variant, stage, start
  and end point, start and end tick, height, kind: fly, hunt, roost), sent when a leg is booked, cut or
  ended. Dragons are few (one per lair), so this is a trickle.
- **As boxes:** one body group (body, neck, head, tail: about 12 boxes) and three wing groups (up, level,
  down: 2 to 4 boxes each), so 4 groups and 2 draws per dragon (E). Built for the leg's heading, rounded to
  22.5 degrees; rebuilt only when the heading changes, which on straight legs is once per leg. Origin moved
  every frame from `Path::at` with the client's game time. Wings swap pose a few times a second. Hunting is
  drawn as a slow circle at the stay radius (cosmetic; the sim does not say where in the radius it is).
  Roosting is drawn lying on the lair, wings folded.
- **Embodied dragons seen from far away:** the real entity exists on the server near one player. Other
  players far from it get its position from the Ice and Fire plugin at 1 Hz (no sim work).
- **Terrain repo:** no.
- **Verdict: doable with sim work** (the leg record). Embodied ones seen from afar: doable now.

### 4.3 Airships

- **Real ships (Sable 2.0.5, Aeronautics 1.3.2, V):**
  - `SubLevelTrackingSystem` uses `sub_level_tracking_range`, default 320 blocks, in Sable's common config.
    The pack ships no Sable config, so 320 applies.
  - A player entering range gets a full sync (`ClientboundStartTrackingSubLevelPacket` and the plot's chunk
    data); movement then comes in `ClientboundSableSnapshotDualPacket` (pose, linear and angular velocity).
  - A ship in an unloaded area freezes into a `HoldingSubLevel` in `SubLevelHoldingChunkMap`. **It does not
    move while nobody is near.**
  - There is no global ship map and no DH support in Sable. A docked ship's blocks live in Sable's plot, not
    at the dock, so the far terrain at a distant base never shows its ship (U, reasoned from the plot model).
- **What the sim knows:** for player ships, nothing yet. VOYAGE (I17, C30, G-B7) designs the sim moving a
  player's ship between bases while unloaded; it is design only. For NPC ships, the ships engine has full
  timetables (`Ships::position`, `crates/rtsim-ships/src/engine.rs:3362`, a closed form), and `SHIP_TIMETABLE`
  (0x8601) already carries waypoints and depart, cruise, approach and arrive ticks. But NPC ships have no body
  in the world (P8), so there is nothing to hand over to when you get close.
- **As boxes:** the server sends the ship's silhouette once: its blocks cut into 2-block cells, outer cells
  only, runs merged, each with its block's map colour (E: 100 to 300 boxes for a mid-size ship, about 2 KB).
  Then pose at 1 Hz. The client re-cuts the silhouette when the ship's yaw moves by more than 15 degrees (E);
  pitch and roll are ignored at that distance.
- **Terrain repo:** no. Stamping a docked ship into the far terrain would outlive the ship leaving.
- **Verdict:**
  - Real ships that are loaded (someone is near them), and frozen ships at their held pose: **doable now**,
    with a Sable plugin on the server. Sable accessor names are U.
  - Ships moving between bases while nobody is near: **doable with sim work**, and that work is VOYAGE itself.
  - NPC fleets: **not until they have bodies.** Drawing ships you can fly toward and never reach would break
    the rule that what you see is real.

### 4.4 Windmills and factories

- **What the far view shows today:** a windmill without sails. Assembled sails are a contraption entity, not
  blocks, so DH's terrain never sees them; it sees the bearing and the tower.
- **What the sim knows:** the glass box has `GraphNode` positions and model codes (MILLSTONE 10 to BOILER 51,
  BLACK_BOX 80) and a `Solution { dom_level, saturated, duty }` per sweep every 100 ticks. It has **no
  windmill, water wheel or steam engine model code**, the Create plugin treats bearings only as kinetic
  sources, and the solution is not sent to Java or saved.
- **What Create gives while loaded (V, `javap`):** `WindmillBearingBlockEntity` (`getGeneratedSpeed()`,
  `lastGeneratedSpeed`), `MechanicalBearingBlockEntity` (`getMovedContraption()`, `getInterpolatedAngle(float)`,
  `getAngularSpeed()`, `isRunning()`), `BearingContraption` (`getSailBlocks()`, `getFacing()`), and `Contraption.getBlocks()` (local position to
  `StructureBlockInfo`). The bearing's contraption entity is tracked up to 20 chunks away (`AllEntityTypes`).
- **What is missing:** a capture. The Create plugin reads the sail shape, axis and speed while the windmill is
  loaded and keeps them in a Java-side cache file. Under UR-1 that cache is disposable: delete it and the next
  load rebuilds it. The sim never holds the only copy.
- **As boxes:** a flipbook. The client rotates the sail blocks about the bearing axis in steps and cuts each
  step into boxes. Four-armed sails repeat every 90 degrees, so 4 poses at 22.5 degrees cover a full turn (E:
  about 100 boxes a pose for 12-block arms). One group per pose; one active at a time; the controller picks the
  pose from game time and speed. Other shapes get more poses or coarser motion.
- **Spin at what speed?** Two honest options:
  - **The last real speed.** Create windmills turn at a constant speed unless overstressed, so this tells the
    player nothing new. No sim work.
  - **What the sim says.** Stop when the sim says the domain stalled or a raid broke its machines. Needs the
    domain state sent to Java, and it shows factory state at a distance, which I18 forbids as written
    (decision 2).
- **Other "whirring":** large water wheels are blocks with a spinning model, so the far terrain shows the
  block, not the spin; at DH distances that is fine. Lamps and lit windows at night: DH stores block light in
  its terrain data, so they should already glow (U, not seen). Chimney smoke from running boilers would be
  another sim-state signal under decision 2.
- **Terrain repo:** no.
- **Verdict: doable with plugin work.** The last-real-speed version needs no engine work.

### 4.5 Smoke and fire under attack

- **What the sim knows (V, `crates/rtsim-conflict/src/engine.rs`):**
  - War parties are polylines with a time per waypoint (`party_pos`, line 2400): Envoy, Scouts, Raid,
    Invasion, Siege, Plunder.
  - Events with place and time: `ambush` (2635), `arrive` (2725), `raid_district` (2923, a player district's
    machines and stores), `raid_node` (3031, a village's or a haven's market).
  - Fights happen only when watched; unwatched ones `resolve` (2854) at once by Lanchester's law at the
    arrival tick.
  - Damage to a district is a `Damage { survivors, stolen, destroyed, machine_lost }`
    (`crates/rtsim-industry/src/threat.rs:455`), applied lazily when the chunk loads (`IND_CHUNK_RECONCILE`).
    The reconcile rule says "Never place fire" (`docs/industry-boundary.md:237`). `District.attacks` keeps the
    last 16. The archive keeps 32-byte rows for raids, ambushes and dragon flights.
  - A village raid takes goods and people from its market. It changes no blocks.
- **What is missing:** an engine-to-Java war event record: kind, place, radius, start tick, intensity (from
  `destroyed` and `machine_lost`, or the village's losses). The march before the fight is section 4.6.
- **As boxes:**
  - **Fire:** 10 to 30 small orange and yellow boxes on the ground over the district, at block light 15,
    unshaded, SSAO off, in 2 or 3 groups that swap and dim (block light 15 down to 12) at random. Ground height
    from a terrain repo column read (section 3.7).
  - **Smoke:** dark grey puffs in two groups whose origins rise every frame and wrap, wider as they rise, so the
    column climbs without rebuilding boxes. Opaque first; translucent once tested (3.2).
  - **How long:** the fight resolves in an instant, but the effect lasts: fire for a while scaled by
    intensity, smoke longer and thinning. All clients see the same thing, because times come from the server.
  - **Before the fight:** the band's march, section 4.6.
- **Terrain repo:** no scorch marks (section 3.7). Real damage reaches a player's far view after the chunk
  loads and the reconcile runs: that player's DH re-reads the chunk. Whether DH then pushes the change to
  other players' far views is U.
- **Mismatch to know about:** a raided village smokes, then looks untouched when you ride in, because the sim
  changes no blocks there and the reconcile never places fire. That is decision 4.
- **Verdict: doable with sim work** (the event record).

### 4.6 War bands on the march

Ben decided (MOWZIE, 2026-10-21) that a Mowzie grove's Invasion band, with the Umvuthi when it leads,
visibly marches from its grove toward the player: "Possibly being able to see them coming via DH." The
same case covers every faction's war band in the conflict engine and in the lifelike conflict design
(branch `plan/conflict-life`: goblin, bandit and raider camps and the two Mowzie cultures, marching on
players, villages and each other).

- **What the sim knows (V, `crates/rtsim-conflict/src/engine.rs` on main):**
  - Every party out of its camp (envoy, scouts, raid, invasion, siege, plunder) has its whole march at
    departure: `Party.route` (waypoints on the road graph) and `Party.times` (the tick at each), with
    `party_pos(p, t)` (line 2400) giving its position at any tick in closed form. Each segment is booked as
    a straight leg in the shared leg index (`book_leg`, line 2338), the same `Path` dragons use. Nothing is
    stepped; the route changes only on events (seen, lurking, recall, a rout, arrival).
  - It also knows the band's make-up: `units` (alive per unit, each with a body kind from the culture),
    `kind`, `faction`, and `chief` (the war chief rides). MOWZIE's rework adds which party kinds a chief
    rides with (`chief_rides`: the Umvuthi only on an Invasion, and only when its grove is out).
  - March speed is a culture value: 0.02 blocks a tick for goblins and the two Mowzie cultures (goblin's
    PLACEHOLDER, MOWZIE §4.1), 0.03 for bandits and raiders (`data/cultures/*.json`). At 0.02 a band covers
    480 blocks in a Minecraft day.
  - Java learns nothing about a party until a player meets it (BAND_MATERIALIZE, `docs/boundary.md`
    0x8803).
- **What is missing:** one engine-to-Java record per march: party id, culture, kind, frame, the chief flag,
  unit counts by body kind, and the route as waypoints with ticks. Sent at departure and on each change, with
  a GONE when the party ends. About 48 + 8 per unit kind + 16 per waypoint bytes, so about 400 bytes for a
  20-waypoint march (E). It is the same "far mover" family as the dragon leg record (4.2), decoded in core.
  This is the sim work; the positions already exist.
- **As boxes:**
  - **The column.** Warriors two abreast, about 1.5 blocks between rows, laid along the current segment.
    One box per row (2 warriors), so a 26-warrior invasion is about 13 boxes and a strip about 20 blocks
    long (E). Each row sits at its own point on the route, so a diagonal or a bend needs no rotation: the
    column simply follows the road. Rows go in two groups that bob half a block out of step, so the column
    reads as walking even though it barely moves.
  - **Colours by culture and body:** Umvuthana in their mask colours, raptor packs as a lower, faster-looking
    cluster beside the column, goblins green, bandits brown, raiders grey; the Frostmaw, a culture of one, a
    single large pale figure (E: palette to be picked against the far terrain).
  - **The leader.** When `chief` is set, one taller group in the column's middle: for the Umvuthi a box of
    its 1.5 by 3.2 size (MOWZIE §4.5) topped with sun-mask gold. It does not glow: under DH's built-in
    shader only light makes things bright, and inventing a glow would show what eyes would not.
  - **Size against distance (E):** at a 70 degree field of view on a 1080-pixel screen one pixel covers
    about 0.0013 times the distance in blocks. So a 3-wide column is a few pixels at 800 blocks and a speck
    at 1,500; individual warriors are lost beyond about 300, which is inside vanilla range anyway. Seeing
    a band coming therefore means seeing it within about a kilometre, which at 0.02 blocks a tick is about
    two Minecraft days of warning. A dust haze over the column (pale boxes in dry biomes such as the
    Umvuthana's savanna) would carry farther; it is optional and what eyes would see.
  - **Night:** the column is as dark as the land under it. No torches are invented.
  - **Per band:** 3 groups (two row groups and the leader), 3 draws, about 15 to 20 boxes (E).
- **The hand-over (V, `seen`, line 3602):** a march leg meeting a player's leg in the shared index books
  `Ev::Seen` at the meeting tick. `seen` embodies the band at `party_pos`, on the player's side of a seam
  (TP11), up to `spawn_quota` 16 members (`tunables.rs:94`), then steers it waypoint by waypoint with
  BAND_STEER at march speed, and `book_unseen` takes it back where its walk leaves the area.
  - The real band appears where the proxy is, so there is no jump in place. The proxy hides when the feed
    says the party is embodied and the client has its puppets, or its chunk is loaded on the client.
  - **Seam: fewer bodies than boxes.** A band bigger than the quota shrinks at the meeting: a 26-warrior
    column becomes 16 real warriors. The rest stay in the sim's count.
  - **Seam: the Umvuthi.** MOWZIE never sends the Umvuthi to a band met on the march; it appears only at a
    base under attack, on a war seat 168 blocks out, beyond the 160 blocks a client tracks it from. So a
    gold leader drawn in a far column would vanish when the column is met. That is decision 5.
  - **Other players far from an embodied band** see it drawn from its puppets' positions, sampled at 1 Hz
    on the server, as for embodied dragons.
  - **At the target:** a fight where a player watches is real. An unwatched one resolves at once and becomes a
    war event (4.5) at the target, whether that is a base, a village or another faction's camp.
- **Not drawn:** a band mustering at its camp and a band walking home. Neither is ever embodied when met
  (`seen` skips the Mustering and Home phases), so a player who rode up to one would find nothing.
- **Terrain repo:** no.
- **Verdict: doable with sim work** (the route record). MOWZIE's rework already gives each march its
  positions over time; the far view needs only that record and a decision on the Umvuthi seam.

## 5. Real versus simulated: who draws what, and the hand-over

Three layers draw. DH's terrain draws what stands still (land, bases, track, docks). Vanilla draws everything
in render distance. **Proxies draw only what moves or burns, and only where vanilla does not.**

| Thing | Real source near you | Proxy source far away | Proxy hides when |
|---|---|---|---|
| Train carriage | Create carriage entity | Create's server position (phase 1: Create's map sync) | the client has that carriage's entity, or its point is in a chunk the client has loaded |
| Dragon, flying | Ice and Fire entity, once embodied within 160 blocks of a player | engine leg | the client has the entity (network id sent with the proxy) |
| Dragon, embodied near another player | the same entity | Ice and Fire plugin, 1 Hz | the client has the entity |
| War band on the march | its puppets, once `seen` within 160 blocks of a player | the engine's route record; puppets sampled at 1 Hz once embodied | the party is embodied and the client has its puppets, or its chunk is loaded on the client |
| Player airship | Sable sub-level within 320 blocks | Sable plugin, silhouette plus 1 Hz pose | the client's Sable container has the sub-level |
| Windmill sails | the contraption entity | the cached silhouette and speed | the bearing's chunk is loaded on the client |
| Fire and smoke | nothing (no fire is placed) | war event | it is in a chunk the client has loaded, or its end tick passed |

- **The rule:** entity presence first, loaded chunks second. A slow sweep (about once a second, like DH's
  beacon culler) applies the second test; the first is checked per frame by the controller, which is cheap.
- **Dragon seam:** the engine spawns the real dragon on its leg at the edge plus 32 blocks, so the proxy should
  be where the entity appears. A small jump is possible if the spawn point differs from the leg (U).
- **Train seam:** Create registers `carriage_contraption` with a tracking range of 15 chunks (V,
  `AllEntityTypes`), cut down by the server's view distance as for any entity. The proxy stays until the real
  carriage appears.

## 6. Server to client

### 6.1 The shape (D4)

- **Core (`mic_rtsim`) owns the feed and knows no pack mod.** It registers two payloads, keeps subscribers,
  filters by distance, enforces a budget, and offers a small SPI that plugins fill: "far sources" on the
  server (a callback that yields objects near a point) and a client-side registry of far objects that a
  renderer reads. Engine records (dragon legs, war events) are decoded in core when they are generic, so a
  "mover of kind dragon, style 2" carries no Ice and Fire class.
- **Each plugin stays built around one mod:**
  - `mic_rtsim_create`: trains and windmills (server sources; in phase 1, a client source that reads
    `TrainMapSyncClient`).
  - `mic_rtsim_iceandfire`: embodied dragons.
  - A Sable plugin: ships.
  - **`mic_rtsim_dh` (new): the renderer.** It knows DH and nothing else. It reads the core client registry
    and draws.
- Today no module registers a payload or has client code (V: no `CustomPacketPayload`, `PayloadRegistrar` or
  `Dist.CLIENT` in `neoforge/`). This adds the first.

### 6.2 mods.toml and sides

- Core registers the payloads with `PayloadRegistrar(...).optional()`, so clients without them still join.
  The client-bound handler only hands data to the core registry.
- `mic_rtsim_dh`: `@Mod(dist = Dist.CLIENT)`; dependencies on `mic_rtsim` and on `distanthorizons`
  (required, `side = "CLIENT"`, range from 3.3.1). In packwiz it is `side = "client"`, so servers never get it.
- Existing plugins' dependency lines stay `BOTH`; their new client sources sit behind `Dist.CLIENT` entry
  points.

### 6.3 Payloads, recipients, rate, size (E)

- **Client to server, `far_sub`:** version, radius in blocks (DH `chunkRenderDistance()` times 16), detail
  tier, group budget, kinds wanted. Sent on joining a level and when the DH distance changes; radius 0
  unsubscribes. A client sends nothing if DH generic rendering is off or it runs on macOS.
- **Server to client, `far`:** a batch of records:
  - `LEG` (id, kind, style, start and end point, start and end tick): dragons, later parties and ships. About
    60 bytes, sent once per leg.
  - `ROUTE` (id, culture, kind, chief flag, unit counts, waypoints with ticks): war bands (4.6). About 400
    bytes, sent once per march and again on a change.
  - `SAMPLE` (id, tick, position, heading; for trains, two points per carriage): about 30 bytes per object
    per second.
  - `SHAPE` (id, merged boxes in local cells with palette colours): once per object per subscriber. About 2
    KB for a ship, 1 KB for a windmill.
  - `SPIN` (id, axis, speed, phase tick), `EVENT` (id, kind, place, radius, start and end tick, intensity),
    `REAL` (id, entity network id, for hand-over), `GONE` (id).
- **Recipients:** subscribers only, with `PacketDistributor.sendToPlayer`, and only objects within their
  radius. Nobody else pays anything.
- **Rate and size:** legs and events trickle. Samples at 1 Hz: 50 four-carriage trains all in range is about
  50 × (16 + 4 × 24) bytes, about 5.6 KB/s per subscriber, against Create's 45 KB/s for the same trains. Five
  embodied dragons and two ships add well under 1 KB/s.
- **Where the server work runs:** Create trains and Sable sub-levels must be read on the server thread. A
  1 Hz sweep over 50 trains with a distance test per subscriber is small (E); it never touches the engine
  step.

### 6.4 Client interpolation

- **Legs:** exact. The controller evaluates the same lerp as `Path::at` at
  `level.getGameTime() + partialTicks`, converted to sim ticks with the offset the feed sends. Game time
  reaches the client every 20 ticks from vanilla and is stepped locally in between.
- **Samples:** drawn 1 second behind the newest sample, lerped between the two around that time, like
  vanilla entity interpolation; held if samples stop.
- **One controller per level** runs in `DhApiBeforeGenericRenderSetupEvent` once per frame: moves origins,
  swaps flipbook poses, flickers fire, applies the entity hand-over. Groups carry no lambdas of their own.

### 6.5 Multiplayer, dedicated servers, single player

- **Dedicated server:** needs `mic_rtsim` and its plugins, which it has. DH on the server is not needed for
  proxies; it is what generates and sends far terrain, which proxies need underneath them (section 10).
- **Clients:** need DH and `mic_rtsim_dh`. Clients without them never subscribe and cost nothing.
- **Single player:** the same code over the in-memory connection. The DH level there is a
  `DhClientServerLevel`, which has a renderer (V).

## 7. Performance on a weak PC (D1)

- **Engine:** two new record kinds, both event driven: one per dragon leg, one per war event. Nothing per
  tick. No change to the step.
- **Server Java:** one 1 Hz sweep (trains, embodied dragons, loaded ships) and a distance filter per
  subscriber; windmill and ship silhouettes cut once and cached.
- **Client, per frame:** one controller pass over live proxies, plus DH's own cost of one map walk and one
  pre-render check per registered group and one instanced draw per active group. No rotation maths, no
  per-frame buffer uploads.
- **Box counts (E):** carriage 1 to 10, dragon about 20 across 4 groups, windmill about 100 per pose, ship
  100 to 300, a burning district 30 to 60, a marching band 15 to 20 across 3 groups. DH's own debug code builds a 10,000-box group, so tens of
  thousands of boxes are within what DH expects; what costs on a weak PC is draws, not boxes.
- **Tiers (a client option, E, to be measured):**

| Tier | Active groups | Registered groups | Boxes | Flipbooks |
|---|---|---|---|---|
| Off | 0 (no subscription) | 0 | 0 | |
| Low | 32 | 96 | 4,000 | wings 2 poses, sails 2 poses, no smoke rise |
| Medium (default) | 96 | 288 | 12,000 | wings 3, sails 4 |
| High | 256 | 768 | 30,000 | wings 3, sails 6, translucent smoke if it works |

  The server gets the budget in `far_sub` and sends the nearest objects first; the client drops the
  farthest groups when over budget.
- **How to measure (phase 1):** DH pushes a profiler section "Custom Objects" in `LodRenderer`, so the F3
  profiler and spark both show the frame cost directly. Bandwidth from the packet counts.

## 8. Shaders and DH settings

- **The pack has no Iris or Oculus** (V: `pack/mods`), so the built-in shader applies: material is ignored,
  light is the lightmap, colour is what we pick.
- **With Iris and a DH-aware shader pack** (read from Iris source, `iris/compat/dh/`, branch
  `multiloader-new`): Iris binds the generic render events and swaps in the pack's `dh_generic` program, or
  its `dh_terrain` program when the pack has no `dh_generic`. Complementary Reimagined and Photon ship only
  `dh_terrain` and `dh_water`, so boxes would be shaded like far terrain. Iris cancels only groups named
  "Clouds", and only when the pack asks; our groups are never filtered. Under a shader pack the material
  byte matters (ILLUMINATED for fire, so it can glow), and whether the pack honours our per-group light
  and alpha is U. Iris 1.11.2 and older were marked incompatible with DH on 2026-09-28.
- **Generic rendering off** (`IDhApiGenericRenderingConfig.renderingEnabled()` false): nothing draws. The
  plugin does not subscribe and writes one line in the log and its config screen. It does not flip the
  setting for the player.
- **Overdraw prevention** moves DH's terrain near clip, not the proxies' (3.4). The plugin's own hide test
  (section 5) covers it.
- **Rotation through a custom shader was considered and rejected:** overriding
  `IDhApiGenericObjectShaderProgram` replaces DH's program for beacons and clouds too and fights Iris for
  the same slot.

## 9. Prior art, and which DH to target

### 9.1 Who else draws moving things in DH

- **LOD Entity Rendering** (Steveplays; https://modrinth.com/mod/lodentityrendering,
  https://github.com/steves-underwater-paradise/lodentityrendering; LGPL-3.0; MC 1.20.1 only, last update
  2024-12-31, built on API 4.0.0). The only confirmed user of the box API. It draws every server entity as one
  `createForSingleBox` box in the average colour of its texture, and moves it with `setOriginBlockPos`. The
  server sends every entity's position to every player every tick, with no distance limit and no throttle,
  and it finds the register through `worldProxy.getSinglePlayerLevel()`, which the Javadoc says is null in
  multiplayer. **Lessons:** moving boxes work; filter and throttle on the server; get the register from the
  render parameter or the level load event, never from the single-player level.
- **SeeU** (https://modrinth.com/mod/seeu, https://github.com/cat4blep/SeeU; MC 1.21 and newer; own licence)
  and **SeeU Extra** (https://modrinth.com/mod/seeu-extra; MIT) show far players and chosen entities as
  client-side stand-ins with their own server sender. DH is only the terrain under them; use of the box API
  is U.
- **Separate Sable Render Distance, SSRD** (https://modrinth.com/mod/ssrd; 1.21.1 NeoForge; All Rights
  Reserved). The only mod found that shows Sable ships far out. It does not draw boxes: it mixes into Sable,
  Sodium and entity tracking to stretch Sable's own range, and patches DH's depth. DH 3.3.2's switch to
  reverse-Z depth broke SSRD 1.8.6 and older. **Lesson:** stretching real tracking sends full ship data to
  far players; a silhouette is far cheaper (section 4.3). Not in the pack.
- **Valkyrien Skies issue 1300** (https://github.com/ValkyrienSkies/Valkyrien-Skies-2/issues/1300, open, no
  replies) proposes ship bounding boxes as a `RenderableBoxGroup`, the idea of section 4.3. It also notes that
  near the edge of vanilla range, LODs can cover vanilla objects when no shader pack is on. That is a seam to
  watch in the hand-over (section 5, U).
- **Not found:** any mod drawing Create trains, windmills or dragons through DH. Nothing to reuse beyond the
  API itself.
- **The API example:** https://gitlab.com/distant-horizons-team/distant-horizons-api-example (MIT; API
  4.0.0, dated). `GenericRendering.registerForLevel` runs from a `DhApiLevelLoadEvent` handler; it builds
  boxes, sets light, adds groups, and animates through `setPreRenderFunc` with `setOriginBlockPos`. It
  matches section 3. Which threads may call `add` and `remove` is not documented (U); doing it on the render
  thread inside the controller is safe by construction.

### 9.2 API changes since 3.3.1, and the version to target

| API | Shipped in | Change that matters here |
|---|---|---|
| 3.0.0 | DH 2.2.0 (2024-08) | the box API, the generic-rendering config and events |
| 4.0.0 | 2025-03 | level load and unload events |
| 4.1.0 | 2025-10 | `remove(obj)` and `remove(index)` on groups |
| 5.1.0 | 2025-12 | `DhApiRenderParam.clientLevelWrapper` |
| 7.1.0 | DH 3.3.0 and 3.3.1 (2026-09) | texture config; **this pack** |
| 7.2.0 | DH 3.3.2 and 3.3.3 (2026-09) | depth direction and range getters (reverse-Z depth) |
| 7.3.0 | unreleased main (3.3.5-dev) | hot-swappable render definitions, `getRenderBoxList()` |

Sources: https://api.modrinth.com/v2/project/distanthorizonsapi/version and the Javadoc at
https://distant-horizons-team.gitlab.io/distant-horizons/.

- **No version adds rotation, textures or per-box transforms**, including unreleased 7.3.0. Flipbooks stay the
  answer.
- **Nothing we need is newer than 5.1.0.** The plugin can compile against the pack's 3.3.1 jar.
- **For 1.21.1, the only DH after 3.3.1 is 3.3.3** (2026-09-29; 3.3.4 is MC 26.3 only). It fixes the Chunky
  trap (section 10). It also brings 3.3.2's reverse-Z depth, which does not touch the box API (the shader
  gets DH's matrices), and it bumps DH's config version, which resets DH's config file. The pack ships no DH
  config, so nothing is lost.
- **Recommendation: move the pack to DH 3.3.3** and declare `[3.3.1,)` in `mic_rtsim_dh`. Whether 3.3.3
  breaks any other mod in the pack is U until it is tried.

## 10. The Chunky trap

- **What happens (V, DH source at tags 3.3.0 to 3.3.2, `common/.../AbstractModInitializer.java`,
  `logIncompatibilityWarnings`, called only from `onInitializeClient()`):** if
  `Class.forName("org.popcraft.chunky.api.ChunkyAPI")` succeeds, DH sets `generatorPlan` to `DISABLED` and
  `disableUnchangedChunkCheck` to true on its API layer, and logs "Partially Incompatible Distant Horizons mod
  detected: [Chunky]". The config file and screen still show the old plan. The client then asks for no far
  terrain, in single player or from a server. A dedicated server with Chunky is untouched. LODs from chunks
  the client itself loads probably still build (U).
- **The fix:** DH 3.3.3, changelog "Only disable DH world gen when Chunky world gen is active"
  (`AbstractChunkyAccessor` listens to Chunky's progress event and re-enables after 2 s of silence). Sources:
  https://gitlab.com/distant-horizons-team/distant-horizons/-/blob/3.3.2/common/src/main/java/com/seibel/distanthorizons/common/AbstractModInitializer.java,
  https://gitlab.com/distant-horizons-team/distant-horizons-core/-/commit/ff7d366f,
  https://gitlab.com/distant-horizons-team/distant-horizons/-/issues/1072.
- **This pack:** DH 3.3.1 and Chunky 1.4.23, both `side = "both"` (V: `pack/mods/distanthorizons.pw.toml`,
  `pack/mods/chunky.pw.toml`). So every client has Chunky, and far terrain is probably missing for everyone
  except where they have walked (U for this exact pack; the mechanism is V).
- **Why it matters here:** a far view of trains and dragons over a void is worse than no far view.
- **Fixes, either works:** DH 3.3.3 (section 9.2), or Chunky `side = "server"` (it is a pregeneration tool;
  single-player worlds would lose `/chunky`). See decision 3.

## 11. Reuse versus custom

| Piece | Reuse (what, where) | Custom (what we write) |
|---|---|---|
| Far terrain | DH's LOD: bases, track, docks, lairs, villages as they are | nothing |
| Box drawing | DH `IDhApiCustomRenderObjectFactory`, `IDhApiRenderableBoxGroup`, lightmap, fog | box sets for each kind; one controller |
| Train positions | Create `TrainMapSync` (phase 1), `Create.RAILWAYS.trains` (phase 2) | the carriage staircase; the 1 Hz sampler |
| Train hand-over | Create's own carriage entities near the player | the hide test |
| Dragon flight | engine `FlightLeg` and `Path::at`; Ice and Fire entities near players | the leg record; the dragon box model and wing poses |
| Airships | Sable sub-levels, its 320-block tracking and sync near the player | the silhouette cutter; pose sampler; VOYAGE for unloaded travel |
| Windmills | Create's windmill bearing, contraption and speed while loaded | the capture cache; the pose cutter |
| War | conflict engine events, `Damage`, reconcile on chunk load | the event record; fire and smoke boxes |
| War bands | the engine's routes and `party_pos`, its meeting and embodiment (`seen`), Mowzie and other pack mobs as the real bodies | the route record; the column and leader boxes |
| Ground height | DH terrain repo column reads | nothing |
| Networking | NeoForge optional payloads; vanilla game-time sync | the two payloads and their filter |
| Settings | DH's own render distance and generic-rendering switch | a detail tier option |

Nothing here adds an entity, block or item (D3). The only new jar is `mic_rtsim_dh`, client side.

## 12. A player's view

You stand on your base's roof at dusk with vanilla render distance 12 and DH at 256 chunks.

- **East, 900 blocks off,** your second base on the ridge is a grey mass in the far terrain, as today. New:
  the big windmill's four sails turn above it in small jumps you only notice if you stare.
- **Your station below** sends a four-carriage train north. You watch the real train leave, and as it passes
  your render distance it does not vanish: four dark blocks keep gliding along the track the far terrain
  shows, past the lake, until the hills hide them.
- **Over the mountains,** a red shape with beating wings crosses the sky at cruise height and drops onto the
  peak where you know a lair is. It folds up there. At night it is a dark lump against the stars.
- **South, toward the village,** a column of dark puffs starts to rise, with orange specks glowing under it.
  A raid has just resolved there. The specks flicker for a while and go out; the smoke thins later. When you
  ride down, the market is poorer, but no house is burned (unless decision 4 says
  otherwise).
- **South-west, at dawn,** a dark strip lies on the road out of the Umvuthana grove 800 blocks away: a
  column two abreast, a taller figure in sun-mask gold in its middle. It hardly seems to move, but by
  evening it has crossed the river. When it comes within 160 blocks of you, the real masked warriors are
  there on the road where the column was.
- **West, 1,500 blocks away,** your friend's airship lifts off their base: a blocky silhouette that slides
  across the sky and turns in steps. When it comes within 320 blocks the real ship takes its place, ideally
  without a visible jump.
- **Later, when your own district is raided while you are away,** you see the smoke from your roof at the
  other base. When you go there, the broken machines are already missing, because the chunk's reconcile ran
  when you arrived.

## 13. Phased plan

Each phase ends with something you can look at in game.

0. **Far terrain first.** Apply decision 3 (DH 3.3.3, or Chunky server-only) and check that a client sees
   far terrain in multiplayer. Without it nothing below looks right.
1. **Trains, first visible slice.** Builds:
   - in core, the client-side far-object registry (no DH, no Create);
   - in the Create plugin, a client source that calls `TrainMapSyncClient.requestData()` each tick and copies
     `currentData` into the registry;
   - `mic_rtsim_dh`: registration on `DhApiLevelLoadEvent`, one group per carriage with the staircase cut,
     the per-frame controller, the hide test, the tier option, the macOS and generic-rendering checks.

   Then measure the "Custom Objects" frame cost and the packet bandwidth with 20 and 50 trains.
2. **Our own feed, and dragons.** Builds: the two payloads and subscriber filter in core; a 1 Hz train source
   in the Create plugin (Create's map requests turned off); the engine's dragon leg record and its decoder;
   the Ice and Fire source for embodied dragons; the dragon box model with wing poses; entity hand-over.
   Needs decision 1.
3. **Windmills.** Builds: the capture in the Create plugin (sail shape, axis, speed) with its disposable
   cache; the pose cutter and flipbook on the client. Spins at the last real speed unless decision 2 says
   otherwise.
4. **War.** Builds: the engine's war event record from `raid_district`, `raid_node` and `ambush`; fire and
   smoke on the client with ground height from the terrain repo. Then war bands on the march (4.6): the
   engine's route record, the column and leader boxes, the hide on embodiment, and decision 5's answer for
   the Umvuthi.
5. **Airships.** Builds: a Sable plugin source for loaded and held sub-levels (silhouette and pose); the yaw
   re-cut on the client. Ships moving between bases while unloaded arrive with VOYAGE, not here.
6. **Later, when they exist in the world:** NPC fleets (after P8 gives them bodies) and far herds (PLAN
   §4.10 already lists them).

## 14. Verified and unverified, side by side

| Verified (V) | Not verified (U) |
|---|---|
| **DH API:** factory, group and box methods; one-colon name rule; defaults; register only on client levels (`DhServerLevel` returns null); groups closed with the level | Exact frame cost per group and per box on a weak PC |
| **DH shader:** no rotation; material unused; light from the lightmap per group; alpha kept, blending on | Whether translucent boxes look right (depth writes, draw order) |
| **DH renderer:** pre-render runs before the active check; no culling; near plane at most 7.5 blocks; skipped on macOS; config switch | That vanilla geometry hides a near proxy (reasoned from draw order and the beacon culler) |
| **Create:** `TrainMapSync` packet contents, rates, requester cache, client map, no config gate; windmill bearing and contraption accessors; carriage tracking 15 chunks, bearing contraption 20 | That carriage proxies meet the real carriage without a visible jump |
| **Sable:** 320-block tracking default, sync packets, `HoldingSubLevel` freeze | Accessors for a held ship's pose and bounds; whether plot chunks appear in DH terrain |
| **Sim:** dragon legs and constants; conflict events and damage; no fire placed; ship timetables; glass box solution not exported | Ice and Fire draw size per stage; whether train raid records carry a position |
| **Conflict:** party route and times, `party_pos`, legs in the shared index, `seen` embodies at the party's place up to quota 16, march speeds by culture | How far a column is really visible (pixel arithmetic only); how MOWZIE's rework changes the march |
| **Iris source:** generic program swap, `dh_terrain` fallback, only "Clouds" cancelled | How a given shader pack lights and blends our boxes |
| **Pack:** DH 3.3.1 and Chunky both `side = "both"`; no Iris, no SSRD; no Sable config. **DH source:** the Chunky disable at 3.3.0 to 3.3.2 and its 3.3.3 fix | That this pack's clients really have no far terrain today |

## 15. For Ben

### 15.1 Decisions that are yours

1. **Stream sim state to clients that opt in.** PLAN §4.10 says "No sim state is streamed". Recommended: yes,
   only to clients running the new `mic_rtsim_dh` with DH's generic rendering on, filtered to their DH radius,
   with a detail tier they choose. It adds the first network payloads and the first client-side code to
   mic_rtsim, and one client-only jar to the pack. Alternative: no streaming, which still allows phase 1
   (Create's own train sync) but rules out dragons, war smoke and anything else the sim owns. *Architecture
   and a public interface, so yours.*
2. **How I18 reads for a far view.** Recommended: amend it to "the far view shows what your eyes would see
   from where you stand: movement, fire, smoke, sails turning or stopped; never numbers, names or anything
   you could not see from there." Under that reading windmills may stop when the sim says their machines
   were destroyed. Alternative: windmills always spin at their last real speed, and only war events (which
   you asked for by name) show sim state. *A design rule you wrote, so yours.*
3. **Give clients far terrain (the Chunky trap).** Recommended: move the pack to DH 3.3.3, the only newer
   1.21.1 build, which fixes the trap at its source and keeps `/chunky` in single player (section 9.2).
   Alternative: keep DH 3.3.1 and make Chunky `side = "server"`. *Changes what players download, so yours;
   easy to undo.*
4. **Do raids leave scars?** Recommended: no. Smoke and fire are shown while and after a raid resolves; the
   only lasting marks stay what the reconcile does now (machines broken, stores taken), and villages show
   none. Alternative: the reconcile also swaps some surface blocks of a raided district or village for burnt
   ones (no fire placed). That changes world blocks the player built. *Yours.*
5. **The Umvuthi when its band is met on the march.** Recommended: MOWZIE's rework seats the Umvuthi out of
   sight behind a band met on the march, as it already does at a base (a war seat beyond the 160 blocks a
   client tracks it from), and the far view draws the gold leader at that seat until the client sees the
   body. Alternative: the far view draws the leader only until the band is met, and it vanishes then.
   *Changes the MOWZIE design, so yours.*

### 15.2 Choices I made (each can be undone)

- **No spike.** The bytecode answered how to register, what material does, rotation and per-frame cost.
- **Architecture:** feed and client registry in core with no pack-mod dependency; each source in the plugin
  of its own mod; the renderer as its own client-only plugin. Keeps D4.
- **First slice reuses Create's `TrainMapSync`**, then phase 2 replaces it with our 1 Hz filtered feed.
- **Wire format sketched, not fixed** (section 6.3). No compatibility work before release.
- **No terrain repo writes;** column reads for ground height only.
- **Rotation by flipbooks and heading re-cuts,** not by overriding DH's shader.
- **Hand-over:** entity presence first, loaded chunks second, with a slow sweep like DH's beacons.
- **One controller per level** in `DhApiBeforeGenericRenderSetupEvent`; groups carry no lambdas.
- **Compile against the pack's DH 3.3.1 jar** (API 7.1.0; nothing we use is newer than 5.1.0); declare
  3.3.1 and newer.
- **Get the register from `DhApiRenderParam.clientLevelWrapper`** in the controller, not from
  `getSinglePlayerLevel()`, which LOD Entity Rendering uses and which is null in multiplayer.
- **Tier numbers** in section 7 are estimates to be replaced by phase 1 measurements.
- **NPC ships are not drawn** until P8 gives them bodies.
- **Windmills spin at the last real speed** until decision 2.
- **Opaque smoke first;** translucent only after it is seen to work.
- **War bands:** a column two abreast, one box per row, bobbing in two groups; colours by culture; no
  invented torches or glow; mustering and homebound bands not drawn, because neither is ever embodied.
- **Macs and generic rendering off:** the client does not subscribe; one log line, no setting changed.
- Wrote this note only. No code, pack or DECISIONS.md change.
