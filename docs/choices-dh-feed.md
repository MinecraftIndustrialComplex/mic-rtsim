# Choices: the far view, phase 2 (DHT, continued)

Reversible choices made on 2026-10-22 by the far-view agent (branch `feat/dh-feed`), building phase 2 of
research/distant-horizons-2026-10-21.md §13 ("our own feed, and dragons") under DECISIONS.md DH-1 (opt-in
clients) and DH-2 (show what your eyes would see). Phase 1's choices are DHT1 to DHT24 in
`docs/choices-dh-trains.md`; these continue from DHT25. Each can be undone. What is measured and what is not is
at the end, under "Measured". The wire formats are in `docs/boundary.md` §12.

mic_rtsim_dh is still not in the pack; it ships with rtsim's pack commit (Ben, 2026-10-22).

## Records and payloads

| # | Choice | Why |
|---|---|---|
| DHT25 | **A file of its own for phase 2**, numbering on from DHT24. | Phase 1's file is about Create's own sync; this phase replaces that with a server feed and adds dragons, so the two read better apart. |
| DHT26 | **The far view's ranges are 0x00D0 to 0x00DF (latest-wins), 0x0C00 to 0x0CFF (journaled) and 0x8C00 to 0x8CFF (commands); FAR_LEG is 0x8C00.** 0x00C0 to 0x00CF, 0x0A00 to 0x0BFF and 0x8A00 to 0x8BFF are left free. Checked against main, every branch (`git branch -a`) and every worktree under `mods/` on 2026-10-22: nothing else uses them. MOWZIE.md's 0x8807 to 0x880B and CHIEF_PAID 0x0814 are untouched. | Concurrent agents take the same ids, so the gap leaves room for the next families (the topology's 0x00B0 is the last taken). |
| DHT27 | **FAR_LEG is event driven and always on while rtsim-jni runs a session.** Every dragon goes out once at a session start (one bounded pass over the rows), then one record whenever a leg is booked or cancelled, a state is tracked, or a dragon is created or removed. It is not gated by subscribers. Switching ecology off clears Java's table and drops records still on their way, and switching it on resends every dragon. A changed dragon whose page will not load is skipped and counted, as the session-start pass does. The engine side never passes over every dragon on a step. | Dragons are few and their legs change rarely, so the trickle costs little. The engine does not know who is subscribed, and a session that starts with nobody subscribed must still have the table when someone subscribes. Pinned by `far::tests::the_bytes_java_reads` and `FarLegsTest` (the same 80 bytes on both sides), and by `far_legs_follow_a_dragon_from_its_body_to_its_lair` in rtsim-jni's protocol tests. |
| DHT28 | **Two optional payloads, `mic_rtsim:far_sub` and `mic_rtsim:far`.** The subscription carries its own version. A message is one byte array, encoded by plain Java (`FarWire`) with varints relative to the subscriber's block. | §6.2: a client without them joins a server with them, and the other way round. Plain Java keeps the format unit tested without Minecraft. A versioned subscription lets a later client and an older server ignore each other quietly. |
| DHT29 | **Handles per subscriber, and only what changed** (`FarView`): an INTRO when an object is new or its looks change, a LEG when its leg changes, a SAMPLE when it moved an eighth of a block, a REAL when its entity id changes. An object the filter stops choosing is held for 2 sweeps (PLACEHOLDER), then GONE. RESET starts a view over: the server sends one on every subscription and on a dimension change, and a client whose message did not decode subscribes again. Nothing is sent in a second where nothing changed. | Legs trickle, as §6.3 says. A still train or a roosting dragon costs nothing after its first second. The hold keeps the hand-over seamless (DHT31) and stops an object at the budget's edge from churning a new handle every second. A RESET on every subscription is the only way back from a client that lost its view: before the review, the server kept sending diffs to a client that held nothing. |
| DHT30 | **A subscription is the renderer's own: DH's radius in whole chunks, its detail tier, the tier's object budget, and the kinds it draws (dragons and carriages).** The server caps the radius at 16,384 blocks and the budget at the tier's 48, 128 or 320 (PLACEHOLDERs), sends objects within the radius plus 64 blocks, dragons first and then nearest first. A client asks at most once a second, and only when its wants changed. | §6.3. Dragons first because there are few of them and they matter more on a skyline than the 200th carriage. The 64-block margin is phase 1's (DHT5). |

## Sources and the hand-over

| # | Choice | Why |
|---|---|---|
| DHT31 | **One item per id, the higher priority wins.** The engine's legs (priority 0) and Ice and Fire's bodies (priority 10, `DragonFeedSource`) give the same id, the dragon's agent id. A BODY leg is never drawn from the engine. So when a dragon is embodied its body's samples replace its leg on the same handle, carrying the entity's network id, and when the body goes the engine's leg comes back on that handle with entity -1. Between the body's capture and the engine's next leg the dragon is in neither source; DHT29's hold keeps its handle through that gap. | §5's hand-over with no seam: the client keeps one mover, so nothing pops. The entity is the truth while it exists. Tested end to end by `FarFeedGameTests` (core: leg, body, still body, the gap, leg again, a new subscription, the hold and GONE, and a withdrawn subscriber) and `FarDragonFeedGameTests` (Ice and Fire: a real puppet's id, looks and position), and on the client by `FarFeedReceiverTest`. |
| DHT32 | **On the client, legs keep to the server's clock.** (Samples ran one second behind until DHT44: a mover moved from where it was drawn to each new sample over the next 20 ticks.) A flown leg is timed in game time and drawn on the far clock through an offset (the far clock minus the client's game time, which vanilla corrects to the server's once a second); when the offset moves two ticks, every flown leg is timed again. A hunt is drawn circling at 0.5 blocks a tick (PLACEHOLDER), spiralling onto its circle over 40 ticks (PLACEHOLDER) from where the dragon was drawn and turning the way it was flying. | §6.4. With a server below 20 ticks a second, a client clock alone would put a dragon ahead of the server's by a quarter of its leg. The hunt's circle is cosmetic: the engine does not say where in its radius it is. The spiral stops a dragon that arrives at the centre jumping 24 blocks or more onto the circle. |
| DHT33 | **The band between simulation distance and render distance is drawn by a vanilla-side renderer (`FarBandRenderer`).** A mover is hidden while the client has its entity within 64 blocks of the mover; otherwise DH draws it outside the client's loaded chunks and the band renderer draws it inside them (and DH as well toward their far edge, DHT45), after the entities, with vanilla's depth, as the same boxes DH would draw (finest step, flat colour, one shade per face), lit by vanilla's lightmap at full sky light as DH's box shader lights its boxes (DH's default sky light 15, block light 0; `VanillaLight`) and faded into vanilla's fog. | Create spawns carriage entities only in entity-ticking chunks, and an engine dragon that is not embodied has no entity anywhere, but vanilla draws over DH in loaded chunks: without this, a train crossing the band vanished (phase 1's open item). DH-2 says draw what your eyes would see. The 64-block check stops a stale or reused entity id hiding a proxy somewhere else. Unlit, a band box was full bright at night beside a DH box that was not. The lightmap is emulated on the CPU, leaving out night vision, the darkness effect, a boss's darkening and block light. |
| DHT34 | **Create's trains come from the server (`TrainFeedSource`), read as Create's own map sync reads them** (the leading and trailing bogey points on the graph, or the rotation anchors off it), with each carriage's entity id while the server has one. While the feed answers (`FarFeedClient.active()`), the client asks Create for nothing (`TrainFarSource` stops). A server without the feed still gets phase 1's Create sync. Carriage ids are `0x54 << 56` ('T'), 48 bits of a hash of the train's id and the carriage index. | §13 item 2. One sweep a second reads every train once for all subscribers. The 'T' namespace keeps carriage ids apart from the engine's agent ids. |

## Dragons in DH

| # | Choice | Why |
|---|---|---|
| DHT35 | **A dragon is one body group and one wing group per pose, played as a flipbook** (`DragonModel`): body, neck and head, tail; wings UP, LEVEL and DOWN on MEDIUM and HIGH (the cycle up, level, down, level), UP and DOWN on LOW. Only the shown pose's group is active, so a wingbeat flips flags and never re-cuts. A roosting dragon folds its wings (body only). Each is cut for one of 16 headings, changing step only once the heading is 0.15 of a step past halfway, and re-cut when it grows a stage, changes colour or the tier's step changes, paced as the trains are. Material TERRACOTTA, wings 0.7 as bright as the body. Each dragon starts its wingbeat at its own frame. | §4.2's shape (4 groups, 2 draws). The hysteresis keeps a dragon flying along a step's edge from re-cutting every frame. A flight that flaps in step looks mechanical. |
| DHT36 | **Dragons take the budget first**: up to 8, 24 or 64 dragons by tier (PLACEHOLDERs), nearest first, each two active groups (one at rest) and its boxes. The carriages get what is left. Dragon proxies stay registered up to twice the tier's dragon count, and one nobody wanted for 10 s is released. | DHT30's reasoning, on the client. |
| DHT37 | **One palette for the body and the leg** (`DragonFeedSource.PALETTE`, registered for the legs through `FarLegs.registerStyle`): style = species (fire, ice, lightning), variant = colour 0 to 3. The colours are by eye (PLACEHOLDER), as are the stage lengths 3, 6, 10, 14, 20 blocks for stages 1 to 5, the wingspan of 1.4 lengths, the wing rises and the wingbeat of 4 frames of 5 ticks. | With the same looks on both sides, the hand-over needs no new INTRO (the GameTests check it). Ice and Fire's render sizes per stage are not verified. |

## Tests and bench

| # | Choice | Why |
|---|---|---|
| DHT38 | **Test hooks: `FarFeed.setSink`, `setPlayers` and `removeSource`; the client's message handling is its own plain-Java class (`FarFeedReceiver`).** | A GameTest's players never log in, so the player list does not have them. Unit tests have no Minecraft classes, and `FarFeedClient` refers to the payload types. |
| DHT39 | **The bench gets dragons and a band.** `FarDragonBench` is a far source on the single-player server, made of made-up dragons: `-PfarBenchDragons=<n>` far ones (500 to 1,400 blocks east; a third fly 30 s legs, a third hunt, a third are sampled flying circles as embodied dragons are, with no entity on this client), `-PfarBenchBandDragons=<n>` 120 to 150 blocks east. `-PfarBenchBandTrains=<n>` puts trains on a 140-block ring around spawn. A band run sets the simulation distance to 5, vanilla's least (`-PfarBenchSim`), so the band ring lies in loaded chunks that are not entity-ticking; the bench quits if vanilla refuses the value. `-PfarBenchFeed=false` keeps the feed off, so Create's own sync carries the trains, for the bandwidth comparison. The feed's bytes are counted on the client, as the payload's byte array. | Neither the engine nor Ice and Fire is needed to load the renderer. A synthetic carriage in an entity-ticking chunk tries to spawn a contraption it does not have and drops its train (DHT22), so the band ring needs the low simulation distance. The byte array is the whole message body, as DHT23 counted Create's. |

## The hand-over, second pass

A capture session on 2026-10-22 (media in `/home/greencheetah/mic-media/dh-2026-10-22/`) found the hand-over
between a far object and its real body showing seams. These fix them, on branch `fix/dh-handover`. Fixes to the
boxes' sizes were left out on purpose: another branch may draw real models instead of boxes. The rules here
(`HandOverRule`, `FarHide`, `FadeRing`, the receiver's run-on) are apart from the box drawing, so such a renderer
keeps them.

| # | Choice | Why |
|---|---|---|
| DHT40 | **A body's entity id goes to the client as the entity joins its server level, ahead of its spawn packet** (`RtsimApi.farEntityJoining`, `FarFeed.embodied`, `FarView.real`): Ice and Fire's bridge calls it just before `addFreshEntity`, and the Create plugin on the server's `EntityJoinLevelEvent` for a carriage entity (Create sets the carriage's train id and index before it adds the entity). Each subscriber holding the object gets a one-record message (one REAL) at once; the sweep's item then says the same and sends nothing more. The client hides its far object from the frame its level has that entity near it (DHT33's test). | The far object was dropped only at the next once-a-second sweep: a dragon's box stayed under its body for 2 to 13 frames, a carriage's for about 0.85 s. NeoForge handles the payload on the main thread in order with vanilla's packets, so the REAL is applied before the spawn packet is, and the hide test needs no new state. A client-side match was looked at and dropped: NeoForge applies a carriage's spawn data (its train id) after the entity joins the client level. If `addFreshEntity` fails, the id names no entity and the far object stays drawn. |
| DHT41 | **A new dragon body faces the way the far view drew it**: yaw, body yaw and head yaw (`IafDragons.face`, set again after the bridge writes Ice and Fire's own state into the new body) from the last sweep's item of it (its leg's or sample's direction), else the engine's last flown leg (`FarLegs.Rec.hx`, `hz`, kept through hunts and roosts), else toward its flight target, else east (what a client draws for a dragon it never saw fly), else any way. Its flight target is set before it joins, so its spawn packet already says it flies. | The body spawned facing a random way, 80 to 124 degrees off its box. |
| DHT42 | **A far LEG is drawn at the body's middle**: the engine's y is the feet, as the entity's is; the leg's y is that plus the species' centre for its stage (`FarLegs.Style.centreByStage`, `DragonFeedSource`: half of Ice and Fire's 1.2-block base height times `DragonSize` at mid-stage, about 0.42, 1.05, 2.05, 3.41 and 4.2 blocks for stages 1 to 5, rounded to whole blocks on the wire). | A body's samples are its middle, so the box sat about 3 blocks below the body at the hand-over. The wire's layout is unchanged; what a LEG's y means is (boundary.md §12.2). |
| DHT43 | **The dive after the take-over is Ice and Fire's flight ceiling, not ours**: Ice and Fire clamps a flight target to `maxFlight` (256 in the pack) and forces a dragon above it down. The capture flew its dragons at y 320; the engine cruises at 180. The bridge logs a warning, the first 3 times, when a body materializes or is steered above the ceiling (`IafDragons.maxFlight`). Nothing else changes. | Not reproducible below the ceiling; with flying set before the spawn (DHT41) a body never shows a frame of a standing pose in the air either. |
| DHT44 | **Sampled movers are run on, not trailed** (`FarFeedReceiver`): from where it is drawn, a mover heads for where its last two samples say it will be when the next comes, each point on along its line at its speed at the sample, changing speed as it was (at most doubling it a sample), and runs 10 ticks (PLACEHOLDER) past that at the same pace should the next be late, then waits, at most 40 ticks past its sample. One braking to a stop runs on to where it comes to rest, even when that is after the next sample is due. A mover is never drawn going backward along its way (its velocity now, or the one it had if it stopped): one that stopped short of where the run had it waits there. A sample more than 8 blocks a tick (PLACEHOLDER) from the last, or 160 blocks from where the mover is drawn, is a jump (a teleport, a toroidal world's wrap): drawn there at once, standing until the next sample. The server sends a sample once more, unchanged, the first sweep a mover stood still (`FarView`), so a stopped mover stops rather than running on. The first sample after a leg goes on at the leg's pace as drawn. | Drawn a second behind, a box trailed its train by about half a second of travel and jumped back toward the station as it left. Run on, a steady train is where it is (`FarFeedReceiverTest`). A train braking hard passes its stop by a few blocks (under 6 in the test) and waits there: the review found it slid back to the stop and, for a train stopping dead, about 10 blocks backward, which reads worse than a box a few blocks long. A run past a teleport would have slid the box across the map for a second. |
| DHT45 | **DH draws a mover in a loaded chunk too where it could be in DH's own vanilla fade, and the band draws DH's cut there** (`FadeRing`, `HandOverRule`, `FarTrainView.shownCut`, `FarDragonView.shown`). DH 3.3.3's vanilla fade mixes each vanilla pixel toward DH's image by a smoothstep of its 3D distance from `(near + 16) * 1.5` to `(near + 16) * 1.9` blocks, `near` being DH's near clip plane (134 to 170 blocks at render distance 10 on 16:9; nearer when the camera moves at 10 blocks a second or more). `FadeRing` reads that plane from DH once a frame (`RenderUtil.getNearClipPlaneInBlocks`, falling back to the same formula on the API's overdraw setting). DH draws a mover with no entity whose midpoint is in a loaded chunk once its 3D distance plus its reach is within 2 blocks (SLACK) plus 16 (PREPARE) of the fade's start, both PLACEHOLDERs. There the band draws the same boxes DH drew (the carriage's cut, the dragon's heading and step), so vanilla's pixels fade into a copy of themselves. | The band's box faded into the terrain DH drew behind it: a train vanished at 160 to 170 blocks for about 0.7 s and faded in over 0.6 s, and on leaving, nothing was drawn for about a second. Nearer than the fade, vanilla's pixels cover DH's copy exactly, so drawing both shows one. PREPARE covers DH building a new group's boxes off-thread, which it does only for an active group. When DH leaves its fade out (`vanillaFadeMode` NONE, a renderer mode other than DEFAULT, an Iris shader pack; `ClientApi.shouldRenderFade`), `FadeRing` puts the ring nowhere and DH draws only outside the loaded chunks, as before; a DH version whose settings it cannot read falls back to the API's renderer mode and Iris's own API. A box re-cut in the overlap shows DH's old boxes for the frames its async rebuild takes. Inside the fade, DH's copy can show against the sky past the band's box's edge where vanilla has no pixel; DH does the same for every vanilla entity there, and the box is the same box. More DH groups are active near the ring: DH draws a mover there that the band draws too, so a tier's budget (DHT30, DHT36) counts it. |

## Real models (DH-6)

Ben asked for the far view to draw what players built: "Trains don't match the trains players built they are
just black boxes" and "Dragons look like a blocky mess and not like dragons". A spike on 2026-10-22
(`docs/spike-dh-real-models.md`, media in `/home/greencheetah/mic-media/dh-spike-2026-10-22/`) drew each carriage
as its own blocks and each dragon as Ice and Fire's own model inside DH's pass, and passed. Ben approved the
plan ("Looks good, go do some tests and implement if they look good", 2026-10-22), the wire additions with it
("carriage layouts, and dragon species, colour and age in the feed"); DECISIONS.md DH-6. These choices are on
branch `feat/dh-real-models`. The boxes stay as the fallback wherever a model cannot be drawn.

| # | Choice | Why |
|---|---|---|
| DHT60 | **The far feed's message version is 2, and an INTRO also names the object's shape and age** (boundary.md §12.2): `i64 shape` (a content hash, 0 for none) and `zig age` (a dragon's age in days, -1 for none), after the colour. `far_sub` carries the client's version 2 too; a v1 client or server is told no as any other version is. An INTRO is sent again when the shape or the age changes. When a dragon goes from its body back to a LEG, the client keeps the age the body's INTRO gave it, so it is drawn the same size. No compatibility with version 1. | A carriage of one design is a hash, so a train of 20 costs 20 longs a sweep and the layout goes once (DHT62). The age sets a dragon's size as Ice and Fire sizes it, which the stage alone does not (a stage spans 25 days). Nothing is released, so there is nothing to stay compatible with. A LEG still carries only the stage, so a dragon that was never embodied on this client is drawn at its stage's middle age. |
| DHT61 | **A carriage's shape is its blocks as Create keeps them, and its bogeys** (`CarriageShape`, `CarriageShapes`): the assembly direction, a palette of global block state ids, the blocks in a box in (y, z, x) order by gap, and each bogey's place, style id, size id and whether it is upside down. Read from the carriage's live contraption, or, with no entity, from the saved entity Create keeps in `Carriage.serialisedEntity` (package-private, by reflection; missing, the carriage is a box). Kept per carriage while that source is the same object (held weakly), so it is encoded once per source; encoding spends at most 2 ms a sweep per level (PLACEHOLDER), and past it a carriage keeps the shape it had, or is a box, until a later sweep. A door or lever opened in place changes a live contraption without a new source, so the shape stays the old one until the entity unloads. Limits a decoder holds: 4,096 states, 1,024 blocks a side, 65,536 blocks, 4 bogeys, 256-byte ids; a layout past them is not sent (a box). Block entity data, actors, storage, seats and glue are left out. | The far case is a carriage with no entity, and Create's saved form is the only copy then. Global block state ids are what chunk packets use, true for one server and its mod list. The spike measured 85 to 120 bytes deflated for a carriage, and a decoder that refuses a hostile layout before it allocates. Block entity renderers (a chest's lid, a sign's text) are not drawn far off. |
| DHT62 | **A shape's bytes go once per subscriber, in the `mic_rtsim:far_shape` payload, after the message that names it** (`FarFeed.sendShapes`): nearest first in the sweep's order, at most 64 KiB (PLACEHOLDER) a sweep per subscriber, the rest the next sweep. The hash is FNV-1a 64 of the raw layout with a finalizer (0 becomes 1); the payload is the hash and the raw-deflate bytes, at most 32 KiB, and the client checks the hash, the inflated size (1 MiB) and the layout before it keeps or bakes it. A RESET clears what the server has sent and what the client keeps, together; a subscriber held to more than 1,024 shapes gets a RESET. The client keeps at most 4,096 shapes or 40 MiB. A server must have both payloads negotiated with a client before it subscribes it. | After the message, so a RESET in that message cannot clear a shape sent ahead of it. Nearest first, because the near ones are seen first. The budget keeps a station of new designs from bursting the connection. Both sides clear on the RESET, so neither has to ask what the other holds. A shape not here yet is drawn as nothing for a short grace, then as a box. |
| DHT63 | **A plugin's far look draws its kind as the real thing, in DH's pass and in the band, with the boxes' hand-over** (`FarLook`, `FarLooks`, `FarModel` in the core; `FarModelRenderer` in mic_rtsim_dh). mic_rtsim_create registers the carriages' look and mic_rtsim_iceandfire the dragons', each from its own client entry; a look bakes static vertex buffers itself within 2 ms a frame (PLACEHOLDER) shared by every look, at the frame's start, and places a mover's model about its midpoint. The renderer draws them from DH's `DhApiBeforeGenericRenderCleanupEvent` the first time it fires in a frame, with DH's framebuffer, matrices and depth function, vanilla's fog off, and every GL state it touches put back (`GlState`); and in the band after vanilla's entities, with vanilla's matrices, depth and fog. Who draws a mover is `FarHide` as for the boxes (no model while the client has the entity; DH beyond the loaded chunks and in DH's vanilla fade; the band inside them). A mover whose look has no model (no shape, a shape that would not decode, an unknown species) is a box; one whose model is baking is drawn as nothing for up to 2 s (PLACEHOLDER), then as a box until it is baked; with a model, its DH groups go dark and are released after 10 s. Each kind is held to its tier's count (8, 24 or 64 dragons; 48, 128 or 320 carriages), nearest first by horizontal distance (the boxes' order), counted after the hand-over and before the frustum, a dragon model reaching as far as its boxes; in DH's pass models and boxes share a dragon count (a box takes its place in the models' count and a model in the boxes', whether or not it draws), and the carriages' is the feed's object budget, which bounds models and boxes together. In the band only the models keep to the count: the band's boxes (`FarBandRenderer`) have none. A look's cold first draws (Create's bogeys) run at the first frame the far view runs: behind the loading screen as the client joins, or in the options screen when the detail goes on. The bench's `-PfarBenchModels=false` draws every mover as its box. | Ben's ask (DH-6). In DH's pass, DH terrain hides the models, they hide it, and DH's own fog fades them by the depth they wrote, as it fades the terrain; the spike's pictures and its hill test showed it. The boxes' hand-over already fades into DH's copy (DHT45), and a model drawn by the same rule fades into the same model. A box shown for the frames a bake takes would only flicker; a mover whose shape never comes still shows as a box. The caps are the boxes' (DHT30, DHT36), and a mover is a model or a box, so one count for both keeps the tier's promise. Meshes cached by hash outlive a far feed RESET: the hash names the layout, so a mesh is still right. |
| DHT64 | **A carriage's mesh is its blocks drawn by the game's block renderer against a tiny level of the carriage, with Create's bogeys copied in, one static buffer per block layer, shared by hash** (`CarriageLook`, `ShapeLevel`, `BogeyMeshes`). Faces between two blocks of the carriage are culled and shaded as a chunk mesh does them; light is open sky with no block light, tints are plains'. Only blocks drawn as models; block entity renderers (a chest lid, a sign's text, a Create belt) are not drawn. A bake runs a block at a time inside the frame's budget across frames; a block that throws is left out and the carriage begun again (at most 8, then it is a box). A carriage that draws nothing (only block entity blocks, and bogeys of a style with no mesh) is a box. Translucent layers (glass) are baked once, unsorted, so their faces can layer wrongly from some angles close in. Bogeys are drawn once per style, size, axis and way up by Create's own renderer, wheels at rest, and copied; every style is drawn at the far view's first frame (DHT63). Placed as Create places a carriage: the bogeys' midpoint on the segment's midpoint, the forward axis along the segment, heading then pitch. A mesh nobody drew for 60 s is freed, at most 512 meshes (PLACEHOLDERS); past that, new designs wait as boxes. | The block renderer is what a player's own build looks like in a chunk, every mod's block models included. The tiny level gives the renderer the neighbours it culls against. Create's first bogey draw is cold (about 180 ms with Sodium in the spike), which would be a hitch inside a frame. A half-written block would break its layer's quads, so a bake that hits one starts over without it. Block entity renderers need the block entity's data, which the shape does not carry (DHT61). An upside-down bogey is drawn flipped about its block's middle at the block the server found it at; a carriage mixing upright and upside-down bogeys is not checked. |
| DHT65 | **A dragon's far model is Ice and Fire's own renderer's output on a client-only stand-in, baked per species, colour and stage at 12 points of the flight loop and once at rest; the nearest two within 300 blocks are drawn live** (`DragonLook`, PLACEHOLDERS). A stand-in is an entity made with the species' type and never added to a level (it never ticks, no server knows it, nothing but the look holds it, and its id is negative, below every id a server gives; D3). NeoForge still tells listeners of its construction and its renders. A bake is one render a frame-slice: two warm-up renders, the 12 frames 5 ticks apart, two at rest, the rest pose. Each frame is a static buffer per render type the renderer asked for, in its order. A far dragon is drawn at the frame of the loop nearest its time (offset by its key, so a flock does not beat in step), turned to its heading with the level's lights turned back to the baked normals, scaled from its stage's middle age to its own age as Ice and Fire scales it (its render size), and stood on its feet below the feed's point (the hitbox's middle, `DragonFeedSource`). A live stand-in is started on the bakes' clock: it gets the two warm-ups a bake gives its probe, ending at the last start of the loop at or before its time (a whole number of loops after the probe's), and gets a render at each later start of the loop, so its wings and the baked frames' stay at the same point of the loop and a dragon going from one to the other keeps its beat; a new one starts when it lands or takes off, or when time runs back. Live once a frame: in DH's vanilla fade, where both passes draw, both draw the baked frame it is at, so the fade never mixes two poses. A set nobody drew for 60 s is freed, at most 32 sets; past that, the set least recently drawn makes room if nobody drew it for 1 s, else the new look waits as a box. An ask forgotten after 10 s frees a half-baked set. A set that would not bake is a box until the level changes. Live stand-ins nobody drew for 5 s are dropped. Leaving the world, or a new level, frees everything. Translucent render types are baked unsorted, as the carriages' (DHT64). | The spike measured 20 live stand-ins at 12 ms of CPU a frame and the baked poses at 0.36 ms; two live ones keep the near wings smooth for little. Twelve frames of a 60-tick loop read as a wingbeat at the distances DH draws. Baking at the stage's middle age and scaling keeps the sets to 60 at most. Ice and Fire's controllers (GeckoLib's) start their loop at the render that ends their transition, so the same warm-ups put a stand-in on the bakes' loop; GeckoLib starts each later loop at the first render past the last one's end and drops what it ran over, about a sixth of a tick a loop at 60 frames a second, and after a gap the whole gap, so a render at the end itself keeps the beat; a stand-in shares no id with a real dragon, which GeckoLib could otherwise take for a re-render of the stand-in and draw in its pose. This replaces the plan's impostor quads with baked real-model poses: the spike's impostors were flat at every angle where the poses were the model from every side, for the same cost. A dragon moving from an engine leg (stage only, so its stage's middle age) to its body (its own age) changes size once. A dragon model's hand-over reaches as far as its boxes (`FarDragonView.reach`), so the two views count the same dragons; the bench logs how far a model's radius passes that, and HandOverRule's 18 blocks of SLACK and PREPARE cover it. |
| DHT66 | **The bench dresses its synthetic carriages as Create carriages of four designs** (`BenchCarriages`): a steam locomotive first, then wooden coaches, copper tank wagons and glass observation cars in turn, made as Create stores a carriage with no entity (`Carriage.setContraption`), nothing placed in the world. `-PfarBenchDress=false` leaves them bare. | The bench's carriages had no blocks, so they could only be boxes, and its numbers are wanted for the real models. |

## Measured

Bench round 3 on 2026-10-22, 10:21 to 11:11 (neoforge/README.md, "The far view's dev client"), on this branch
between 30c6bd6 and 1500bc5 (the commits between change only what the feed's stop log prints), and r3long2
at e5fa3fb, until 11:24. Ben's laptop
(Ryzen, 24 threads, its Radeon), single player, render distance 12, simulation distance 12 (5 in the band run),
DH's default settings, MEDIUM unless named. Each profile is vanilla's 10-second client recording after 900
warm-up ticks. Trains are phase 1's (4 carriages, 500 to 1,500 blocks east); dragons are `FarDragonBench`'s
(DHT39).

| Run | What ran |
|---|---|
| r3c0, r3c0z | No objects: the controls, first and last in the round |
| r3t50, r3t50s | 50 trains (200 carriages) on the feed |
| r3t50nf | 50 trains with the feed off, so Create's sync carries them |
| r3d24, r3d24s | 24 dragons |
| r3d24t50 | 24 dragons and 50 trains |
| r3d64t50h | 64 dragons and 50 trains on HIGH |
| r3band | 4 trains (16 carriages) on the 140-block band ring and 6 band dragons |
| r3long, r3long2 | As r3d24t50 with 6,400 warm-up ticks, for the server's sweep once the JIT has run |

**The machine was not quiet, and the GPU changed state mid-round.** Other sessions' jobs held the load average
between 10 and 24 in the profile windows. In the first three runs the GPU was at its middle clock level (600 to
1,066 MHz); from r3d24 to the end it held its lowest, 600 MHz at 15 W and 67°C. After the round the laptop's
platform profile read `low-power` (on AC); nothing in this work set it. So fps and GPU time do not compare
across runs: the two empty controls ran at 258 and 184 fps, and DH's own clouds cost 145 and 330 us a frame in
them. What holds is one of our groups against one of DH's cloud groups in the same window.

### Render thread (CPU), from the profile

Microseconds a frame. "Our draws" is the `mic_rtsim_dh` section inside DH's "Custom Objects", with the groups
drawn in brackets. The controller and band columns are their own timers over the same window.

| Run | Objects held | fps | Custom Objects | Our draws | Per group | DH clouds | Per cloud group | Controller | Band pass | Load |
|---|---|---|---|---|---|---|---|---|---|---|
| r3c0 | 0 | 258 | 501 | none | | 145 (38) | 3.8 | 1 | 0 | 10 to 11 |
| r3t50 | 128 | 123 | 1,991 | 921 (96) | 9.6 | 214 (37) | 5.8 | 102 | 12 | 15 to 17 |
| r3t50s | 130 | 85 | 3,133 | 1,057 (96) | 11.0 | 355 (37) | 9.6 | 244 | 26 | 18 to 20 |
| r3t50nf | 200, Create's sync | 210 | 1,487 | 572 (96) | 6.0 | 171 (37) | 4.6 | 118 | 10 | 19 to 20 |
| r3d24 | 24 | 67 | 2,952 | 703 (48) | 14.6 | 489 (37) | 13.2 | 82 | 13 | 23 to 24 |
| r3d24s | 24 | 107 | 2,233 | 517 (48) | 10.8 | 372 (37) | 10.1 | 67 | 14 | 21 to 23 |
| r3d24t50 | 129 | 83 | 3,514 | 1,147 (96) | 11.9 | 389 (37) | 10.5 | 252 | 32 | 19 to 20 |
| r3d64t50h | 264 | 80 | 4,974 | 2,284 (256) | 8.9 | 297 (37) | 8.0 | 377 | 48 | 15 |
| r3band | 22 | 211 | 958 | none | | 288 (37) | 7.8 | 9 | 163 | 15 to 16 |
| r3c0z | 0 | 184 | 1,070 | none | | 330 (37) | 8.9 | 2 | 1 | 16 |

- **One of our groups costs 1.1 to 1.3 times one of DH's cloud groups** in the same window (1.7 in r3t50).
  Phase 1 had them equal. A dragon is two groups (its body and the shown wing pose), so it costs about two
  carriages.
- **The far view's whole render-thread cost at the heaviest load**, r3d64t50h (64 dragons and 96 carriages
  drawn on HIGH): 2,284 us of draws, 377 us of controller and 48 us of band pass, 2.7 ms of a 12.5 ms frame.
  The controls moved too much (Custom Objects 501 and 1,070 us) to subtract them.
- **A wingbeat flips group flags and cuts nothing.** The 24-dragon windows had 1,235 to 1,261 flips (4 a
  second for each dragon) against 77 to 90 re-cuts, from turning hunters.
- **The band pass** walks every mover each frame: 10 to 48 us with nothing in the band (24 to 264 movers held).
  Drawing, it cost 163 us for 10 movers and 168 boxes in r3band, about 16 us a mover, because it cuts and
  tessellates each band box every frame. The other 12 band movers were behind the camera; the screenshot shows
  the six dragons and the two trains' carriages in view, drawn in loaded chunks where DH draws nothing. If the
  band ever holds many movers, caching the cuts as the DH controller does is the fix.

### GPU

**Not separated.** Vanilla's whole-frame timer query gave medians of 1.9 to 6.9 ms a frame, the controls 3.8
and 2.2 ms. On a shared GPU whose clock changed mid-round, that separates nothing, so the boxes' GPU time is
unmeasured, as in phase 1.

### Bandwidth per subscriber

Counted on the client as the payload's byte array (DHT39), over the 10-second window:

| Run | Objects held | Messages a second | Records a second | Bytes a second |
|---|---|---|---|---|
| r3d24, r3d24s | 24 dragons | 0.9 | 7.2, 6.9 | 115, 111 |
| r3band | 22 | 0.9 | 14.0 | 191 |
| r3t50, r3t50s | 128, 130 | 1.0, 0.8 | 134, 104 | 1,695, 1,289 |
| r3d24t50 | 129 | 0.9 | 100 | 1,236 |
| r3d64t50h | 264 (HIGH) | 0.9 | 193 | 2,551 |

**Create's sync for the same 50 trains (r3t50nf, feed off): 4.01 packets a second of 10,302 bytes, 41,283
bytes a second** (phase 1 had 41,020). With the feed on, Create's map sync sent its 2 packets at the start,
before the feed's first message, and nothing after (DHT34).

So for 50 trains on MEDIUM the feed sends 1.3 to 1.7 KB a second where Create's sync sends 41.3 KB: 24 to 32
times less. Two things make the difference. The feed sends at most the tier's 128 objects, nearest first,
where Create sends all 200 carriages. And it sends once a second only what changed, about 10 to 13 bytes a
second for each object held, where Create sends every carriage 4 times a second, 206 bytes a second for each.
On the server, r3long's one subscriber got 420,894 bytes in 297 messages (1,417 bytes a message, the first
full message included).

### The server's sweep

`FarFeed`'s stop log, on the single-player server thread: CPU time for each sweep (one a second, every
subscriber in the same tick), after the first 10 sweeps. In the one-minute runs only about 45 sweeps follow
those 10, before the JIT has compiled most of the path:

| Run | CPU a sweep | Split |
|---|---|---|
| r3c0, r3c0z | 127, 295 us | |
| r3t50, r3t50s | 1,311, 3,711 us | r3t50s: Create trains 666, dedupe 397, sends 2,345 (filter 398, view 1,246, encode 596, network 93) |
| r3d24, r3d24s | 1,492, 1,373 us | r3d24s: engine legs 40, bench dragons 99, dedupe 71, sends 910 |
| r3d24t50 | 3,024 us | Create trains 514, dedupe 345, sends 1,846 (filter 329, view 1,002, encode 429, network 76) |
| r3d64t50h | 3,731 us | Create trains 396, dedupe 351, sends 2,625 (filter 368, view 1,518, encode 648, network 80) |
| r3band | 1,352 us | |
| r3long, after 240 sweeps | 1,386 us | Create trains 217, dedupe 75, sends 655 (filter 192, view 229, encode 150, network 64) |
| r3long2, after 240 sweeps | 1,092 us | engine legs 35, bench dragons 30, Create trains 243, dedupe 63, sends 512 (filter 144, view 155, encode 129, network 63) |

- The same filter, view and encoder in a plain-Java loop outside Minecraft (not committed), once warm: 160 to
  280 us a sweep for 200 carriages with or without 64 dragons, 24 to 42 us for 24 dragons. So most of the bench
  figures are the JIT and the load: even after 240 sweeps, r3long's and r3long2's dedupe and sends (730 and
  575 us) are 2 to 5 times the loop's. Both long runs had a load average of 15 to 22.
- The longest single sweep took 12 to 70 ms of wall time in the one-minute runs and 187 ms in r3long2; the log
  does not say which sweep it was. In r3long2 it is longer than the 93 measured sweeps together (153 ms), so
  it fell in the warm-up.
- **The sends are per subscriber; the trains and dedupe are once a sweep.** Every subscriber's send happens in
  the same tick, so many subscribers make a spike once a second. By arithmetic, not measured: at r3long's 655
  us of sends, 10 subscribers would put about 6.5 ms into that one tick. Spreading the subscribers over the 20
  ticks would flatten it; it is not built.

### The hand-over's second pass, before and after

Captured on the evening of 2026-10-22 in a private headless sway at 1920x1080, 30 fps, render distance 10,
simulation distance 5, field of view 60. Before: the capture session's own take of two lanes of looping
one-carriage trains (`dh-2026-10-22/raw/rec/hand_a.mkv`, main before DHT40). After: the same scene and camera
on this branch at a9dc6709 (`dh-fixes-2026-10-22/raw/rec/hand_after.mkv`). Each frame was classed along each
lane from colour kymographs: a box is changed pixels at least 90% flat dark grey, a carriage under 75% grey.
Stills and the tools are in `/home/greencheetah/mic-media/dh-fixes-2026-10-22/`.

| Seam | Before | After |
|---|---|---|
| A carriage arrives (box to carriage), at 88 to 108 blocks | 2 arrivals: the box and the carriage both drawn for 25 and 18 frames, the box 25 and 16 blocks behind | 4 arrivals: no frame with both, none with neither; the box about 2 blocks behind |
| A carriage leaves (carriage to box), at 93 to 97 blocks | 3 departures: no frame with both or neither; the box appears 10 to 11 blocks behind | 6 departures: no frame with both or neither; the box appears 5 to 7 blocks ahead |
| DH's fade ring (block x 1540 to 1660, about 125 to 200 blocks) | 5 crossings: the train not seen for 40, 38, 36, 33 and 21 frames | 10 crossings: seen in every frame |

- **The box ahead as a carriage leaves is mostly Create's own trailing.** A carriage syncs every 3 ticks
  (`AllEntityTypes.CARRIAGE_CONTRAPTION`'s update interval) and the client moves toward each snapshot over the
  next 3 (`CarriageSyncData.approach`, Create 6.0.10), so the carriage a client draws runs 3 to 6 ticks behind
  the server's, 3 to 6 blocks at these trains' 1 block a tick. The box, run on to the server's time (DHT44),
  is drawn where the server has it. An arriving carriage's spawn packet puts it at the server's position, and
  there the box is within 2 blocks of it. A possible follow-up: as a body leaves, start its far object where
  the body was drawn and ease it onto the feed's position over a few ticks.
- The edge measures are muddied by the boxes' sizes, which this pass leaves alone: a carriage's box is a cube
  about twice as long as these carriages.
- For 2 frames as each carriage arrives, Create draws its body without its bogeys. No frame is empty.

### Not measured or not exercised

- The boxes' GPU time (above).
- A weak PC: the tier budgets (DHT30, DHT36) are still PLACEHOLDERs.
- A dedicated server with remote clients, and more than one subscriber: these runs were single player, so the
  per-subscriber spike above is arithmetic, and the connection's compression is not counted.
- Real Ice and Fire dragons in a client: the bench's dragons are made up (DHT39). The hand-over is tested by
  GameTests (DHT31); the hand-back from a real body to the engine's leg runs only in the core GameTest, as the
  Ice and Fire GameTest checks a real puppet's id, looks and position.
- The hide test with real carriages or dragons near the camera.
- A server running for hours: the longest run was r3long2's 333 sweeps.
- The band's light at night or under the darkness effect, and block light (DHT33 leaves them out).

### Traps the bench found

- **An autosave with the synthetic trains crashed the server** (r3long, at tick 6,000): Create's
  `RailwaySavedData.save` threw "MountedStorageManager is uninitialized" writing a carriage with no
  contraption. The bench now turns saving off while its trains run (e5fa3fb); the stop save still runs after
  the trains are taken out. Earlier runs ended before the first autosave. r3long2 ran 333 sweeps (about 6,660
  ticks) with it and stopped cleanly.
- **Vanilla refuses a simulation distance below 5** (30c6bd6).

### Log lines that are not ours

- Vanilla's profile dumps log "Preparing crash report" and "Negative index in crash report handler" on the
  server thread. Nothing crashed.
- The dev client warns that the core's GameTest blocks (`mic_rtsim:test_machine` and the like) have no
  blockstate files. They exist only in dev runs.

## For Ben

Nothing here blocks the merge; each is reversible before a release. Ben approved the formats below and
the pack plan on 2026-10-22 ("Approve, ship with rtsim"); the PLACEHOLDERs are still to confirm.

- **New wire formats and payloads (public interfaces). APPROVED (Ben, 2026-10-22):** the optional payloads `mic_rtsim:far_sub` and
  `mic_rtsim:far` with their versioned format (boundary.md §12), the engine record FAR_LEG 0x8C00 (80 bytes),
  the far view's id ranges (DHT26) and the carriage id namespace (DHT34). A client without mic_rtsim_dh still
  joins.
- **mic_rtsim_dh is still not in the pack. DECIDED (Ben, 2026-10-22):** it ships in the pack with
  rtsim's pack commit, not before (DECISIONS.md, DH-2 phase 2).
- **PLACEHOLDERs to confirm:** the hold of 2 sweeps before GONE (DHT29), the hunt circle's 0.5 blocks a tick
  and 40-tick spiral (DHT32), the tier budgets of 8, 24 or 64 dragons and 48, 128 or 320 objects (DHT30,
  DHT36), and the dragon colours, stage lengths, wingspan and wingbeat (DHT37), all set by eye. From the
  hand-over's second pass: the dragon's centre by stage, 0.42, 1.05, 2.05, 3.41 and 4.2 blocks (DHT42, from
  Ice and Fire's base height and size curve, not measured on a body); the run on of 10 ticks and the jump
  at 8 blocks a tick (DHT44); and the fade ring's margins, SLACK 2 and PREPARE 16 blocks (DHT45).
- **A possible follow-up:** spread the subscribers' sends over the second, before a server with many DH
  players (see "The server's sweep").
- **Real models (DH-6). APPROVED (Ben, 2026-10-22):** "Looks good, go do some tests and implement if they look
  good", with the wire additions "carriage layouts, and dragon species, colour and age in the feed": far feed
  version 2, the `mic_rtsim:far_shape` payload and the INTRO's shape and age (DHT60 to DHT62). PLACEHOLDER to
  confirm: the 64 KiB of shapes a sweep per subscriber (DHT62).
- **Another:** a leaving carriage's box appears 5 to 7 blocks ahead of where Create drew the carriage, which
  trails the server (see "The hand-over's second pass, before and after"); easing the far object from the
  body's last drawn position would close it.
