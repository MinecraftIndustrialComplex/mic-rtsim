# Overnight choices: ships, pirates, havens and navies (2026-10-20)

Reversible choices made while Ben was asleep (D9, D10), each with its reason, numbered T#. None is
final until Ben reviews it. The work is `crates/rtsim-ships` (C5, PLAN.md §10.1), its additive
changes to `rtsim-travel` and `rtsim-world`, `rtsim-cli sim ships`, the ship sweep of `bench
scale`, and `docs/boundary.md` §7. DECISIONS.md, PLAN.md and OPEN-QUESTIONS.md are unchanged.
Branch `overnight/ships`.

**Nothing here is a fact about a ships mod.** The body mods are open (P8: Small Ships, Create
Aeronautics Structures, the Seven Seas add-on, not surveyed), and so is how ports are provided.
Every number is a TUNABLE PLACEHOLDER in `crates/rtsim-ships/src/tunables.rs` unless it cites
vanilla (sea level 63, build limit 320) or Veloren (research/veloren-rtsim-2026-10-19.md §4, as
Veloren's own numbers, not Minecraft's).

## Composition

| # | Choice | Why |
|---|---|---|
| T1 | One crate, `crates/rtsim-ships`, holds the rules and composes them with `rtsim-world` (markets, flows, the leg index, knowledge) and the industry threat interface, through their public APIs. It depends on `rtsim-ecology` only for `infra::Infrastructure` (and its `IndustryLink`), the dragons' wrapper of the industry engine. | As the dragons did (R1): PLAN.md §5.4's registry does not exist. Reusing the dragons' infrastructure trait keeps one wrapper of the industry engine instead of two. |
| T2 | Abstract body kinds (`mic_rtsim:ship/sea_merchant`, ...; crews `mic_rtsim:crew/...`), kind ranges 0x0070 to 0x007F, 0x0600 to 0x06FF and 0x8600 to 0x86FF, agent namespace byte 0x53. | P8 is open, so the plugin maps kinds to mods (P15). The ranges sit after the dragons' and collide with no reserved range (checked against boundary.md §2, §4, §6 and industry-boundary.md §1, and the open indplugins and kernel2 branches). |
| T3 | **Lanes are world edges** with a new `Edge::medium` (road, sea, sky), added by `World::add_lane` after the world is built, and kept out of the road index. | A13: merchant ships carry flows "as caravans do", so the phase 1 economy (allocation by news margin, pipelines, quotes, payments, news, conservation, raid pipelines to havens) carries ship trade unchanged. Out of the road index, no llama caravan is ever sampled on a sea lane. |
| T4 | A lane direction carries at most its ships' holds per quantum (`Edge::cap_slot_milli`: ships of the services sailing it × hold slots over the period; the smaller direction's), applied to what a destination may want in `flows.rs`; coins ride free. Roads have no cap (0). | The task: ship capacity a TUNABLE. Capping the want keeps the allocation's rules; a road's caravan still grows with its load. |
| T5 | Two edges from a market to one destination (a road and a lane) **share** the destination's want: best margin first, each up to its own (capped) want (`share_destinations` in `flows.rs`). One edge per destination, as every phase 1 world has, is left exactly as it was. | Without it each edge would be granted the whole want and the destination would receive twice what it asked for. Lanes are faster, so their transport price is lower and ships take the trade first; the road carries what the holds cannot. |
| T6 | `Outside::held` in the world's books: goods a module's rows hold (ships' holds, pirates' loot), counted as seen. `Ships::check` requires the ships' holds to equal it exactly. | A ship that becomes a row takes its consignments out of the flow; without a holder the world's sum would lose them. One map, so a second module using it would need keys (noted below). |
| T7 | A ship row docking sells its hold to the port at the quotes it left with, as far as the treasury pays (C14, as a caravan row sells), and the payment is credited to the origin port's treasury at once; coins aboard and what the treasury cannot pay for are stocked anyway. | Loops do not return to the origin soon, so a purse carried home would wait for days. Conservation holds either way; the teleported payment is the simplification. |
| T8 | Lanes added after `World::build` leave the markets' anchors and `T` bounds as built (as `World::join` does, E15). | Lanes are faster than roads, so their `d` is smaller and the bound is not exceeded on the synthetic coast; recomputing anchors is the joining code's open gap, not a new one. |

## Network

| # | Choice | Why |
|---|---|---|
| T9 | The world model is a `WaterMask` trait over coarse cells (32 blocks); tests and the CLI use `SynthCoast` (a wavy shore and an island) or `AllWater`. In play it would come from the bridge's chunk samples or Deep Time's bake: not built. | The provider is open (the task); the engine knows no terrain. |
| T10 | Candidate lanes are the Delaunay edges of the docks of one medium and frame (exact integer Bowyer and Watson). Sky lanes are straight, at most 4,000 blocks; sea lanes are A* paths over the mask (8 neighbours, no corner cutting, at most 200,000 cells expanded) pulled taut, at most 6,000 blocks and 2.5 times the straight distance. Each dock keeps its 4 shortest lanes. A sea dock snaps to the nearest water cell within 6 cells. | Delaunay gives a planar, local network in O(n²) at build time only; the degree cap bounds services per dock by its platforms (T15). |
| T11 | **Routes are Veloren's closed loops** (`world/src/civ/airship_travel.rs`, research §4; the method only, no code copied, Veloren being GPL-3.0): odd docks paired greedily by shortest path and the lanes between them doubled, an Euler circuit by Hierholzer's algorithm, cut into simple cycles where it returns to a dock still on its stack, and cycles sharing a dock merged while both stay under 12,000 blocks. | Every lane is sailed; loops are about equal and closed, so a timetable is periodic. |
| T12 | Each loop is sailed both ways, as two services (a two-stop shuttle is one). | A loop sails each lane one way; without the second service half the lane directions would have no ships, and their flows nothing to ride. |
| T13 | **Timetables**: four phases per hop (dock 1,200 ticks, Veloren's 60 s; depart and approach over 64 harbour blocks at half speed; cruise at 0.4 blocks a tick at sea, 1.0 in the sky), whole ticks, so the period is fixed. Ships per service: the period over the 4,800-tick headway (Veloren's 240 s), at most the period over a dock stay plus both harbour phases, at most 6. Ship `k` of `n` has clock offset `phase + k × P / n`, the phase drawn once per service. **The service, slot and offset are saved**; position is `(t + offset) mod P` into the timetable. | The task and Veloren's lesson: a restart never teleports a ship (tested: positions equal before and after a reload, at any time). |
| T14 | **Cruise height is forced while unloaded**: a sky ship's unloaded position is at its service's cruise height in every phase but the dock (Veloren `!5091`); a loaded ship's guidance climbs and descends through the harbour phases. Sky cruise heights 200, 216, 232 and 248 by service index (Veloren's 400 to 625 do not fit under the build limit of 320); sea ships at 63. | A ship materializing mid-air is never underground; the two services of a loop never share a height. |
| T15 | **Traffic control is in the timetable**: one platform per (service, stop), at most the dock's 4 berths (Veloren's 4 platforms), and ships of a service spaced by at least a dock stay plus both harbour phases; docks over their berths are a lint. | Veloren's traffic control (`next_pilot`) was never called; ours has no runtime part to rot. |

## Encounters and rows

| # | Choice | Why |
|---|---|---|
| T16 | **Unobserved merchant and navy ships book nothing.** A player's interest area looks up the lanes near it (a static region index of lane paths), books in the shared leg index only the voyages that overlap its window (dock stay plus every straight piece between corners and phase boundaries), and asks `LegIndex::sharing` for every ship leg in its cells. Voyages are unbooked when they end. | A15 through the one shared index (pirates and dragons can meet ships there), with no per-tick or per-voyage work for ships nobody could see: the bench's ship sweep is flat. |
| T17 | A voyage enters a player's view **once** (a crossing), whatever the area's rebookings: the engine remembers (ship, voyage, player) until the voyage ends. Interest areas follow the dragons' rule (kept while the player stays within a quarter of the radius; booked for two windows). The ships book their own interest leg per player (a third per player beside the world's and the dragons'). | Rebooking an area every window, or a plugin releasing a body a little before the engine's range ends, would otherwise materialize the same ship twice. The shared interest area is R31's open integration step. |
| T18 | A ship becomes a row when something happens to it (a player's loot, a raid, a capture): its **window** is the consignments after the previous departure on its lane direction (any ship of any service sailing it) up to its own departure, and never past the current boundary; they are cut from the goods, payment and raid pipelines and held. | SCALE.md §3 with exact totals: departures partition the flow between ships; future consignments are still in the origin's stock. Cutting the raided share too means an observed ship's fate is decided by what is seen, not twice. |

## Pirates, havens and fights

| # | Choice | Why |
|---|---|---|
| T19 | **Pirate cycle**: rest a day (bent by Sociable), choose a lane within 5,000 blocks of the haven by a weighted draw (the lane's flow volume × the share surviving its patrols raised to `2 − bold/128` × an Aggressive bend, over `1 + distance / 2,000`), sail there, hunt for three days (bent by Bold, cut by hull damage and by patrols to no less than a quarter), sail home. A lane is hunted by one haven at a time; the world refuses a second haven while raids are still on their way to the first. | C24 (threats get bored on a booked departure), C25 (temperament from the seed weights the choice), and conservation: one raid pipeline per direction must have one destination. |
| T20 | **A lane's hazard** is 20‰ per unit of hunting strength (crew × 1.0 × hull), times the square law's surviving share against patrols and escorts, capped at 400‰; it is written to both directions as the world's predation (a raid pipeline to the haven's market, delay = the lane's lag plus the pirates' sail home). | SCALE.md §3 (encounters between flows are hazards) and C24 (stolen goods go home and are sold). One market event per hazard change: cost is events, not volume. |
| T21 | **Individual raids**: a merchant seen on a hunted lane is attacked with the lane's hazard as the chance (one draw per ship and voyage), within the raid delay (400 ticks) or the time to sail half the view. Watched, the pirates materialize beside it (or, already in view, turn on it: SHIP_STEER) and the plugin plays the fight. Not watched by then, Lanchester's square law resolves it, the pirates' surviving share floored by the best real outcome for the defender among the last 16 real fights at comparable strength (within a factor of 2). | The task: "Unobserved fights are resolved with Lanchester, never better than observed", as I22 does for districts. |
| T22 | After an abstract fight: crews scale by their surviving shares (rounded up), the pirates' hull loses up to half, and they take 600‰ of each good scaled by their surviving share, or the whole hold when they keep half their strength (a capture). A beaten crew surrenders with half standing; half the standing crew joins the haven's pool (M2's crew conversion). | The square law alone fights to the last man, which leaves nobody to convert. |
| T23 | A merchant that became a row for another reason (a player's loot) and sails a hunted lane meets the pirates with the lane's hazard as the chance, in the middle of its cruise. | Its cargo left the flow, so the flow's hazard no longer reaches it. |
| T24 | Loot from individual raids is held by the pirate ship until it docks at its haven, where it is stocked with no payment. | C24; conservation through `Outside::held` (T6). |
| T25 | **Raids on player infrastructure** (C16 to C18) through the industry threat interface: at the start of a hunt, with a 250‰ chance bent by Aggressive, the pirates raid an unwatched district within 384 blocks of the hunted lane's middle, else a booked player leg crossing the lane's regions (a train leg today; a player's ship schedule would book the same way), as `ThreatSource::Bandit`, class Flying for sky pirates and GroundRanged for sea pirates. The industry engine resolves it. Its loot (whole items) is mapped to the world's goods by name and lands at the haven as players' goods coming in (`World::stock_from_players`, the line's `player_in`); goods with no world good of that name stay in the ships' ledger. | No `Pirate` source was added to the industry crate (its codecs and orders enumerate the sources). |
| T26 | Captains are knowledge notables (lifespan 200 days, heir the haven). A ship whose captain died of age takes a new one; a sunk pirate's captain dies by violence with a killing report it witnessed, so the haven inherits the grudge (C24). | Heirs inherit grudges without a new mechanism. |
| T27 | **Losses come back** (Veloren's killed captains did not): a lost merchant or navy ship relaunches two days later at its slot's next stay at its service's home dock, with the same offset; a haven rebuilds a pirate ship four days after a loss when its crew pool can man it, retrying until it can (the first pool is one crew). | The task: no missing respawns; never into the middle of a player's view. |
| T27a | (2026-10-22, amends T27) **A pirate ship the plugin reports with no crew is lost** (SHIP_FIGHT, SHIP_DAMAGED or SHIP_DEMATERIALIZED with crew 0), as an abstract fight to the last man already loses it, so its haven books the rebuild; `Ships::check` refuses an active pirate with no crew. | Kept active, a crewless pirate had no strength, so the square law made every lane worthless: it chose a lane and found none forever (287 fruitless hunts in 200 days of `sim ships`, 5,887 by day 3,000) and, never lost, was never rebuilt. What still ends the hunts in long runs is the pool: it starts at one crew and grows only by captures, so a haven rebuilds about once (EQUILIBRIUM.md §2.4.6, whose hires are slice S8). |

## Navies

| # | Choice | Why |
|---|---|---|
| T28 | **M2's recommendation: a patrol is a flow that lowers an edge's hazard.** A navy is a patrol service from a port: a depth-first walk of the lanes within 2 hops, out and back, 2 ships of 6 crew. Its presence on a lane is the share of the period its ships spend sailing it × crew × 1.5, recomputed when a navy ship is lost or relaunched. Players' escorts are a strength per lane and key (`Ships::set_escort`), for a future plugin. | Patrols are timetabled ships too, so they can be seen; their effect needs no fights, only the hazard formula. |

## Save, CLI, bench

| # | Choice | Why |
|---|---|---|
| T29 | `Ships::save` writes the whole network (docks, lane paths, services, ships with their offsets) and every table, wheel and index; only pure indexes are rebuilt on load. The state hash is the hash of the save's bytes. | The persist pattern (S1, S8): a restored engine continues with the hash of one that never stopped (tested at random moments of watched runs, and in `sim ships`). |
| T30 | The coast's ports are three times phase 1's synthetic villages (population and resources), so the sea carries visible trade. | Phase 1's villages trade a trickle at their targets. |
| T31 | The ship sweep varies ships per service (1 to 16, about 56 to 814 merchant ships) on a fixed 100-port ocean grid, and its contract is on the ships engine's own CPU per sim day (flat) and the heap (linear). | An unobserved ship costs nothing, so the cost per ship falls as one over the count, which the flat contracts' max/min bound would reject; the total staying flat is the claim. |

## What the plan documents say that this changed or found (not edited)

- PLAN.md §10.1 timetables sky ships; this work timetables sea ships the same way (T13), since
  "sea ships between villages with ports" need the same restart guarantee.
- PLAN.md §10 says patrols are open (M2); this takes OPEN-QUESTIONS.md's recommendation (T28).
- The economy's flows assumed one edge per pair of markets; lanes beside roads needed T5.

## Incomplete

Since then, on `overnight/integrate2` (docs/overnight-choices-integrate2.md): the route through the
native engine (U1), one interest area per player (U2), the server switch (U10), hazard in prices
(U9) and SHIP_CARGO (U18) are done; the rest of this list stands.

- **No plugin, no route through the native engine.** `rtsim-jni` does not carry the ship records;
  there is no Java. The fake in `synth.rs` is the reference.
- **Op ids** of CARGO_LOOTED, SHIP_SUNK and SHIP_CAPTURED are applied once (the last 4,096 kept,
  PLACEHOLDER; the core forgets by WORLD_SAVED instead, not wired here). The engine's SHIP_CARGO
  (setting a hold after a promotion) is defined but not emitted.
- **News on lanes** rides the world's caravan lattice for the lane (departures from volume), not the
  timetabled ships; and a flow's arrival is its lag (dock wait plus sail), later than the ship's
  docking.
- **Hazard does not enter prices**: the allocation's transport price ignores a lane's hazard, so
  traders do not avoid hunted lanes yet (PLAN.md §10.1's "fewer caravans take it").
- **One interest area per player** across the world, the dragons and the ships (R31's step).
- **Patrols** cover a spanning tree of the lanes within their hops, not every lane; navy ships do not
  fight pirates individually (only the hazard formula).
- **Pirates resting at their haven** book nothing, so a player at the haven does not see them.
- **Ports joining late** get a shuttle to their nearest dock; the network is never re-planned (that
  would move ships).
- **A server config switch for ships** (C25) is not there.
- **`Outside::held`** is one map; a second module holding goods would need it keyed by module.
- Not measured: a watched world's cost per ship in view; the build cost of the network at thousands
  of docks (Delaunay is O(n²)).
