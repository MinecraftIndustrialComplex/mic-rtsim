# Choices: the oil gate, dragon half (OILGATE.md)

Ben approved OG-1 to OG-8 and OG-10 to OG-13 as recommended in OILGATE.md §8 (2026-10-21). This
file logs the choices made while building them. Each can be undone; the records in
docs/boundary.md §6 (OIL_CHUNKS, DRAGON_BREATHS, OIL_CONFIG, PLACE_LAIR, OIL_SURVEY, DRAGON_MATERIALIZE
flag 16 and its oil field) are what other code depends on. The underground half (OG-9, OG-9b to
OG-9d) is not built; its seam is described at the end.

The engine side is `crates/rtsim-ecology/src/oil.rs` (the index, field flights, refuelling, lairs,
sensing) with hooks in `engine.rs`, `dragon.rs` and `persist.rs`; the drinking raid and the roof
gate are in `crates/rtsim-industry` (`threat_drinking`, `threat::roof_pass`). Every number is a
PLACEHOLDER until OG-10's measurement and the fire-event count set it, except the reserve's capacity
and refuel level, which are Ben's (OGB-2).

Ben answered the For Ben questions on 2026-10-21 (DECISIONS.md OGB-1 to OGB-5): question 1 gave way to
his own bar, which docs/oilgate-cheese-audit.md judges the gate against; questions 2, 3 and 5 took (a);
he skipped 4, so it is taken as recommended. OGB-2 and OGB-5 are built (OGC44, OGC45).

## The roof gate (OG-11)

**OGC1. The roof gate's measure is the unroofed share of the surveyed machines.** For a flying
attack, `roof_pass = (machineBlocks - roofedMachines) / machineBlocks` from the district's latest
IND_SURVEY; 1 for a ground attack, for a district never surveyed and for one with no machines.
"Roofed" is the survey's existing rule (the block above the machine cannot see the sky), so a glass
roof is no roof. OGB-7 (Ben, 2026-10-21) changed this: `roofedMachines` now counts only the machines
whose cover lies beyond `roofReachBlocks` (OGC65), so a machine under a roof a flyer reaches counts as
open.

**OGC2. A roof stops a flyer's block damage, not its theft or its fight.** A flyer's destroyed
blocks are `floor(n * roof_pass)` and drawn only from blocks the survey flagged roofless. Theft and
the Lanchester fight do not see the roof: a dragon lands in the yard and takes what is in the tanks.
OGB-7 changed this: a roof within reach is itself one of the roofless blocks a flyer draws (OGC64), and
once it is broken the next survey finds the machine under it open.

## Knowledge (OG-8)

**OGC3. Encroachment is a new report kind (9).** Weight -25 (an assault's) and a life of 10 days: a
dragon resenses a working district once a day, so the district stays resented while it works and is
forgotten ten days after it stops. Its gossip mapping is MinorNegative 25, only to keep the mapping
total: only dragons witness encroachment, so no village ever passes it on.

## The reserve (OG-4, OG-13)

**OGC4. Fire dragons only.** `SpeciesData::uses_oil` is true for fire, false for ice and lightning:
their breath has no fuel in the pack's story.

**OGC5. The reserve's numbers.** Capacity 600 points, low at 240 (Ben, 2026-10-21, OGB-2; built
first as 100 and 40); one point lost per 3,000 ticks (the pack's
`hungerTickRate`, so oil and food keep one rhythm); one point per breath while embodied
(DRAGON_BREATHS) and one per raid pass while abstract. A closed form like `fullness`: `oil` and
`oil_at` on `Dragon`, no per-tick work.

**OGC6. Setting the reserve re-anchors it.** Any write (refuel, drink, breaths) stores the level
now and the time, so the time drain restarts from the write.

**OGC12. The gate opens on the first oil known.** `oil_gated(d)` is `uses_oil && !oil.is_empty()`:
a world with no oil known (no CDG, no Deep Time, nothing reported yet) plays as before, with no
reserve falling and nothing gated. When the first oil chunk arrives (`gate_opens`), every fire dragon
starts on a full reserve, and each one roosting gets a wake at the time its reserve will reach the
refuel level, so the gate does not wait for some other event to wake it.

**OGC13. A roosting fire dragon wakes for oil only from above the refuel level.** At roost it is
woken at the earlier of hunger and the time its reserve reaches `refuel_below` (240), the latter only
when it is above 240 now; one already low waits for hunger, whose decision weighs refuelling with
food (OGC14).

**OGC18. A dry dragon strikes at half strength.** A raid pass without oil counts at
`dry_strength_milli` 500 (OG-13: bite and claws), burns nothing, and its theft and blocks are the
melee share. The raid is counted as dry whenever any pass lacked fire, including a dragon at zero.

## Refuelling (OG-5, OG-6)

**OGC7. The oil index.** Oil chunks are kept in 32 x 32 chunk region lists (one entry per oil chunk:
its field, its place in the region, bottomless or not); fields are 8-neighbour union-find sets, the
bigger field wins a join (ties to the lower id); the index only grows (a chunk reported twice is one
chunk). Amounts are not kept: oil in the ground is in nobody's books (OG-6), so a dragon resting on
a field takes nothing from CDG's chunks.

**OGC8. Drinking from a district is not held to the theft cap.** A raid that drinks takes the fuel
goods first, in the order of `OilTunables::fuels` (Destroy's crude, CDG's oil, crude, diesel,
gasoline), up to the reserve's room scaled by the surviving share of the attackers; it can take all
of a good's stock. The fuel is the lure: a tank is what draws a dry dragon to a base.

**OGC9. Stolen fuel is booked burnt.** What a dragon drinks leaves the district's stock as theft
(`books.stolen`, the attack's report) and enters the dragons' ledger as `PreyClass::Other`, burnt, so
oil is conserved across both books (test `a_low_dragon_drinks_from_the_tanks_and_the_books_balance`).
1,000 mB is one point; a remainder below a point is dropped.

**OGC14. Refuelling weighs like hunger.** The refuel utility is the reserve's room minus the flight's
cost (`distance_cost_per_kblock`), to the nearest oil chunk within `refuel_reach` 3,000 blocks; a
dragon that is also hungry refuels only when that beats its best prey. It rests on the oil chunk for
`refuel_dwell_ticks` 1,000 (an hour of game time) and leaves full.

**OGC15. A watched tank draws the dragon in person.** A low dragon arriving at a field raids the
district within sense reach that holds the most fuel (at least `fuel_signal_mb`). When a player
watches that district, the dragon materializes there hostile instead (a real fight, no abstract
resolution), and only on the day it first senses the district; driven off, it rests on the field
instead of coming back at once.

## Field flights (OG-3)

**OGC10. The plugin's config sends the switches.** OIL_CONFIG carries `fieldFlights` and
`placeLairs` (default true). `fieldFlights` was off by default until OG-10's A/B showed the gate at
least as dangerous as In Control; since OGB-6 it is on (OGC46). The engine's own default
(`OilTunables::flags`) matches.

**OGC11. Visitors are hostile and do not stay.** A field flight's dragon materializes hostile
(flags 8 and 16) toward the player it was drawn for. When its visit ends (it drank, or it left the
player's view, which includes its body being lost when the session ends or the server crashes), one
carrying nothing leaves the sim; one carrying valuables claims a vacant lair in reach or stays a
wanderer (R18's existing kind).

**OGC24. In Control's process, reproduced.** Per dimension: one roll per geometric gap with mean
1/0.1 s (In Control's `persecond`), each picking one random player of the dimension whose presence is
fresh; that player must be within 100 blocks of an oil chunk and at or above y 15. Caps: 200 drawn
dragons alive per dimension (`maxthis`), 8 per field. The drawn dragon takes Ice and Fire's own age
roll (1 + below(80) days), colour and sex rolls, a full reserve, and arrives as a flight just past
the player's view at cruise height. The roll stops when nobody qualifies and restarts when a player
comes near a field.

**OGC25. Presence.** A player's report counts for field flights for 200 ticks; at most 32 are kept
(stale ones are dropped when the map grows past that).

**OGC19. A refuelling dragon is seen.** A dragon flying to a field passes a player's view like a
hunting one (within two view radii of its target): the sight that tells a player where oil is.

## In Control's fire dragon replaced (OGB-6, the audit's F8)

**OGC46. Field flights on at the built draw; the pack's In Control change waits for mic-rtsim.**
`DragonConfig`'s `fieldFlights` defaults true, and so does the engine's `OilTunables::flags` (field
flights and lair placement), at the draw OGC24 built (a roll a 10 seconds per dimension, 200 alive
per dimension, 8 per field). GameTests still start with every oil switch off (`DragonConfig.Test`).
The pack side is built on the pack repo's branch `oilgate-f8` (bb48c52): `config/incontrol/spawner.json`
loses its `iceandfire:fire_dragon` entry and both `onjoin` rules in `spawn.json` hold only
`minecraft:blaze` (OG-12 (a): the blazes stay desert-keyed), the files `neoforge/iceandfire/pack-proposal/`
held, with `index.toml` and `pack.toml` taking the two files' new hashes by hand (`packwiz refresh` in a
fresh pack worktree also drops the raw jars that worktree lacks). It landed on `master` and was reverted
there (33435e8): mic-rtsim is not in the pack's `mods/` yet, so on master it left the pack with neither
danger. It merges in the same pack commit that adds mic-rtsim and its Ice and Fire plugin to `mods/`.
Until then OGC66 keeps the switch-over whole on any server that runs the plugin. OG-10's "at least as
dangerous as In Control" no longer gates anything (OILGATE.md §4.8).

**OGC47. OG-1's exemption stays for everything but a fire dragon under field flights.**
`adopt_natural_dragons` stays false: a natural or spawner dragon of another species is still left alone,
and so is a fire one while field flights are off. True discards every one, as R18 had it.

**OGC66. While field flights run, a natural or spawner fire dragon is discarded as it joins.** The
engine's `untagged` discards it (reason 1, as R18) when `field_flights_on()` (the OIL_CONFIG switch, or
the tunables' until it comes, and a chance above zero), whatever `adopt_natural_dragons` says. So the
replacement does not hang on the pack's files: with the plugin and flights on, In Control's dragon is
gone even while the pack still has its rule; with the flights off (`fieldFlights` false, or every
GameTest), In Control's dragon stays the danger. Never both and never neither, on the pack's master
today or with `oilgate-f8` merged, except a server that turns flights off after merging `oilgate-f8`,
which has neither (the config comment says so). Rejected: tying it to `adopt_natural_dragons` alone
(a server that turned flights off would then lose In Control's dragon too), and discarding every
species (Ice and Fire spawns none naturally, and only the fire dragon is replaced). OG-10's arm A sends
flights off, so it still measures In Control's dragon; arm B's rules have no fire dragon to discard.
A discarded dragon exists from its join until the engine's next step answers (a tick or two, the same
as R18's discard was); once `oilgate-f8` merges, In Control stops spawning it at all.

**OGC48. OG-10's harness stays, as a measure.** `OilDangerGameTests` (`-Pog10`) is kept for measuring
the field flights. Its arm A reads In Control's rules from `packMods`' config (the smoke server's
copy), which keeps the fire dragon; it sends field flights off, so OGC66 leaves that dragon alone. If
the copy is synced from the pack after `oilgate-f8` merges, arm A has no fire dragon. `-Pog10Hour=<day time>` starts both arms at another hour (default 1000, morning).

**The night run** the audit's F8 asked for (`-Pog10 -Pog10Hour=14000`, one native sample per arm,
2026-10-21, arm A with In Control's fire dragon):

| Site, arm | Dragon damage/min | First dragon hit | Distinct dragons hitting/h | Targeting or hitting/h |
|---|---|---|---|---|
| 1, A | 104.7 | 1,854 | 60 | 80 |
| 1, B | 353.3 | 334 | 120 | 220 |
| 2, A | 0 | none | 0 | 120 |
| 2, B | 0 | none | 0 | 0 |
| 3, A | 235.3 | 692 | 120 | 160 |
| 3, B | 0 | none | 0 | 0 |

By night the field flights beat In Control's dragon at site 1 on every measure: In Control's dragons
hit later and less by night (OGC35's morning runs had them at 260.7 and 334.0 a minute, first hit 676
and 563), and the drawn visitors do not. Site 2 is safe in both arms, and site 3 (no oil) has no
dragon in B, as designed. One sample, so this shows the direction, not a median.
In that run the suite's later lair test (`aLairsDragonIsAdoptedAndItsHoardRobbed`) read its dragon's
DRAGON_MATERIALIZED before it was sent and crashed the server; it now waits for the record. Without
`-Pog10` all 20 iceandfire tests passed.

## Lairs near oil (OG-7)

**OGC20. One lair placed per field.** When a player is near a field with no fire lair within
`lair_reach` 1,500 blocks, the engine sends one PLACE_LAIR (a fire roost, radius 100 around the oil
chunk nearest the player); the plugin finds a free site on its first load. The check runs once per
field and is saved, so a restart never asks again.

**OGC21. A new fire lair surveys its oil.** Every fire lair the engine learns of sends one OIL_SURVEY
of radius 1,024 around itself, so the index knows the fields its dragons can reach before those
chunks generate.

**OGC22. An oil ask in an unknown dimension is dropped.** The native host writes PLACE_LAIR and
OIL_SURVEY with Java's dimension id; one for a dimension Java never named is logged and dropped (an
ask, not a body). The synthetic plugin answers both as the real one must.

**OGC27. Oil alone wakes the native dragons.** The JNI host used to run the dragons only with a
dragon or a lair in the world; a world whose first fire dragon will be a field flight has neither.
Oil known now counts too (`Dragons::active`).

## Sensing and raids (OG-8)

**OGC16. A dragon senses each district once per day.** A dragon arriving at a field senses the
districts within `sense_reach` 400 blocks of it. The report's severity is (1 + 2 if the district
has an oil sample + 1 if its fuel stock is at least `fuel_signal_mb` 1,000 + 1 if it pollutes)
severity units, at most 255.
The encroachers are the district's group actor and the players whose homes lie within the home
radius of it. Since the cheese audit: a district is near by any of its chunks (OGC50), the field
watches its own ground once a day (OGC52, OGC53), and the district's recent builders are named too
(OGC55).

**OGC17. A grudge raids the district through the threat interface.** A dragon whose sentiment toward
a district falls below `grudge_below` raids it as a District target: watched, a real fight there;
unwatched, resolved by the industry engine (a drinking raid when the dragon is low). Since OGC56 a
watched district is resolved by the industry engine too, and the real fight comes on top.

## Saves and the bench

**OGC23. New save magic, no migration.** The dragon pages are RTD4 and the dragons' meta RTN4 (RTD3 and RTN3 were the money sink's tribute, merged first), with
the reserve, the visitor field and the oil index; nothing reads the old format (no backward
compatibility before release). Now RTD5 and RTN5 (OGC58).

**OGC26. The `dragons_oil` scale sweep.** `bench scale --only dragons_oil`: the dragons sweep with a
3 x 3 field per 8 villages known at t 0, under the same contracts (CPU per unit flat, heap slope
1.1).

## The plugin side (`neoforge/iceandfire`, `neoforge/dieselgenerators`, core)

**OGC28. The switches and OIL_CONFIG.** The plugin's server config (`DragonConfig`,
`mic_rtsim_iceandfire-server.toml`) has `fieldFlights` (true since OGB-6, OGC46) and `placeLairs` (true); OIL_CONFIG
carries them to the engine at each session start. On a GameTest server every side effect of the gate
starts off (no flight, no lair placed, no chunk sampled, no survey answered) and a test switches on
what it needs: the tests share one engine and a reused world.

**OGC29. A breath is a fire event.** One point per `DragonFireEvent`, and per ground charge Ice and
Fire shoots without an event; a charge inside a paid stream is not paid twice. Ice and Fire fires an
event every tick of a stream, so a stream costs about 35 to 41 points (measured below) and a full
reserve of 600 (OGB-2) about fifteen streams, three fights of five. The other reading, a point a burst, is the test-only
`breathPerBurst` switch (OG-10's arm C); it makes a reserve last about 40 times as long. See For Ben 2.

**OGC30. A dry dragon fights with bite and claws.** At zero the plugin cancels its fire events and
charges, stops a stream it began, and swaps its fire attacks: the air attack becomes the tackle, the
ground fire a bite (`IafDragons.withoutFire`). Its reduced strength is the engine's (OGC13). In
GameTests only, the melee nudge also sets the dragon down and turns a shake into a bite: a shake hurts
its prey only as a passenger, and a guarded mock player never rides.

**OGC31. The oil sampler's budget.** New chunks and surveys share 4 samples a tick (`samplesPerTick`)
and 2 ms; one sample always runs, so a tick with work pays at least one source call. Only chunks with
oil are sent, up to 1,024 to an OIL_CHUNKS record, at the end of the tick; a chunk is sampled at most
once a session. A Diesel sample first looks at 504 noise biomes, not 12,800 (OGC45).

**OGC32. Oil sources and the lair placer.** Oil mods answer base oil per chunk through
`RtsimApi.registerOilSource` (`OilSource`, `OilSources.forLevel`); Create: Diesel Generators' plugin
registers its `getBaseOilAmount` as source 2, and the Ice and Fire plugin never names the mod.
PLACE_LAIR goes through vanilla's `/place structure` path with Ice and Fire's fire roost (every biome
allowed), nearest the field first, on the first tick a site is free: its 3 x 3 chunks loaded, no
lair structure there, no player within 48 blocks; 2 sites tried and 64 chunks looked at a tick (all
PLACEHOLDER). Ice and Fire's own chance and spawn distance still decide each chunk. The fields asked
for and the roosts placed are saved with the overworld, so a replayed request never places a second
roost. The plugin writes the structure start and references itself (`/place` leaves none), so `Lairs`
finds the roost and the engine adopts the dragon the roost spawns as it builds (DRAGON_UNTAGGED, kind
STRUCTURE).

**OGC33. MATERIALIZE carries a field flight's state** in its flags, so the body knows it is a visitor
(it targets the nearest player at the field and is discarded, not captured, when let go).

**OGC41. A district detected in loaded chunks tells the engine its chunks** (core, `Districts.java`).
A chunk a district gains while loaded never loads for it, so its IND_CHUNK_LOADED never went, and the
engine, which reconciles only into chunks it knows loaded, held every reconcile for a new factory until
its chunks reloaded: a raid resolved and broke nothing. Detection now sends it for each chunk gained
while loaded. This changes core behaviour outside the gate; core 62 of 62 and create 146 of 146 pass
with it.

**OGC43. A survey samples only chunks in memory; the rest when they load.** A sample of a chunk not in
memory reads the biome source: on a real overworld Create: Diesel Generators then costs 79 ms a chunk
(mean of 8, max 121 ms), against 3.2 ms in memory (the dieselgenerators GameTest), and a new lair's
survey covers up to 13,000 chunks, so one survey would have added about 80 ms to every tick for over
15 minutes. Now a survey samples a chunk in memory at once (`getChunkNow`, which never blocks) and
notes any other; a noted chunk is sampled the first time it loads this session, under the same budget.
The survey still covers its whole radius, as players reach it. A chunk saved before this session and
never loaded again stays unknown, as it would without a survey.

**OGC44. Ben's reserve, and the tests that keep the first one.** `OilTunables::default` is capacity 600
and `refuel_below` 240 (OGB-2): a low dragon drinks every 360 points, 1,080,000 ticks (45 days) of the
slow drain if it never fights. The oilgate integration tests (`tests/oilgate.rs`, `quick_oil`) keep
100 and 40 so their schedules stay short, and the closed-form unit test pins 100; the tests that read
the capacity from the tunables (the protocol test's "a drawn dragon arrives full") run on 600. Every
other number is unchanged.

**OGC45. The Diesel source's first look (OGB-5).** `CdgOilSource.baseOil` asks `CdgFirstLook` before
`getBaseOilAmount`. `BiomeManager.getBiome` gives a block the biome of one of the 8 noise cells around
it (x - 2 >> 2 or the next, the same in y and z), so the cells x `(minX - 2) >> 2` to
`((maxX - 2) >> 2) + 1`, likewise in z and for y 60 to 109, hold every biome CDG's 12,800 points can
read: 504 lookups through the level's own `getNoiseBiome`, the same values `getBiome` reads in memory
or not. No oil biome (`#createdieselgenerators:oil_biomes`, the pack's `#c:is_desert`) among them: the
answer is 0. Any other chunk gets the exact call, so the look never changes an answer; it only skips
work. It stands aside, and every chunk gets the exact call, when its answer could differ:
- CDG's config lets a chunk without an oil biome hold oil (normal chunks on and their multiplier
  above 0, or a threshold of 0 or less). The pack's config has the normal multiplier at 0.
- KubeJS is loaded and a script listens to CDG's oil-chunk event (`CDGKubeJSPlugin.OIL_CHUNKS`, read
  by reflection on every call, since a reload can add one). The pack has no such script. If the field
  cannot be read, the look stands aside for the session and says so once in the log.
- Deep Time is installed (`OilSources.deepTimeInstalled`): its mixin answers at the head of
  `getBaseOilAmount` in its levels. This is coarser than needed, since a Deep Time install's other
  dimensions could take the look; asking `isDeepTimeLevel` per level is the finer form.

The dieselgenerators GameTest `theFirstLookAgreesWithGetBaseOilAmount` checks it two ways. On vanilla's
overworld multi-noise biomes for the level's seed (the GameTest world is one flat biome), at 65 chunks
around an oil biome's edge and 25 at random, the cells must hold every biome CDG's points read through
`BiomeManager`, and every chunk with an oil biome for CDG must pass the look. On the level, under the
pack's rule (CDG's normal multiplier set to 0 for the check and put back), the source must answer what
`getBaseOilAmount` does, chunk for chunk, and under CDG's own defaults the look must stand aside.
First run (stub engine, 2026-10-21): 90 noise chunks, 55 with an oil biome for CDG, 35 skipped by the
look, none passed without one; the look took 2.4 ms a chunk (with its early exit) against 128.6 ms for
CDG's 12,800 points on the same uncached biomes. On the level 57 chunks agreed with `getBaseOilAmount`,
but none took the skip: the flat preset's one biome is plains, which CDG's own tag (the GameTest
server's; the pack replaces it with `#c:is_desert`) counts as oil, so the skip is checked only on the
noise biomes. Under CDG's defaults (normal multiplier 1.3) the look stood aside.

## GameTests (OGC34, OGC36 to OGC40, OGC42)

**OGC34. Guarded players.** `Guard` logs every blow on a GameTest player and cancels it, and stops it
riding. Dragons a test lets go are discarded; test sites sit on a far-apart grid.

**OGC36. The OG-5 test lets its chunks go after 260 ticks,** so the dragon is captured and refuels out
of view.

**OGC37. The roof gate test (OG-11, core, native only).** One district: a chest and two furnaces, one
under a stone roof. A flyer of strength 8, then a siege of 20, sent as INDUSTRY_THREAT orders.

**OGC38. A GameTest server catches its clock up.** A GameTest server makes fresh level data (game time
0) but keeps the world's pages, so a resumed engine stood ahead of the game and its wheel never turned.
On GameTest servers only, the Ice and Fire plugin moves the game time up to the engine's clock before
the first test (`GameTestClock`). The same stall made 7 glass exit tests fail on a reused create world;
fresh worlds pass (OGC42).

**OGC39. A mock player's spawn grace ends.** A mock player is never ticked, so its 60-tick spawn grace
never ran out and `ServerPlayer.hurt` refused every bite, claw and fire before the damage event: no blow
on a GameTest player was ever logged (earlier OG-10 tables undercounted). `Guard` ends it by
reflection.

**OGC40. Read-only diagnostic queries** for the tests: `dragons_oil` (26 slots: the gate's counters, a
dragon's reserve, the switches, the clock and `flight_probe`), `industry_threat_probe` (25 slots) and
their JNI exports `nDragonsOil` and `nIndustryThreatProbe`. Between steps; they change nothing.

**OGC42. Verification on fresh worlds.** Every suite that counts here ran on a fresh GameTest world
(the old ones moved aside or deleted).

## OG-10's danger A/B (OGC35)

**OGC35. The harness** (`OilDangerGameTests`, native only, `-Pog10`). Three far patches of the flat
world dressed as desert (desert biome, a sandstone floor at y 64 out to 104 blocks): site 1 on the
surface over oil, site 2 over oil in a sealed pocket at y 10 (below `flight_min_y` 15), site 3 on the
surface with no oil. A probe is a mock player (In Control spawns around it and hostiles target it) with
a still, invisible husk at its feet for the projectiles and area fire that only find entities in the
level's sections; both guarded. Damage is summed as vanilla takes it (a hit within 10 ticks of the last
counts only its excess), from blows with an attacker. Each arm runs 3,600 ticks from morning (fire
dragons sleep at night). Arm A: the pack's own `spawner.json` and `spawn.json`, the gate off. Arm B:
those files without their fire dragon entries, field flights on. In Control is reloaded between arms by
reflection (its reload command's calls), with a watchdog. Ice and Fire's player targeting is emulated
for the mock player in both arms. Arm C (B with a point per burst) is test-only; the sweep between B and
C clears the bodies and the engine's puppets, but B's visitors stay in the engine's sim and come back at
C's start, so C is not a clean measurement.

**Results, 2026-10-21, two native samples per arm** (GameTest servers tick unthrottled; times are game
ticks):

| Site, arm | Dragon damage/min | First dragon hit | Distinct dragons hitting/h | Targeting or hitting/h |
|---|---|---|---|---|
| 1, A | 260.7, 334.0 | 676, 563 | 240, 360 | 440, 560 |
| 1, B | 355.7, 350.3 | 1,023, 816 | 120, 120 | 180, 200 |
| 2, A | 0, 0 | none | 0, 0 | 340, 200 |
| 2, B | 0, 0 | none | 0, 0 | 0, 0 |
| 3, A | 326.7, 293.7 | 43, 368 | 200, 260 | n/a, 360 |
| 3, B | 0, 0 | none | 0, 0 | n/a, 0 |

First hit by any hostile at site 1: A 165 and 291, B 185 and 184.

**Verdict: B is not at least A at site 1.** Damage per minute holds (+36% and +5%), but the first
dragon hit comes 40 to 50% later and B brings a third to a half of A's distinct dragons. Site 2 passes
trivially (nothing reaches the sealed pocket in either arm); site 3 falls, as designed. OILGATE.md §4.8
asks for medians over several runs by day and night; these are two runs by day, so the verdict is
early, but both runs fail the same two measures. Field flights stayed off (For Ben 1, now OGB-1 and the cheese audit)
until OGB-6 retired the pass condition and turned them on (OGC46).

**Fire events per stream** (a 30-day fire dragon, native): ground fire came in 14 bursts, mostly 33 to
38 events over 40 to 44 ticks, with 2 charges inside them; scorch in 12 bursts of 41 events over 41
ticks; hover fire none. The dry fight: 15 points went in 31 ticks (15 events); for 300 ticks after,
no fire got through and none hit the player; the first bite or claw came 301 ticks after the reserve
ran out; the engine got 2 DRAGON_BREATHS records for the 15 breaths.

## The audit's fixes: sensing, builders and watched raids (F2, F3, F7, F9)

The oil cheese audit (`docs/oilgate-cheese-audit.md` §4) named the smallest set of fixes. These four
are engine code in `rtsim-ecology` and `rtsim-industry`; F1, F4 and F5 are built apart (OGC60 and
up).

**OGC50. The chunk test (F3).** `Infrastructure::districts_reaching(frame, x, z, r)`: the districts
with any chunk whose centre lies within `r` blocks of (x, z), each sited at its chunk centre nearest
(x, z) (a tie, the lower chunk), in id order. `sense` uses it instead of the centre test
(`districts_near`, kept as the trait's default and for callers that want centres). The industry
link's `DistrictIndex` keeps a cell map of every district chunk beside the centres' map, and
rebuilds both when the industry engine's `districts_rev` moves: a district declared, its chunks
changed, or removed. Before, it rebuilt only when the number of districts changed, so a district
that grew was looked up where it used to be. `districts_rev` is drawn from one counter for the
whole process, starting at 1 and never saved, so an engine restored from a save never shares a
value with an index built against another engine. Closes B2 and C3's joined districts. Unit tests
(`infra.rs`): a well joined to a base 3,000 blocks away is sensed at the well's chunk; the index sees
a district that grew.

**OGC51. Raids find districts by chunk too.** Home raids (`raid_home`, within `HOME_RADIUS` 512
blocks), District raids and `home_districts` use the chunk test as well. Joining a far well to a
base no longer carries the base's centre out of reach of its owner's home raids, and a watched
fight's site is the district's chunk nearest the home or the drink spot, where the dragon
materializes.

**OGC52. The fields' daily watch, one event for the world (F2).** One `Ev::OilWatch`, booked when
the gate opens (OGC12) and then once a game day. It asks the infrastructure, district by district
and chunk by chunk (`Infrastructure::districts_where`), for the first chunk centre within
`sense_reach` 400 blocks of any oil chunk (`OilIndex::near`), and each such district is sensed
through `sense_site`, as at a drink spot, with the severity and the once-a-day rule of OGC16. One
event a day for every field, not one a field: the cost is one pass over the districts' chunks a day.
The audit's other trigger, at once on a district's first deposit sample, is not built: the ecology
engine sees no event for a deposit sample (it reads `oil_sample` as a signal), and the daily watch
reaches a drilling district within a day. The event is saved (tag 16), so the dragons' format moves
(OGC58). Unit test (`oilgate.rs`): a district drilling 1,000 blocks along a field from the drink
spot is sensed within a day.

**OGC53. Who watches.** Of the fire lairs within `lair_reach` 1,500 blocks of the oil chunk the
district is nearest to (measured as OG-7's `check_lair` measures), the nearest with a live fire
dragon in it (a tie, the lower lair); that lair's first such occupant senses the district. A vacant
lair is passed over for the next in reach. No fire lair in reach, nobody senses it, until OG-7
places one when a player comes near. The watch shares the dragon's once-a-day rule: a district its
watcher sensed at a drink spot that day is not sensed again by the watch, and the other way round.
No flight is booked; the dragon knows its ground.

**OGC54. A district keeps its builders (F7).** The industry engine now keeps the player of each
`IND_EDIT` on the district: `District::editors`, the last 8 (`EDITORS_KEPT`) distinct players with
the tick of each one's latest edit, oldest first. Placed, Broken and Configured count; Command
(`/rtsim district rename | merge | split | exclude`) does not, since an admin's command is not
building. Saved with the district, so its page now starts with a magic, "IDS2" (OGC58).

**OGC55. The builders are named.** `sense` names, besides the district and the players whose home
is within `HOME_RADIUS` 512 blocks of the district's site (P26), the builders whose latest edit
there falls within `grudge_window_days` (PLACEHOLDER 10) before the sensing, wherever they live.
Each is registered as a player actor if new; a player named both ways gets one report. A remote
well's builder then carries the grudge home: hunted at home and on sight. Unit test
(`oilgate.rs`): a builder who lives far from a drilling district gets the grudge; a builder from
before the window and a bystander do not.

**OGC56. A watched raid books its bounded damage at once (F9, P28 as Ben decided it).** A dragon's
raid on a watched district now also resolves through the threat interface, blocks and theft, under
the same caps, cooldown and observed floor as an unwatched one, and the dragon materializes there
for the real fight as before. Industry: `Engine::threat_in_sight`; the attack's note says
"(watched, a real fight too)". `threat_drinking` still skips a watched district for its other
callers (the industry's own pressure threats). Ecology: `Infrastructure::raid_in_sight`. Ice and
Fire griefing stays off, so the real dragon breaks nothing; the bounded damage is the raid's.

The audit wrote "after the real fight"; this resolves it at once, when the raid is decided, before
the dragon materializes. Why:
- It is deterministic, from the same inputs as an unwatched raid, so watched and unwatched agree
  exactly (P28's own words) and the test can say so.
- A real fight ends many ways: the dragon killed, driven off or left alone, the player gone, the
  server stopped. Booking after it would need a saved pending raid and a hook for each ending.
- The outcome already weighs the turrets and the floor of comparable observed fights; its passes
  stand for the first strikes as the dragon arrives.

Unit test (`dragons.rs`): a raid on a watched district books the same outcome and destroyed blocks
as the same raid on it unwatched, and the dragon materializes.

**OGC57. In person, the dragon's own costs are the real fight's.** When the dragon comes in person
(a home raid on the hated player, always; a District raid or a refuel at the tanks, when a player
was last seen within two view radii, 320 blocks), the raid neither burns its oil nor wounds it: the
real breaths cost points (DRAGON_BREATHS) and the real blows wound it (DRAGON_HURT). Its theft,
drink, ledger (eaten, burnt, hoarded), dry count and hunt record count as for an unwatched raid. A
watched district with nobody near is raided through `raid_in_sight` too (the industry engine would
skip it otherwise); no fight follows, so it burns and wounds as any abstract raid. At the tanks a
watched district still draws a low dragon once a day only (the day it is first sensed); that day it
now drinks as it arrives and comes with what it drank. Tests (`oilgate.rs`, `review3.rs`): the
watched tank, a watched district with nobody near, and a home raid that eats at both districts.

**OGC58. Saves, no migration.** The dragon pages are RTD5 and the dragons' meta RTN5 (the daily
watch's event); an industry district page starts with "IDS2" (its builders). Nothing reads the old
formats (no backward compatibility before release). `rtsim-view`'s fixtures are regenerated from
fresh GameTest runs.

## The seam for the underground half (OG-9b to OG-9d, not built)

Nothing of the deep layer exists. What it will hang on:
- **Which chunks:** `OilIndex::add_chunk` returns whether the chunk is new to the index; the one
  caller, `Dragons::oil_chunks` (`oil.rs`), marks the spot where a new oil chunk would queue the deep
  layer's placement. Every oil chunk passes there once, whatever its source (Deep Time, CDG, a
  survey, a deposit sample).
- **The command:** 0x8507 is reserved in `boundary.rs` and docs/boundary.md §6.4 for "place the deep
  layer on this oil chunk's first load"; nothing sends it, and the plugin does not handle it.
- **The y 15 line:** field flights already roll only for players at or above `flight_min_y` 15, the
  height below which the deep layer takes over (OILGATE.md §4.6).
- **OG-9e** (DECISIONS.md, decided while this was built) makes the deep layer a Death Worm oil nest,
  placed on the first load of oil chunks and asleep until a pumpjack drills, under a fifth threat
  class `Burrowing`. The seams above fit it: the new-chunk return is where a nest is queued, 0x8507
  is its placement, and OIL_CHUNKS source 3 (a deposit sample: defined, sent by nothing) is one way
  a drilled chunk could reach the engine. `ThreatClass` has four classes; `Burrowing` is not added.

## The audit's fixes: seeds and the survey (F1, F4, F5)

docs/oilgate-cheese-audit.md §4's F1, F4 and F5. F4 builds OGB-7 (Ben, 2026-10-21); F1 and F5 are
reversible code choices.

**OGC60. A pumpjack hole seeds its own district, whoever set it down (F1).** DS-10 seeds a district
only where a player placed a machine; the pumpjack hole is the one exception, since a hole is the only
way to pump crude. `IndustrySeeds.record` (core) writes a seed with no player: it adds the block
entity to its chunk's seeds when `Districts.seed` calls it a seed and asks for a rescan of the chunk.
The dieselgenerators plugin calls it for every pumpjack hole a loading chunk holds (`HoleSeeds`), and
from `PumpjackHoleSeedMixin` at the hole's first server tick, retrying each tick until the industry
module answers, so a hole a contraption, a Schematicannon, a Deployer, a ship or a command sets down
is seen at once. The plugin's `seed()` answers true for the hole; a pack can still tag it
`#mic_rtsim:district_not_seed`. The rest of a dropped-off base joins by connection, as any machine
does. Not widened: no other block seeds without a player (the wider forms would amend DS-10 and are
Ben's). GameTest `aPumpjackHoleSeedsWithNoPlayer` (dieselgenerators, on the stub): a hole set down
with `level.setBlock` and no player is in its chunk's seeds, and a district forms around it.

**OGC61. A partly loaded district is surveyed where it is loaded (F5).** A whole survey still needs
every district chunk loaded: the walk and the perimeter cut need the whole area, and only a whole
survey sends IND_SURVEY (the defence vector stays the last whole survey's). A district with some
chunks loaded gets a partial survey: the block entities of its loaded chunks only (the machines, the
roofs in reach and the boundary inventories), sent as IND_SURVEY_BLOCKS flagged partial, with no
IND_SURVEY. It is due after the district's revision changes, when a loaded chunk not yet surveyed at
this revision appears (looked at only when the district's loaded count changes, not every tick), when
something asks for a survey, and once a survey period, as a whole one is. A result read while the
district changed is dropped. `Threats.partialSurveys()` counts them. GameTest
`aPartlyLoadedDistrictIsSurveyedWhereItIsLoaded` (core, either engine).

**OGC62. The engine keeps each chunk's rows until that chunk is surveyed again (F5).** Every chunk a
survey reads gets part 0 of IND_SURVEY_BLOCKS, empty when it has no rows; part 0 replaces that chunk's
rows and its later parts add to them. A partial survey's part 0 keeps the chunk's wall faces, which
belong to the last whole survey's cut. The engine drops rows outside the district's chunks at each
IND_SURVEY and each part (it keeps them all while it knows no chunks); an IND_SURVEY at a new revision
no longer clears every row. A raid on a partly loaded district draws on every chunk's last rows. The
fake Java's announce sends a part 0 for every district chunk. Unit test
`a_partial_survey_keeps_the_other_chunks_rows` (threat.rs).

**OGC63. IND_SURVEY_BLOCKS is 56 + 24n, one chunk a record.** 24 u64 district, 32 u32 revision,
36 u32 part (within the chunk), 40 i32 chunkX, 44 i32 chunkZ, 48 u32 flags (1 partial), 52 u32
reserved, 56 n × Block24; Block24's flags gain 16 roof. No compatibility work (unreleased): delete
GameTest worlds after this lands.

**OGC64. A roof in reach is a block a flyer breaks (F4, OGB-7).** For a machine without sky access the
survey finds its column's top block that keeps the sky out: the block under the column's lowest sky
light source, the light engine's own count, which changes with the block in the same tick. When that
block lies within `roofReachBlocks` (ThreatConfig, 8, PLACEHOLDER as OGB-7 says) above the machine, it
is listed with flags roofless and roof. The rows go machines, then roofs, then boundary inventories,
each block once (a roof that is itself a machine or an inventory is not listed twice, and two machines
under one column's roof list it once). "Roofed" keeps its meaning (the block above the machine has no
sky access, so a glass roof is no roof); it is now read from the same sky light sources rather than
the light engine's brightness, so a survey need not wait for lighting. Only the top block is listed:
a roof two blocks thick within reach takes two raids (the second survey lists the lower block).
Unit test `a_flyer_takes_a_roof_in_reach_before_the_machine_under_it` (threat.rs); GameTests
`theDefenceSurveyMeasuresWallsRoofsAndDark` (core, either engine: a stone three up is a roof row,
cover ten up shields) and `aRoofShieldsAMachineFromAFlyerNotFromASiege` (core, native: a dragon breaks
a shed's roof and leaves the machines under it and under deep cover; the next survey finds the shed's
machine open; a siege still breaks the deep-covered one). That GameTest used vanilla furnaces, which no
longer seed a district (DS-10), so it now uses the test machine.

**OGC65. `roofedMachines` counts shielded machines only.** IND_SURVEY's `roofedMachines` (and
`lastSurvey`'s index 4) counts the machines whose cover is beyond reach. A machine under a roof in reach
counts as open in OGC1's roof pass: a shed does not cut a flyer's blocks, it only keeps the machine off
the list until its roof goes. `lastSurvey` index 20 counts the roofs in reach (diagnostics; no new
IND_SURVEY field).


## For Ben (answered 2026-10-21)

Recommended option first in each. Ben's answers are DECISIONS.md OGB-1 to OGB-5; the new questions
are in docs/oilgate-cheese-audit.md §5.

1. **Superseded (OGB-1); the switch is built (OGB-6, OGC46, OGC66; the pack side waits on its branch `oilgate-f8`).** Ben judged In Control's dragons possibly overkill and set his own bar: a
   real oil pipeline must be infeasible and cheesing it hard for a semi-casual player.
   docs/oilgate-cheese-audit.md judges the gate against that bar and asks the follow-up. The question
   as it stood: **OG-10 failed at site 1, so field flights stay off.** B matched A's damage per minute but its
   first dragon hit came 40 to 50% later and it brought a third to a half of A's distinct dragons
   (OGC35's table). (a) Keep `fieldFlights` false and In Control's fire dragon rules as they are; raise
   the field flights' draw (`flight_chance_milli` 100 to 250 and `flight_cap_field` 8 to 12, both
   PLACEHOLDER) and measure again as §4.8 asks (medians over at least 5 runs per site, by day and by
   night); switch when B is at least A at sites 1 and 2. (b) Judge danger by damage per minute alone,
   which B already meets, and switch now (remove the fire dragon from In Control's `spawner.json` and
   both `spawn.json` rules; the proposal files are in `neoforge/iceandfire/pack-proposal/`). (c) Turn
   flights on alongside In Control (danger roughly doubles at oil sites until the rule goes).
2. **Answered (a), built (OGB-2, OGC44).** **The reserve's numbers (OGC5, OGC18, OGC29) are placeholders.** A stream of fire costs 35 to 41
   points at a point a fire event, so the reserve of 100 lasts about three streams. (a) Keep a point a
   fire event and size the reserve from fights: if a fight is about five streams, three fights are
   about 600 points (capacity 600, refuel below 240). (b) Charge a point a burst (the arm C switch) and
   keep 100: about 100 streams, so a dragon is almost never dry. (c) Keep 100 at a point a fire event:
   a dragon is dry after about three streams and finishes most fights with bite and claws.
3. **Answered (a) (OGB-3; at the reserve of 600 the room is up to 600,000 mB).** **Drinking ignores the theft cap (OGC8).** (a) Keep: a dry dragon may take up to 100,000 mB (its
   whole room) from one raid; the tank is the lure. (b) Hold drinking to the item theft cap (10%).
4. **Ben skipped, taken as recommended: (a) (OGB-4).** **A visitor's lost body ends its visit (OGC11).** (a) Keep: after a restart the field flights
   draw again from fresh rolls. (b) Keep visitors through a session end as wanderers.
5. **Answered (a), built (OGB-5, OGC45).** **Sampling new chunks costs about 3 ms a tick while players explore (OGC31).** Diesel Generators'
   `getBaseOilAmount` makes 12,800 biome lookups a chunk (3.2 ms mean in memory, 14.8 ms at most), and
   one sample always runs in a tick with work. (a) Give the Diesel source a cheap first look (skip a
   chunk none of whose biomes can hold oil under the pack's rule) and keep the exact call for the rest;
   check it against `getBaseOilAmount` in the dieselgenerators GameTest. (b) Keep it as it is.
