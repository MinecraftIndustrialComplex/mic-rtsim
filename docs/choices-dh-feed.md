# Choices: the far view, phase 2 (DHT, continued)

Reversible choices made on 2026-10-22 by the far-view agent (branch `feat/dh-feed`), building phase 2 of
research/distant-horizons-2026-10-21.md §13 ("our own feed, and dragons") under DECISIONS.md DH-1 (opt-in
clients) and DH-2 (show what your eyes would see). Phase 1's choices are DHT1 to DHT24 in
`docs/choices-dh-trains.md`; these continue from DHT25. Each can be undone. What is measured and what is not is
at the end, under "Measured". The wire formats are in `docs/boundary.md` §12.

mic_rtsim_dh is still not in the pack.

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
| DHT32 | **On the client, samples run one second behind and legs keep to the server's clock.** A mover moves from where it was drawn to each new sample over the next 20 ticks. A flown leg is timed in game time and drawn on the far clock through an offset (the far clock minus the client's game time, which vanilla corrects to the server's once a second); when the offset moves two ticks, every flown leg is timed again. A hunt is drawn circling at 0.5 blocks a tick (PLACEHOLDER), spiralling onto its circle over 40 ticks (PLACEHOLDER) from where the dragon was drawn and turning the way it was flying. | §6.4. With a server below 20 ticks a second, a client clock alone would put a dragon ahead of the server's by a quarter of its leg. The hunt's circle is cosmetic: the engine does not say where in its radius it is. The spiral stops a dragon that arrives at the centre jumping 24 blocks or more onto the circle. |
| DHT33 | **The band between simulation distance and render distance is drawn by a vanilla-side renderer (`FarBandRenderer`).** A mover is hidden while the client has its entity within 64 blocks of the mover; otherwise DH draws it outside the client's loaded chunks and the band renderer draws it inside them, after the entities, with vanilla's depth, as the same boxes DH would draw (finest step, flat colour, one shade per face), lit by vanilla's lightmap at full sky light as DH's box shader lights its boxes (DH's default sky light 15, block light 0; `VanillaLight`) and faded into vanilla's fog. | Create spawns carriage entities only in entity-ticking chunks, and an engine dragon that is not embodied has no entity anywhere, but vanilla draws over DH in loaded chunks: without this, a train crossing the band vanished (phase 1's open item). DH-2 says draw what your eyes would see. The 64-block check stops a stale or reused entity id hiding a proxy somewhere else. Unlit, a band box was full bright at night beside a DH box that was not. The lightmap is emulated on the CPU, leaving out night vision, the darkness effect, a boss's darkening and block light. |
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

Nothing here blocks the merge; each is reversible before a release.

- **New wire formats and payloads (public interfaces):** the optional payloads `mic_rtsim:far_sub` and
  `mic_rtsim:far` with their versioned format (boundary.md §12), the engine record FAR_LEG 0x8C00 (80 bytes),
  the far view's id ranges (DHT26) and the carriage id namespace (DHT34). A client without mic_rtsim_dh still
  joins.
- **mic_rtsim_dh is still not in the pack.** Adding it is yours to decide.
- **PLACEHOLDERs to confirm:** the hold of 2 sweeps before GONE (DHT29), the hunt circle's 0.5 blocks a tick
  and 40-tick spiral (DHT32), the tier budgets of 8, 24 or 64 dragons and 48, 128 or 320 objects (DHT30,
  DHT36), and the dragon colours, stage lengths, wingspan and wingbeat (DHT37), all set by eye.
- **A possible follow-up:** spread the subscribers' sends over the second, before a server with many DH
  players (see "The server's sweep").
