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
roof is no roof.

**OGC2. A roof stops a flyer's block damage, not its theft or its fight.** A flyer's destroyed
blocks are `floor(n * roof_pass)` and drawn only from blocks the survey flagged roofless. Theft and
the Lanchester fight do not see the roof: a dragon lands in the yard and takes what is in the tanks.

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

**OGC46. Field flights on at the built draw, and the pack's In Control keeps only its blazes.**
`DragonConfig`'s `fieldFlights` defaults true, and so does the engine's `OilTunables::flags` (field
flights and lair placement), at the draw OGC24 built (a roll a 10 seconds per dimension, 200 alive
per dimension, 8 per field). GameTests still start with every oil switch off (`DragonConfig.Test`).
In the pack repo (`master`), `config/incontrol/spawner.json` loses its `iceandfire:fire_dragon` entry
and both `onjoin` rules in `spawn.json` hold only `minecraft:blaze` (OG-12 (a): the blazes stay
desert-keyed), the files `neoforge/iceandfire/pack-proposal/` held. Both sides land together, so there
is never a pack with neither danger nor one with both. The pack's `index.toml` and `pack.toml` take
the two files' new hashes by hand: `packwiz refresh` in a fresh pack worktree also drops the raw jars
that worktree lacks. OG-10's "at least as dangerous as In Control" no longer gates anything
(OILGATE.md §4.8).

**OGC47. OG-1's exemption stays.** `adopt_natural_dragons` stays false: a natural or spawner fire
dragon is still left alone. With In Control's rule gone the pack spawns none, so the setting has
nothing to act on; true would discard any that a server's own In Control rules still make, and is the
switch if one turns up. Left alone, the untagged-dragon tests stand as they are.

**OGC48. OG-10's harness stays, as a measure.** `OilDangerGameTests` (`-Pog10`) is kept for measuring
the field flights. Its arm A reads In Control's rules from `packMods`' config (the smoke server's
copy), which keeps the fire dragon until that copy is synced from the pack; after that arm A has no
fire dragon. `-Pog10Hour=<day time>` starts both arms at another hour (default 1000, morning).

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
radius of it.

**OGC17. A grudge raids the district through the threat interface.** A dragon whose sentiment toward
a district falls below `grudge_below` raids it as a District target: watched, a real fight there;
unwatched, resolved by the industry engine (a drinking raid when the dragon is low).

## Saves and the bench

**OGC23. New save magic, no migration.** The dragon pages are RTD4 and the dragons' meta RTN4 (RTD3 and RTN3 were the money sink's tribute, merged first), with
the reserve, the visitor field and the oil index; nothing reads the old format (no backward
compatibility before release).

**OGC26. The `dragons_oil` scale sweep.** `bench scale --only dragons_oil`: the dragons sweep with a
3 x 3 field per 8 villages known at t 0, under the same contracts (CPU per unit flat, heap slope
1.1).

## The plugin side (`neoforge/iceandfire`, `neoforge/dieselgenerators`, core)

**OGC28. The switches and OIL_CONFIG.** The plugin's server config (`DragonConfig`,
`mic_rtsim_iceandfire-server.toml`) has `fieldFlights` (false) and `placeLairs` (true); OIL_CONFIG
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
early, but both runs fail the same two measures. Field flights stay off (For Ben 1, now OGB-1 and the cheese audit).

**Fire events per stream** (a 30-day fire dragon, native): ground fire came in 14 bursts, mostly 33 to
38 events over 40 to 44 ticks, with 2 charges inside them; scorch in 12 bursts of 41 events over 41
ticks; hover fire none. The dry fight: 15 points went in 31 ticks (15 events); for 300 ticks after,
no fire got through and none hit the player; the first bite or claw came 301 ticks after the reserve
ran out; the engine got 2 DRAGON_BREATHS records for the 15 breaths.

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

## For Ben (answered 2026-10-21)

Recommended option first in each. Ben's answers are DECISIONS.md OGB-1 to OGB-5; the new questions
are in docs/oilgate-cheese-audit.md §5.

1. **Superseded (OGB-1); the switch is built (OGB-6, OGC46).** Ben judged In Control's dragons possibly overkill and set his own bar: a
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
