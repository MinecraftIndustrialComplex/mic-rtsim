# Choices: the oil gate, dragon half (OILGATE.md)

Ben approved OG-1 to OG-8 and OG-10 to OG-13 as recommended in OILGATE.md §8 (2026-10-21). This
file logs the choices made while building them. Each can be undone; the records in
docs/boundary.md §6 (OIL_CHUNKS, DRAGON_BREATHS, OIL_CONFIG, PLACE_LAIR, OIL_SURVEY, DRAGON_MATERIALIZE
flag 16 and its oil field) are what other code depends on. The underground half (OG-9, OG-9b to
OG-9d) is not built; its seam is described at the end.

The engine side is `crates/rtsim-ecology/src/oil.rs` (the index, field flights, refuelling, lairs,
sensing) with hooks in `engine.rs`, `dragon.rs` and `persist.rs`; the drinking raid and the roof
gate are in `crates/rtsim-industry` (`threat_drinking`, `threat::roof_pass`). Every number is a
PLACEHOLDER until OG-10's measurement and the fire-event count set it.

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

**OGC5. The reserve's numbers.** Capacity 100 points; one point lost per 3,000 ticks (the pack's
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
woken at the earlier of hunger and the time its reserve reaches `refuel_below` (40), the latter only
when it is above 40 now; one already low waits for hunger, whose decision weighs refuelling with
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

**OGC10. Field flights are off by default; the plugin's config sends the switches.** OIL_CONFIG
carries `fieldFlights` (default false) and `placeLairs` (default true). Off until OG-10's A/B shows
the gate at least as dangerous as In Control at the oil sites; the engine's own default
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

**OGC23. New save magic, no migration.** The dragon pages are RTD3 and the dragons' meta RTN3, with
the reserve, the visitor field and the oil index; nothing reads the old format (no backward
compatibility before release).

**OGC26. The `dragons_oil` scale sweep.** `bench scale --only dragons_oil`: the dragons sweep with a
3 x 3 field per 8 villages known at t 0, under the same contracts (CPU per unit flat, heap slope
1.1).

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

## For Ben

Recommended option first in each.

1. **Field flights stay off until OG-10 passes.** (a) Keep `fieldFlights` false in the shipped
   config; flip it, then remove the fire dragon from In Control's `spawner.json` and both `spawn.json`
   rules (the proposal files in `neoforge/iceandfire/pack-proposal/`), once B is at least A at sites 1
   and 2. (b) Turn them on now alongside In Control (danger doubles at oil sites until the rule goes).
2. **The reserve's numbers (OGC5, OGC18) are placeholders.** (a) Set the capacity from the measured
   fire events per stream so a full reserve lasts 3 fights (OILGATE.md §4.3). (b) Keep 100.
3. **Drinking ignores the theft cap (OGC8).** (a) Keep: a dry dragon may take up to 100,000 mB (its
   whole room) from one raid; the tank is the lure. (b) Hold drinking to the item theft cap (10%).
4. **A visitor's lost body ends its visit (OGC11).** (a) Keep: after a restart the field flights
   draw again from fresh rolls. (b) Keep visitors through a session end as wanderers.
