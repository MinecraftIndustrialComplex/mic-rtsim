# Player ship voyages

**Status:** design for Ben's review, 2026-10-20. Nothing is built. Ben approves this design before code.

**Decided, and binding here:**
- **I17, C30:** the player's own Sable and Aeronautics ships get a voyage plugin, so they are simulated
  while unloaded. Ben: "I fully expect players to set things up s.t. they have sable ships doing
  logistics. I.E. A sailbot going between two ports to move vaults filled with items betewen them."
- **C32:** a factory can be a moving Sable ship, compiled in the ship's own frame and following it as it
  moves and freezes. Ship-to-ship ferries with both ends moving are part of this plugin. A farm of
  thousands of player-owned animals must be simulated cheaply while unloaded (Ben's server has a player
  with about 2,000 sheep on an airship, served only by a second airship ferrying its cargo).
- **G-B7:** a simulated ship, a player sailbot included, can materialize anywhere a player can see it,
  not only at docks.
- **C31:** the sim must work with Deep Time's world wrapping, through the one topology layer.
- **Also in force:** GLASSBOX.md (G-B3 recompile on edits plus a settings hash, G-B4 replay after one
  checksum window, G-B16 compiled station exchange, G-B17 farms model vanilla's rules), I7 (the unloaded
  part of a partly loaded district keeps running), I8 (reconciliation by epoch on load), I15 and I24
  (station exchange; attacks loot cargo and never cut track), A15 (booked legs in the region index),
  C17 (offline owners), D1, D3 (no bespoke blocks, items or entities), D4.

**This builds on, and does not redesign:** GLASSBOX.md §8.3 (the sailbot as a vehicle district and a
periodic edge), §4.2 (periodic edges), §8.1 (`moved = min(...)` at a call), §8.2 (lines booked once as
standing routes), and `crates/rtsim-ships` (timetables whose positions are a closed form of the clock,
pirates, havens, T25 raids on booked player legs). The topology layer is `feat/topology`
(docs/choices-topology.md, TP1 to TP19).

**Sources.** Sable 2.0.5 (`sable-neoforge-1.21.1-2.0.5.jar`), Create Aeronautics 1.3.2 with its
nested Simulated and Offroad jars (`create-aeronautics-bundled-1.21.1-1.3.2.jar`), Create Propulsion
1.1.5, cc_sable 1.3.4, CC: Tweaked 1.120.2, Drive By Wire 0.3.0, KubeJSable 1.1.0, Create 6.0.10, all in
the Prism instance `mods/` folder and matched against `pack/mods/*.pw.toml`, read with cfr and javap.
Vanilla facts: the decompiled 1.21.1 sources in the Gradle cache. Nothing here was run in a game: a fact
is "verified" when it was read in code or data.

**Numbers:** every number is a verified fact, arithmetic from one, or a labelled TUNABLE or
PLACEHOLDER. Anything unverified says UNVERIFIED.

**Stale notes found:** `pack/modDocs/create-aeronautics.md` and `create-propulsion-simulated.md` cite
Aeronautics 1.1.3 and Propulsion 1.1.4; the pack has 1.3.2 and 1.1.5. INDUSTRY.md §6 case 9, §8.7 and
its I17 row still say player ships stay frozen; I17 and C30 replaced that.

---

## 0. What the player sees

You build an airship with four vaults, hot-air burners and propellers, and put a CC: Tweaked computer
on board that flies it between your mine's dock and your home dock. While you watch, nothing is
different. When you leave, the ship keeps flying its route in the sim: it calls at both docks on time,
the vaults fill and empty there, its thruster tanks drain on each leg, and pirates or a hungry dragon
may meet it on the way. When you come back you find it where its timetable says, at a dock or in the
sky on its path, with the cargo it should have. The ledger shows "Skylark: 14 round trips while you were
away, 3 stacks of iron lost to sky pirates on the 9th". A ship whose route changes every trip stays
frozen where Sable left it, as today, and the ledger says why.

## 1. Reuse table

| What happens | The real mechanic | Mod (verified id or class) |
|---|---|---|
| The ship exists, with a stable identity | A Sable sub-level: `SubLevel.getUniqueId()` (UUID), `getName()`; a custom tag in `setUserDataTag` (saved as `user_data`) | sable |
| Its pose and velocity | `SubLevel.logicalPose()` (position is the centre of mass, orientation a quaternion), `ServerSubLevel.latestLinearVelocity` | sable |
| It stops when unloaded | `SubLevelHoldingChunkMap`: the ship freezes into a `HoldingSubLevel` when a chunk under it unloads; reloads when every covering chunk is loaded; keeps 0.9 of its velocity (`sub_level_velocity_retained_on_load`) | sable |
| It is moved | `PhysicsPipeline.teleport(body, position, orientation)`, loaded ships only | sable |
| It is kept loaded on purpose | `ServerSubLevelContainer.addForceLoadTicket(ship, SubLevelLoadingTicketType, key)`; a ticket type is `SubLevelLoadingTicketType.create(id, codec)`; tickets persist | sable |
| It is noticed loading and unloading | `SubLevelContainer.addObserver(SubLevelObserver)`: `onSubLevelAdded`, `onSubLevelRemoved(UNLOADED or REMOVED)` | sable |
| Its cargo and machines | Ordinary block entities in the ship's plot chunks (`ServerLevelPlot.addChunkHolder` sets ENTITY_TICKING): Create vaults, chests, tanks, deployers, mills | create, sable |
| Goods cross between ship and world | Sable redirects Create's capability lookups across the ship boundary (`CapManipulationBehaviourBaseMixin`): funnels, belts, depots, chutes, arms, ejectors, deployers, both ways; open-ended pipes see ship blocks; package ports reach within `packagePortRange + 2` | sable, create |
| It docks | Simulated `docking_connector`: states UNPOWERED, EXTENDED, LOCKING, LOCKED; the pairing is saved (`OtherConnector`, `OtherConnectorSubLevelId`); while docked, a fixed constraint holds the two, and a two-slot buffer (one 64-item slot each way), a fluid tank and a 10,000 FE battery pass between them | create-aeronautics (Simulated) |
| Docked or roped ships act as one | Sable loading dependencies: linked ships freeze and load together | sable |
| It is flown with nobody aboard | A player-built CC: Tweaked program: cc_sable's `sublevel` API reads the pose, Propulsion thrusters take `setPower`, Simulated's nav table, swivel bearing and sensors are peripherals; CC GPS gives world coordinates on a ship (Sable's `WirelessNetworkMixin`, code path only) | cc-tweaked, cc_sable, createpropulsion, Simulated |
| It burns fuel | Propulsion thrusters: `fuel_mB = 1.0 × power × consumption_multiplier × ticks` from data in `thruster_fuels` (pack: crude 0.7 thrust / 1.5 consumption, diesel 1.0/1.0, gasoline 1.1/1.2, ethanol distillate 0.8/1.0, refinery gas 0.6/2.0); tank 1,000 mB; solid-fuel thrusters burn coal (4,000 ticks a piece) | createpropulsion, pack kubejs |
| Or none | Aeronautics hot-air burners take no fuel (redstone-driven); propellers take SU from the ship's own Create network | aeronautics, create |
| It is timetabled while unloaded | `rtsim-ships` `Service` and `Leg` (dock, depart, cruise, approach), `fix()` (position as a closed form of the clock), `voyages_meeting`, `departure_at_or_before` | mic_rtsim |
| It is attacked | Sky pirates' T25 raids on booked player legs; dragons' R30 look-up of booked legs; Lanchester resolution against the ship's own defences (I21, I22) | mic_rtsim |
| Farms aboard | Vanilla sheep: `EatBlockGoal` (adults start eating with chance 1 in 1,000 per goal check, 40 ticks), `Sheep.ate` regrows wool, `shear` drops 1 + nextInt(3) wool; Create deployers with shears | minecraft, create |

## 2. Custom-roll ledger

Everything not in the table above, kept small:

1. **The vehicle record** in the industry engine: the Sable UUID, the ship's district id, its learned
   voyage, its stores, its fuel model, its state (learning, simulated, stale, not learnable, stranded).
   Saved with the industry state.
2. **The observer** (Java, `mic_rtsim_aeronautics`, PLAN.md's planned plugin): a `SubLevelObserver`, a
   pose sampler on the industry sweep, the docking-connector read, and the unload and load hand-over.
3. **The voyage learner** (Rust): turns pose samples and calls into a timetable (§4.2).
4. **One Sable ticket type**, `mic_rtsim:voyage`, used only to materialize a ship (§4.7).
5. **Passenger hand-over** for mobs aboard a moving simulated ship (§4.8), if V-8 is taken.
6. **Boundary records** for the above, in docs/boundary.md §7's ship range (ids assigned when built).

No new blocks, items or entities.

## 3. What happens today

Verified in Sable's code:
- When a chunk under a ship unloads, `ChunkMapMixin.sable$onStatusChange` calls `processUnload`: unless
  the ship is force-loaded, it and its loading-dependency chain are serialized into a `HoldingSubLevel`,
  filed under the holding chunk of its position. **Nothing moves or ticks while held.**
- `PhysicsChunkTicketManager.update` runs every physics tick. It expands each ship's bounds by 1 plus
  its predicted fall (at most 20) and holds the ship as soon as any covered chunk is not loaded enough.
- On reload, `SubLevelHoldingChunk.collectReadySubLevels` waits until every chunk under the saved bounds
  plus 1 is loaded, then `SubLevelSerializer.fullyLoad` allocates the same UUID and teleports it there.
- A ship with a player aboard cannot unload (`sub_levels_with_players_cannot_unload`, true).
- There is no API that moves a held ship. `SubLevelData.pose` is final; its `fullTag` is a mutable NBT
  tag, and rewriting its pose is UNVERIFIED and undocumented.
- The ship's blocks live in a plot grid in the same dimension: 128 × 128 plots of 2,048 blocks from plot
  (10000, 10000), so near x = z = 20.48 million; at most 16,384 ships per dimension.
- Splitting is on (`sub_level_splitting`): a 6-connected flood fill, 200 steps a tick, makes each
  disconnected piece a new ship. There are no assemble, split or merge events; KubeJSable 1.1.0 adds
  bindings and a physics-tick event only.
- No mod in the pack has autopilot, waypoint or route code (searched in Sable, Simulated, Aeronautics,
  Offroad, Propulsion, cc_sable and Drive By Wire). Drive By Wire needs a player holding the controller.

So today a sailbot stops in mid-air the moment its player walks away, and goes on from there when
someone comes near it again. The cargo at both ports stops moving.

## 4. The design

### 4.1 A ship is a vehicle district

- **Identity:** the Sable UUID. The plugin writes `{mic_rtsim: {vehicle, epoch}}` into the ship's user
  data tag, so a ship that comes back from a save, a split or a teleport is recognised.
- **Frame:** the district's members are the ship's plot positions. Plot coordinates do not change as the
  ship moves or turns, so the glass box compiles the ship once in its own frame (C32), exactly as it
  compiles a district on the ground: its vaults are stores, its deployers, mills and farms are machines,
  its propellers are kinetic consumers on its own network (GLASSBOX §2.1).
- **What depends on the world, not the frame:** only the pose (where it is), what it can reach at a call
  (§4.3), and what threatens it at sea (§4.9). Weather and wind do not: Sable's wind hook is fed only by
  PM Weather, which is not in the pack.
- **States** are the district states (INDUSTRY.md §2.3): learning, simulated, stale (edited since),
  not learnable (with a reason), plus **stranded** (out of fuel, §4.4).

### 4.2 Learning a voyage

The plugin never reads the autopilot. A CC program, a nav table aimed at a compass, a redstone timer
and a player at the wheel all look the same: a pose that moves. The voyage is learned from what the ship
does, as the glass box learns couplings (I6).

1. **Samples while loaded:** each industry sweep (GLASSBOX §1.4 cadence), the pose, the linear velocity,
   the fill of each store, and the state and partner of each docking connector. One read per tracked
   ship per sweep.
2. **A call** is a docking connector reaching LOCKED (its partner id says with whom), or, with no
   connector, the pose staying within R of one place for N ticks (R and N TUNABLE, PLACEHOLDER 2 blocks
   and 200 ticks). A call's dock pose is the mean pose over the call.
3. **A leg** is the trace between two calls: its duration, a polyline of sampled positions with their
   altitudes (rtsim-ships' `Polyline`, one point per sweep, simplified to within 2 blocks, PLACEHOLDER),
   and the fuel each tank lost.
4. **The voyage** is the shortest repeating sequence of calls, with each call's dwell and each leg's
   duration. It becomes an `rtsim-ships` `Service` whose lanes are the learned polylines: `Leg.dock` is
   the dwell, and `depart`, `cruise` and `approach` split the leg where the speed changes. The period P is
   the sum, the phase is taken from the last departure seen.
5. **Confidence:** the voyage replays after one checksum window (G-B4) in which a whole observed round
   trip matched the model within tolerance (each leg's time within 10%, each dock pose within R,
   PLACEHOLDER). Until then the ship freezes as today.
6. **Not learnable:** no repeating sequence over the window (a player flying it by hand, a program that
   wanders), or a call with no reachable port. The ship then freezes as today, and the ledger says why.
7. **Real behaviour wins:** whenever the ship is loaded again, the samples continue. A trip that disagrees
   with the model makes it stale; the next window relearns it. A CC computer restarts its program when
   its chunk loads, so a program that does not resume where the model says will show as a deviation,
   and the model follows the real ship.

### 4.3 Ports and the exchange

A port is **wherever goods actually cross during a call**, found by the same compile as any district
boundary (GLASSBOX §1.2):
- **A world district** whose movers reach the ship's stores: funnels, belts, chutes, mechanical arms,
  ejectors and deployers reach across the ship boundary through Sable's capability redirect; open-ended
  pipes and hose pulleys move fluids (fuel included); Create 6 package ports reach a ship within
  `packagePortRange + 2`.
- **A docking connector pair**, whose two-slot buffer, tank and battery pass goods only while LOCKED.
  Whether a connector can lock to a world-side connector (`otherSubLevel` may be null in the code) is
  UNVERIFIED; a GameTest decides.
- **Another vehicle district** (§4.5).
- **Not a port:** Create's Portable Storage Interface. Its moving half is a contraption movement
  behaviour and a Sable ship is not a contraption, so a PSI never serves a ship (by construction,
  UNVERIFIED in game). A village dock is not a port either: a player's ship trades nothing with a
  village unless a district of the player's is there.

**The exchange at a simulated call** is GLASSBOX §8.1's: `moved = min(stock or room on each side, mover
rate × dwell)`, from the compiled movers on both sides, with the learned exchange (the cargo diff of the
observed calls) as the fallback when a side has black-box parts (G-B16, I15).

### 4.4 The voyage while unloaded

- **Position** is `Service::fix(offset, t)`: the leg, the phase and the point along the learned
  polyline, a closed form of the clock. Nothing is stepped.
- **Cargo** moves only at calls, as periodic edges between the ship's stores and the port's (GLASSBOX
  §4.2: `b × (floor((t − φ)/P) + 1)` batches by time t), so a catch-up costs the same however many trips
  passed. The edge stops at the first breakpoint: a store full or empty, a port district that changed
  state, an attack (§4.9).
- **Fuel** is a store on the ship like any other. Each leg draws its learned fuel (the tank loss measured
  on observed legs), checked against Propulsion's formula `fuel_mB = 1.0 × power × consumption_multiplier
  × ticks` for the throttle levels seen. Refuelling is part of a call's exchange (a pipe at the dock).
  Hot-air burners draw nothing. Propellers draw SU, so their cost is whatever drives the ship's network:
  a steam engine's boiler and burner fuel, a Diesel engine's fuel, both stores aboard.
- **Running dry** is a breakpoint: the instant the fuel store reaches zero on a leg, by arithmetic on the
  leg's draw rate. The ship is **stranded** at that point of the polyline, at that altitude, and the
  ledger says "out of fuel at x, z". On load it materializes there (§4.7), and Sable's physics decides
  what a ship without thrust does: one held up by hot air or envelopes hangs there, one held up by
  thrusters falls, for real.

### 4.5 Ship-to-ship ferries (C32)

Both ends of a call can be vehicles. The ferry's call is then with another vehicle district, and where
that call happens depends on the partner:

- **A hovering partner** (the sheep ark of C32 most likely: an airship that holds still) is a fixed dock
  pose. The ferry is an ordinary voyage, and the exchange is a periodic edge between two vehicle stores.
- **A partner on its own periodic voyage:** the ferry's calls are learned as "meet vehicle V", not as a
  pose. Each meeting is found from both timetables: departing at t0 from its last call at speed v, the
  ferry meets V at the first t where the distance from the ferry's departure point to V's position at t
  is at most v × (t − t0). That is one root per meeting, found on V's polyline pieces (closed form per
  straight piece, a quadratic). When the two periods are commensurate (P_ferry × a = P_V × b for small
  whole a and b, checked on learning), the meetings repeat with period P_ferry × a and the edge is
  periodic again; otherwise a catch-up costs one root per meeting in the window.
- **A partner that moves irregularly:** the ferry is not learnable, because its partner is not.
- **Docked pairs move together:** while LOCKED, Sable makes the two one loading-dependency chain, so they
  freeze, load and are materialized together (§4.7).

### 4.6 Farms aboard (C32: 2,000 sheep)

A farm on a ship compiles in the ship frame like a farm on the ground (G-B17: farms model vanilla's
rules). For sheep, the rules are these (vanilla 1.21.1, verified):
- An adult sheep starts eating when `EatBlockGoal.canUse` passes: a 1 in 1,000 roll (1 in 50 for a
  lamb), and short grass at its feet or a grass block under it. Goals are checked on a mob's full AI
  ticks, every second tick (`(tickCount + id) % 2 == 0`), so a sheep on grass starts eating after 2,000
  ticks on average, and eating takes 40 ticks.
- Eating calls `Sheep.ate`: the wool grows back, and a lamb ages 60 seconds' worth. With mob griefing on
  (the pack does not change the gamerule), the grass block becomes dirt and short grass is destroyed.
- Shearing (shears, or a Create deployer holding shears) drops `1 + nextInt(3)` wool: 2 on average.

So a pen of n sheared adults on grass regrows about n / 2,040 fleeces a tick at most, limited by the
grass that is left (grass spreads back by random ticks, so whether random ticks run in a ship's plot
chunks matters: UNVERIFIED, the same question as OPEN-QUESTIONS open fact 2), and by the shearers' cycle
rate. Wool out is `min(regrowth, shear capacity) × 2` per tick, a closed form with no per-sheep work:
2,000 sheep cost what 20 do. The checksum window (G-B4) compares this with the wool actually counted
while the farm was loaded; if they disagree, the farm stays learned-only for that window and the
ledger says which term was off.

The sheep themselves stay where they are: a hovering farm ship never moves while unloaded, so its
animals never need moving. A farm on a ship that travels is §4.8's case.

### 4.7 Materializing anywhere watched (G-B7)

The only public way to move a ship is to teleport it while it is loaded. So:

1. **When the ship's sim position comes into a player's view** (the interest area check that already
   materializes sim ships, PLAN.md §10.3), the plugin puts a `mic_rtsim:voyage` force-load ticket on the
   held ship. Sable loads it where it froze, which loads the chunks under that stale pose.
2. **Once it is loaded** (`onSubLevelAdded` for that UUID), the plugin teleports it to the sim pose with
   `PhysicsPipeline.teleport`: at a dock, the dock pose (docking connectors re-lock within Simulated's
   0.5 blocks and 20°); at sea, the polyline point at the leg's altitude, heading along the leg; stranded,
   where it stopped.
3. **Before the teleport,** the ship's world bounds at the target are checked for blocks. If the space is
   taken (a player built there), the target slides back along the path to the first free pose; if none
   is free within one leg, the ship stays abstract until the player leaves, and the ledger says so.
4. **Then** the cargo and fuel deltas since the last load are applied to its stores, with the epoch (I8),
   and the ticket is dropped. The ship is now in the watched chunks and Sable runs it from there.
5. **If the stale pose loads by accident** (a player walks to where the ship froze while the sim has it
   elsewhere), the observer sees it added there. It is teleported at once to its sim pose: into the
   watched area if that is in view, otherwise into unloaded chunks, where Sable holds it again under its
   new position at the next physics tick. Either way it is never seen at a place it is not. Whether a
   ship teleported into unloaded chunks is held cleanly there is UNVERIFIED; a GameTest decides (§7).

Cost: one extra chunk load at the stale pose per materialization, and never more than one per ship,
because after it the held copy sits where the sim last put it.

### 4.8 Passengers, riders and logging off aboard

- **Where entities aboard live** (mobs, item frames, a player standing on the deck) is UNVERIFIED: in
  world space carried by collision, or in plot space with the blocks. It decides what follows the ship.
  If they are in world space, a ship moved by the sim leaves them behind.
- **V-8's recommendation:** when a tracked ship is held, the entities inside its world bounds and
  standing on its blocks are recorded in the ship frame (a mob's UUID and its offset). When the ship is
  materialized, they are moved with it by the same rigid motion, after their chunks have loaded. A ship
  whose passengers cannot be found on load is flagged, never duplicated.
- **A player riding the ship** keeps it loaded (`sub_levels_with_players_cannot_unload`), so it is
  watched and never simulated.
- **A player who logs off aboard:** the ship unloads with them gone and keeps its voyage. When they log
  back in, they appear aboard at the ship's current sim pose, at their offset in the ship frame, and the
  ship is materialized around them (V-9).

### 4.9 Threats at sea: pirates, dragons, hazards

- **Booking:** a learned voyage is booked once into the region and time-window index as a standing route
  (GLASSBOX §8.2, A15). Booking costs nothing per trip.
- **Sky pirates:** T25 already raids "a booked player leg crossing the lane's regions (a train leg today;
  a player's ship schedule would book the same way)". A ship leg is that booking.
- **Dragons:** R30's hungry dragon looks up booked legs in the regions it passes and falls on the first
  it overlaps. Ship legs join train legs in that lookup.
- **Resolution:** Lanchester against the ship's defence vector (I21), measured while loaded from the
  ship's own turrets (Create Radars with Create Big Cannons), guards aboard, and observed fights;
  untested defences count at a reduced weight (I22).
- **What an unwatched attack does (V-5):** it loots cargo from the ship's stores within the attackers'
  carry and the per-attack cap (I23), and breaks no blocks. A broken envelope or propeller would crash
  the ship on load, a consequence the sim cannot bound. Loot goes where it goes for the attacker: a
  pirate haven's market (C24), a dragon's eaten, burnt and hoarded books (R5).
- **Watched,** the pirates or the dragon materialize and the fight is real.
- **Escorts:** an armed player ship that sails a lane NPC merchants use could add its strength to
  `LaneThreat.escorts`, which rtsim-ships keeps for players and nothing feeds yet (V-11).

### 4.10 Edits, breaks, splits and reassembly

- **Edits** to a ship's blocks recompile it (G-B3: player edits plus a per-sweep settings hash), and it
  replays again after one checksum window (G-B4). Edits that change only cargo or settings do not touch
  the voyage; edits that change mass or thrust make the voyage stale too, because leg times may change.
- **A split** (Sable's flood fill) shows as new UUIDs added while the tracked ship is loaded, with
  bounds that overlap it. The tracked ship goes stale; each piece is a new vehicle, learning.
- **A ship removed** (`onSubLevelRemoved` with REMOVED, for example disassembled) ends its vehicle. Its
  stores are world blocks again, so a ground district picks them up on the next census.
- **Damage taken while loaded** is just an edit.

### 4.11 World wrap (C31): what voyages need from the topology layer

Deep Time and Toroidal World are not in the pack's mods yet (no `.pw.toml` for either), so none of
this can be tested in the pack today. What the voyage plugin needs:

1. **Canonical poses** (TP4): every stored pose and dock is canonical; a learned polyline is a chart
   walk (TP7), restarted at the canonical position at each call.
2. **Meetings through seams** (TP9): threat legs and ship-to-ship meetings use `first_within` and
   `closest_to`, so a pirate east of the seam finds a ship west of it.
3. **Materialization at the nearest image** (TP11): the ship appears at the image of its sim position
   nearest the watching player; its orientation is turned by the same motion (across a north or south
   half-turn the heading reverses).
4. **East and west crossing of a loaded ship (UNVERIFIED, a real gap):** Toroidal World folds an
   *entity* that crosses its seam (TP11). A Sable ship is not an entity. If nothing folds the sub-level,
   a ship flying east past x = b keeps going while its rider is folded back by 2b, and falls. The
   topology work needs a Sable adapter that teleports the ship by the same motion, with its passengers,
   in the tick it crosses.
5. **The plot grid must never be folded:** ship blocks live near x = z = 20.48 million, far outside
   [-b, b). Neither Toroidal World nor the wrap engine may treat plot chunks or anything in plot space
   as positions to fold. UNVERIFIED until both are installed together.
6. **North and south:** ships stop one block inside the edges under edge_stop (TP12), and ship crossing
   north and south "in the engine (not built)" stays not built. A learned route never crosses: the real
   ship would have stopped. Within `CONE_CLEARANCE` (48 blocks, TP13) of a cone point, a ship is not
   materialized.

## 5. Walkthroughs

**A. The sailbot.** Ana's airship Skylark has four Create vaults, hot-air burners for lift and two
Propulsion thrusters on diesel. A computer aboard runs her program: fly to the mine's dock by GPS, wait
until the vaults stop filling, fly home, wait until they are empty.
1. She flies it once by hand while building the docks. Irregular, so it is "not learnable: no repeating
   route" on the ledger.
2. She starts the program and watches three round trips from her home tower. Each call locks the
   docking connectors at both docks; funnels on the dock side fill and empty the vaults through Sable's
   capability redirect. The learner has a voyage: 2 calls, legs of about 4 minutes, dwells set by the
   funnels' rate.
3. The checksum window passes: the third trip matched the first two. Skylark is "simulated".
4. Ana logs off. The ship freezes over the desert, mid-leg, and the sim takes it.
5. Over the next real hour the sim runs the timetable: 7 round trips by arithmetic, each moving `min(ore
   at the mine, room in the vaults, funnel rate × dwell)`. The thruster tanks draw their learned diesel
   per leg; the home dock's pipe refills them at each call.
6. On the 5th trip a sky pirate's hunt crosses the leg (T25). Unwatched: Lanchester against Skylark's
   one cannon; the pirates take 2 stacks of iron to their haven. No blocks break.
7. Ana logs back in at home. Skylark's timetable has it 300 blocks out, inbound. Her view reaches that
   point: the plugin force-loads the stale pose over the desert, Skylark loads there, is teleported onto
   its path heading home, the vaults get their deltas, and the ticket is dropped.
8. Her program restarted when the computer loaded. It reads GPS, sees it is between docks and flies
   home. The samples agree with the model; nothing goes stale.
9. The ledger: "Skylark: 7 round trips, 7 vault loads of iron ore delivered, 2 stacks lost to sky pirates".

**B. The sheep ark and its ferry (C32).** Bo's ark holds about 2,000 sheep on grass decks, sheared by
deployers into vaults. It hovers in one place. A second ship, the ferry, shuttles wool to Bo's base and
brings back nothing.
1. While loaded, the ark is compiled: n sheep, the grass decks, the deployers' cycle, the vaults. The
   checksum window compares the model's wool with the wool counted; they agree.
2. The ferry's calls lock its connector to the ark's, then to the base's. The ark does not move, so its
   dock is a fixed pose and the ferry is an ordinary voyage.
3. Bo leaves. The ark costs one closed form: wool per tick, `min(regrowth, shear capacity) × 2`, until
   its vaults fill (a breakpoint). The ferry's periodic edge drains them at each call.
4. A hungry dragon's leg crosses the ferry's route (R30). The ferry is looted: wool, eaten or burnt in
   the dragon's books. The ark is not on a booked leg and is not touched by flyers passing over, unless
   it is the dragon's target (its sheep are livestock for P12: whether a player's farm ship counts as
   prey like a village's livestock is V-10's question too).
5. The ferry's diesel runs short because Bo never piped fuel to the ark's dock: it is stranded halfway on
   its 31st trip. The ledger says so. When Bo next looks that way, the ferry is there, hanging under its
   envelopes.

**C. Across the seam (Deep Time, later).** Skylark's route crosses the east edge. Its learned path is a
chart walk past x = b; positions fold to canonical ones; a pirate lane just west of the seam meets it
through `first_within`. Ana watching from the west side sees it at the image nearest her. While she rides
it across, the Sable adapter (§4.11 item 4) moves the ship and her together.

## 6. Cost model

| Mechanic | When it costs | Cost |
|---|---|---|
| Pose and store samples | per tracked loaded ship per sweep | one pose read, one connector read; store fills only at calls |
| Learning | per call while loaded | O(samples of the leg) once, to simplify the polyline |
| Position while unloaded | per read | O(log legs) (`leg_at`), closed form |
| Cargo at calls | per catch-up | O(1) per periodic edge (§4.4), however many trips passed |
| Fuel and stranding | per catch-up | O(1); the stranding instant is computed once per leg schedule |
| Moving partner meetings | per catch-up | O(1) when periods are commensurate, else one quadratic root per meeting in the window |
| Farm aboard | per catch-up | O(1) per pen, independent of animal count |
| Threat booking | per learned voyage | once, as a standing route; zero per trip |
| Materialization | per ship coming into view | one force-load at the stale pose, one teleport, O(stores) deltas, O(passengers) moves |
| While nobody watches and nothing changes | | zero |

Memory: per vehicle, the timetable (legs and simplified polylines, a few hundred points at most,
PLACEHOLDER) and the compiled district. The bench (`bench scale`) gets a "player ships" sweep: 100
simulated ships, catch-ups after 1, 10 and 100 real hours.

## 7. Tests before it ships

GameTests settle each UNVERIFIED point before the code depends on it:
1. Force-load a held ship with a custom ticket type, teleport it, drop the ticket: it stays where it
   was put and unloads cleanly.
2. Teleport a loaded ship into unloaded chunks: Sable holds it there, and it loads there later.
3. Where entities aboard live (world or plot space), and whether a teleport carries them.
4. A docking connector locking to a world-side connector.
5. Random ticks in plot chunks with no player near (grass regrowth, crops aboard).
6. A CC GPS fix on a ship computer.
7. The sheep model against a counted pen of 200 sheep for one checksum window.
8. With Toroidal World and Deep Time installed: a ship crossing the east seam with a rider; plot chunks
   untouched.

## 8. Proposals for Ben

| Id | Question | Recommended | Alternatives |
|---|---|---|---|
| V-1 | How a voyage is known | learned from the ship's own motion, calls and cargo over one checksum window; the autopilot is a black box | (b) read the CC program or nav table targets (impossible in general); (c) the player declares a route (gamed, and needs a UI) |
| V-2 | What marks a call | a docking connector reaching LOCKED, else a dwell within R for N ticks | (b) dwell only; (c) connectors only |
| V-3 | What counts as a port | any district, ground or vehicle, whose movers exchange goods with the ship during a call, through Sable's cross-boundary capability lookups | (b) docking-connector pairs only; (c) ground districts only |
| V-4 | How a ship is moved to where the sim has it (G-B7) | force-load its held copy with a `mic_rtsim:voyage` ticket, teleport it, drop the ticket | (b) rewrite the pose in the held ship's NBT (no API, UNVERIFIED); (c) show it only where it froze |
| V-5 | What an unwatched attack does to a ship | loots cargo within caps, breaks no blocks | (b) bounded block damage like districts (I23), which may crash it on load; (c) no unwatched attacks on ships |
| V-6 | Fuel | learned per leg from tank losses, checked against Propulsion's formula; a ship that runs dry is stranded where its fuel ends | (b) the formula only; (c) ignore fuel |
| V-7 | Ship-to-ship ferries | hovering partner as a fixed dock; partner on a periodic voyage met by the pursuit root; irregular partner not learnable | (b) hovering partners only; (c) learn every meeting as observed, no replay between |
| V-8 | Mobs and items aboard a ship the sim moves | recorded in the ship frame when it is held, moved with it when it materializes | (b) a moving ship with mobs aboard is not learnable; (c) left where they were (they fall) |
| V-9 | A player who logged off aboard | appears aboard at the ship's current pose | (b) the ship waits for them; (c) they appear where they logged off, in the air |
| V-10 | Large farms aboard (C32) | vanilla's eat, regrow and shear rules per pen, checked by the checksum window; a player's farm ship is prey for dragons only when a dragon targets it, like any district | (b) a learned wool rate per farm; (c) farm ships are livestock for P12 like a village's |
| V-11 | Armed player ships as escorts | later: their strength feeds `LaneThreat.escorts` on lanes they sail | (b) never |
| V-12 | World wrap (C31) | ask the topology work for a Sable adapter that folds ships and riders at the east and west seam, and a guarantee that plot space is never folded; north and south stay edge_stop | (b) voyages near a seam are not learnable until then |
| V-13 | A held ship's stale pose loaded by accident | teleported at once to its sim pose, into view or back into unloaded chunks | (b) it loads there and the sim gives way (cargo kept, timetable reset) |
