# Choices: the far view, phase 1 (DHT)

Reversible choices made on 2026-10-22 by the far-view trains agent (branch `feat/dh-trains`), building phase 1
of research/distant-horizons-2026-10-21.md §13 under DECISIONS.md DH-1 (opt-in clients) and DH-2 (show what
your eyes would see), on the pack's Distant Horizons 3.3.3 (DH-3). Each choice here can be undone. What is
measured and what is not is at the end, under "Measured".

Phase 1 adds no server code and no payload: the train source reads Create's own train map sync. mic_rtsim_dh
is not in the pack.

## The far registry (core, client side)

| # | Choice | Why |
|---|---|---|
| DHT1 | **A client-side registry in the core, `mic_rtsim.far.FarObjects`, that knows neither DH nor Create.** A source commits its whole current set under its own name (a key `(source, object, part)`; a source cannot write another's keys); a renderer reads one immutable snapshot (`movers()`, with a `version()` that moves only on change). A renderer says how far it wants movers (`want(consumer, radius)`); a source asks its mod for nothing while `wantedRadius()` is 0. | §6.1's shape (D4): mic_rtsim_create and mic_rtsim_dh never see each other, and phase 2's own feed, dragons or ships are more sources and kinds behind the same API. The demand gate is what keeps a client with DH off, or on a Mac, from asking the server for every train. |
| DHT2 | **A mover is two samples and a clock**: the previous and newest 6-number run (two end points), the registry tick of the newest sample and the interval to move over; `segmentAt(now)` lerps, then holds at the newest sample. The clock (`FarClock`) is client ticks counted while the game is not paused; leaving a world clears every source. | No extrapolation: a train that stops does not overshoot. The registry must not depend on Create's `AnimationTickHolder`, and pausing a single-player game freezes the proxies with the trains. |
| DHT3 | **One mover per carriage (kind CARRIAGE), keyed by train UUID and carriage index.** | What Create sends and what the real carriage entity carries (`CarriageContraptionEntity.trainId`, `carriageIndex`), so the hide test can match them. |

## The Create train source (mic_rtsim_create, client side)

| # | Choice | Why |
|---|---|---|
| DHT4 | **A `Dist.CLIENT` `@Mod` entry for mic_rtsim_create (`client.MicRtsimCreateClient`)**; the plugin's dependency lines stay BOTH. | A server never loads a client class, and one jar keeps working on both sides. |
| DHT5 | **While a renderer wants movers it calls `TrainMapSyncClient.requestData()` every client tick** (Create sends a request on every fifth call; the server answers every 5 ticks), and copies `currentData` into the registry on each new packet and whenever a carriage entity joins or leaves the client. Carriages in another dimension, between dimensions (Create sends null), with a non-finite position, or more than DH's radius plus 64 blocks away are left out. | §13 phase 1 as written. Copying only on a new packet keeps the per-tick cost at a field compare. The 64-block margin stops carriages popping in at the edge of DH's radius between packets. |
| DHT6 | **The proxy runs one packet behind**: it moves from Create's previous sample to its newest over the 5 ticks after the packet came (the packet's age is read on Create's clock and moved onto the registry's), as Create's own map does. | Create's samples are 5 ticks apart and exact; a proxy 0.25 s late moves smoothly, an extrapolated one jerks at every stop and curve. |
| DHT7 | **When nothing wants movers it calls `TrainMapSyncClient.stopRequesting()`, which also clears Create's client map.** | The server forgets a client 20 ticks after its last request. Clearing the map only matters to full-screen map mods (JourneyMap, FTB Chunks, Xaero), which the pack has none of; if one is added, this should stop calling `stopRequesting` while that mod is present. |

## The renderer (mic_rtsim_dh)

| # | Choice | Why |
|---|---|---|
| DHT8 | **A separate client-only mod, `mic_rtsim_dh`** (`@Mod(dist = CLIENT)`; requires mic_rtsim and distanthorizons `[3.3.1,)`, every dependency side CLIENT; no mixins; compiled against the pack's DH 3.3.3 jar, never bundled). | §6.2: a dedicated server never needs DH, and a client without DH never loads it. 3.3.1 is the API the research verified (§3); the pack runs 3.3.3. |
| DHT9 | **One relative-positioned box group per carriage** (`createRelativePositionedGroup`, name `mic_rtsim_dh:carriage`), whose boxes are cut around the carriage's midpoint; each frame only the group's origin moves. | §3.3: an origin is a uniform, so a moving carriage costs no buffer upload. |
| DHT10 | **One controller per frame in `DhApiBeforeGenericRenderSetupEvent`**, with no pre-render lambdas on the groups. DH fires that event twice a frame (its SSAO pass and the other); a frame counter from NeoForge's `RenderFrameEvent.Pre` makes the controller act once. Any exception it throws is caught and logged five times at most. | §3.3 and §7: one pass over the proxies instead of a lambda per group. A failure in our code must never take DH's frame down. |
| DHT11 | **The register is the one of the level being drawn, read from the render parameter (`clientLevelWrapper.getRenderRegister()`) each frame.** A change of register, DH's level load event, the unload of the drawn level and leaving the world release every group and start over; an unload of another level (single player's server level) changes nothing. | §3.1. DH closes a level's groups with the level, so holding groups across levels would leak or draw into a dead register. |
| DHT12 | **The box shape: 3 wide, 3 tall from rail height, 1 block past each end point** (an 8-block run is a 10-block carriage). Dark grey `0x3A3B3F`, DH's METAL material. | §4.1's default size. Not the train's map colour (`CreateClient.RAILWAYS` has it): a map colour is a label in a map screen, not what eyes see from far away (DH-2). |
| DHT13 | **The staircase cut (`StaircaseCutter`)**: a run whose sideways drift is under 0.25 blocks is snapped to its axis; level and snapped, it is one cuboid. Otherwise it is steps of 1 block (2 on LOW) along the major axis, each as wide across as the carriage's footprint needs (`k = 1 - s·|sin 2θ|/w`, `a_x = s·|cos θ| + w·k·|sin θ|`, `a_z = s·|sin θ| + w·k·|cos θ|` for heading θ, step s, width w), so neighbouring steps overlap and leave no gap; a slope climbs step by step. At most 48 boxes a carriage (longer runs get longer steps). | Boxes cannot rotate (§3.2), so a diagonal is a staircase. The footprint formula keeps a diagonal carriage about as wide as a straight one (the unit tests check 3 wide across at 45 degrees, no gaps, and a shallow 20-degree run). |
| DHT14 | **Re-cut only when the run turned more than 10 degrees (undirected, in 3D, so a slope counts) or its length changed by more than 0.5 blocks since the last cut (`HeadingRule`), or its colour changed; at most one re-cut per carriage every 250 ms, and one confirming `triggerBoxChange` 1.5 s after each cut (`RecutPacer`).** | §4.1's rule. The pacing bounds uploads on a carriage swinging through a curve. The confirmation is there because DH's group marks its vertex data clean after an async upload, so a change made while the previous upload is in flight can be lost; a second trigger once the carriage has settled picks it up. |
| DHT15 | **The hide test, per frame: a carriage is not drawn while the client has its real carriage entity, or while its run's midpoint is in a chunk the client has loaded (`ClientChunkCache.hasChunk`).** | §5's rule. Checked every frame (a hash lookup per carriage) instead of §5's 1 Hz sweep, because it is cheap and a sweep would leave a proxy drawn for up to a second over a real carriage. See "For Ben" for the band this leaves empty. |
| DHT16 | **The budget (§7's tiers, `FarDetail`)**: the nearest visible carriages (horizontal distance from the camera) are drawn, up to the tier's active groups and boxes; a hidden or over-budget carriage keeps its group, inactive; when the registered budget is full the group unwanted longest is released; a group nobody has wanted for 10 s is released. LOW 32 / 96 / 4,000 with 2-block steps, MEDIUM 96 / 288 / 12,000, HIGH 256 / 768 / 30,000; OFF asks for nothing. | §7's table, which is still to be checked on a weak PC (see "Measured"). Keeping hidden groups means a carriage leaving vanilla's view costs no new group. |
| DHT17 | **The radius is DH's LOD radius (its chunk render distance times 16).** | Past it DH draws no terrain, so a carriage there would float in the void. |
| DHT18 | **The client option `detail`** (`config/mic_rtsim_dh-client.toml`, `[far_view]`, default MEDIUM, also in the mod list's config screen). | §7: a client option. |
| DHT19 | **The gates, checked every client tick: macOS (`os.name`), tier OFF, DH not ready, DH rendering off, DH generic rendering off, no world.** Each turns the far view off and withdraws the demand; the state is logged once when it changes. It never changes a DH setting. | §3.5 and §8: DH draws no generic objects on a Mac, and a player who switched generic rendering off meant it. |

## Building and the bench

| # | Choice | Why |
|---|---|---|
| DHT20 | **The DH jar comes from Modrinth at build time** (`fetchDistantHorizons`: the pack's DH 3.3.3 URL, checked against the pack's sha512 from `pack/mods/distanthorizons.pw.toml`), or `-PdhJar=<jar>`. | The smoke server's mods folder (`packMods`) still has DH 3.3.0, and a server folder is the wrong place for a client mod anyway. The hash pins it to the pack's file. |
| DHT21 | **`./gradlew :dh:runClient` is a dev client with the core, mic_rtsim_create, Create 6.0.10, DH 3.3.3 and mic_rtsim_dh, on the stub engine** (`-Dmic_rtsim.gametest=true -Dmic_rtsim.engine=stub`), in `dh/run/` (ignored). | The far view needs no engine, and a worktree without a native build can still run it. |
| DHT22 | **The bench is dev code in the jars, inert without `-Dmic_rtsim.farbench=true`** (set by `-PfarBench=<trains>`; `-PfarBenchCarriages`, `-PfarBenchDetail`, `-PfarBenchLabel`). Server side (`bench.FarTrainBench`): synthetic Create trains (mic-fixes' construction) on two 16-sided rings of radius 300 and 500 around (1000, 0) at y -60, driven at 0.6 blocks a tick by Create's own railway tick, taken out before the server saves. Client side (`FarBench`): opens or creates the flat world `farbench`, parks a spectator at (0, -45, 0) looking east, waits 900 ticks, takes a screenshot, runs vanilla's 10-second client profile and quits. | Real Create trains, moved by Create and sent by Create's sync, without a world anyone built. Sixteen sides turn 22.5 degrees a node, under the 29 degrees Create allows between straight edges. The rings stay 500 to 1500 blocks out: past vanilla's view, inside DH's, and away from entity-ticking chunks, where a synthetic carriage would try to spawn a contraption it does not have and drop its train. |
| DHT23 | **Bandwidth is counted by encoding Create's client copy of each packet again with Create's own codec** (`client.TrainSyncMeter`, bench only). | Single player's connection is in memory, so nothing on the wire can be counted. The client copy holds every encoded field with the server's values, so the count is the packet body exactly; the payload id, length prefix and compression are not in it. |
| DHT24 | **Frame cost is read from vanilla's client metrics profile** (the F3+L recording, which carries DH's "Custom Objects" section) plus the controller's own timer over the same window. | §7's method. Both are CPU time on the render thread; the GPU's time is in neither. |

## For Ben

- **The band between the simulation distance and the render distance.** Create spawns a carriage's entity only
  in entity-ticking chunks, which end at the server's simulation distance. A carriage farther out but inside
  the client's loaded chunks has no entity, so vanilla draws nothing there, and the hide test (§5's rule) hides
  the proxy too, because vanilla draws over DH in loaded chunks (research note §5). So where a client's render
  distance is above the server's simulation distance, a train crossing that band vanishes until it reaches the
  DH range. That is how Create behaves without DH as well. Nothing to decide now; phase 2 could draw those
  carriages with a vanilla-side renderer if it matters.
