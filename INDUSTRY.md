# Industry: distant factories, supply lines and the threats to them

**Status:** draft design for Ben's review, 2026-10-20. Nothing is built. This is workstream C15.

**Decided, and binding here:**
- C15: simulate distant factories and supply lines across Create and the pack's addons, without
  relying on chunk loaders. Chunk loaders can still be used alongside.
- C16: bandits, dragons, natural mobs and bosses attack player supply lines and factories, even while
  the player is AFK.
- C17: on dedicated servers, attacks also hit bases whose owners are logged off.
- C18: unwatched attacks resolve abstractly. Real changes apply on the next chunk load: stock stolen
  from boundary inventories, and machines and blocks destroyed, bounded per attack.
- C19: a district is a black box by default. What crosses its boundary is measured while loaded,
  replayed while unloaded, and reconciled into boundary inventories on load. Plugins add precision.
- Also binding: D1 to D4, A9 (journal), A13 (flows), A15 (lazy regions, booked legs), A16 (paging),
  and SCALE.md §0.

**Open:** I1 (Ben chose "decide later"): schematic snapshots for rebuilding. I2 onward are Claude's
proposals, each with the recommended option first, gathered in §20.

**Numbers:** every number is a verified fact (Appendix A), arithmetic from one, or a labelled TUNABLE.
**Inputs:** `research/industry-inventory-2026-10-19.md` (pack mods unloaded),
`research/economy-2026-10-19.md` (integer breakpoints), Create `mc1.21.1/dev` checked against the 6.0.10
jar.

---

## 0. What happens today when a factory unloads

| Thing | While its chunk is not ticking | Verified in |
|---|---|---|
| Machines, belts, pipes, funnels, chain conveyors, packagers, frogports | **Stop.** Block entities tick only at `FullChunkStatus.BLOCK_TICKING`. | `LevelChunk.isTicking` |
| Kinetic networks | **Gone.** Each block entity leaves its network on unload, and saves its own copy of speed and network stress. | `KineticBlockEntity`, inventory |
| Factory gauges | **Wait** while *any* link of their network is unloaded (`waitingForNetwork = getUnloadedLinkCount > 0`, which is `totalLinks - loadedLinks`). A home gauge whose network includes an outpost's stock link freezes when the outpost unloads. | `FactoryPanelBehaviour.tickStorageMonitor`, `GlobalLogisticsManager` |
| Stock requests | **Fail:** only loaded links answer. | `LogisticallyLinkedBehaviour.isValidLink` (`!isChunkUnloaded()`) |
| Trains | **Run,** ticked once per server tick with no chunk check. They stall only when a player rides within 32 blocks and the 16-block lookahead is not entity-ticking. | `GlobalRailwayManager.tick`, `Train.tick`, `Carriage` |
| Package mail | **Works.** On every 21st tick that a train is at a station, `runMailTransfer` swaps packages between `Carriage.storage` and the station's ports. An unloaded port uses `GlobalPackagePort.offlineBuffer`. | `Train.tick`, `GlobalStation.runMailTransfer` |
| Offline buffer size | **18 slots.** A package stacks to 1 and holds 9 stacks, so at most 18 × 9 × 64 = 10,368 items of a 64-stack item wait per unloaded postbox. | `GlobalPackagePort`, `PackageItem.SLOTS` |
| Package fetching | **Works:** the "fetch packages" instruction reads offline buffers, so trains go to unloaded stations holding matching packages. | `FetchPackagesInstruction.start` |
| Portable Storage Interface transfer | **Stops.** Carriage entities are discarded in non-ticking chunks. | `CarriageEntityHandler` |
| Contraptions, Sable and Aeronautics ships | **Frozen.** | inventory; PLAN Appendix A |
| Random ticks (crops) | **Happen only** within 128 blocks of a player, or under a NeoForge *ticking* ticket. create_power_loader and mic-grid-loader use ticking tickets, so crops grow under them. | patched `ServerChunkCache.tickChunks`, `ChunkLoadManager` |
| Natural spawns | **Need a nearby player,** even in force-ticked chunks: `LocalMobCapCalculator.canSpawn` is false with no player near. Spawner blocks need a player within 16 blocks. | vanilla `LocalMobCapCalculator`, `BaseSpawner` |
| Sleeping | **Skips `dayTime` only.** `gameTime` still advances by one per tick. | `ServerLevel.tick`, `tickTime` |
| A stopped server | **Nothing runs.** | |

**What follows:** goods shipped by train already move unloaded, but only as packages; everything that
makes or uses goods stops; and a gauge network spanning two places breaks as soon as one end unloads.

**Dimensions.** `GlobalRailwayManager.tick(level)` returns unless `level` is the overworld, so it runs
once per server tick and then ticks every train in its global map. Carriages resolve their dimension
through `server.getLevel` (`Carriage.manageEntities`), and package fetching reads each station's own
dimension. PLAN §10.4 and the inventory read this as "overworld trains only"; the source reads as every
dimension. A GameTest settles it (§19).

---

## 1. Prior art: what to take

| Source | What it does | Lesson |
|---|---|---|
| Unloaded Activity (Modrinth `unloaded-activity`, unloadedactivity.moono.dev) | Binomial catch-up of random ticks on load ("a little bit of math") instead of replaying ticks. It skips redstone and other mods' machines. | Closed-form catch-up per mechanism works. No mod we found does this for machines. |
| Offline Progression (CurseForge); The Block Keeps Ticking (Modrinth `PVuSY2aI`) | Replays real ticks for claimed chunks: 2 chunks per player, `maxCatchupDays` 2, a 1,200 s wall-clock budget, plus a "Lazy Tax" knob. | Cost is O(elapsed), held back by hard caps. This breaks SCALE.md §0, so it is rejected. |
| DSP SampleAndHoldSim (Thunderstore `starfi5h/SampleAndHoldSim`) | Runs remote planets fully once per `UpdatePeriod` (default 10) and applies the recorded I/O change in between. Its listed costs: stock can go negative, stats spike, launches come in bursts. | The closest prior art to measure and replay. Never overdraft. Treat bursts apart from steady flow. |
| Satisfactory Factory Tick (docs.ficsit.app) | Simulates every machine always; LOD affects visuals only. Dedicated servers can pause when empty. | Running everything is the cost we cannot pay. |
| Factorio FFF-18, 148, 176, 364 | "Do less": entities sleep and wake on change; belts store gaps; wakes apply in deterministic order. | Steady flow costs O(boundary). Transitions are the work. |
| X4 low attention (Steam guide 3463911140; discussion 840627496100645234) | Simplified remote combat ("turrets only have a 30% chance to fire"). Players complain in both directions. | Players exploit whichever way an abstraction errs. Unwatched is never better than watched. |
| Idle games (Cookie Clicker wiki, theidleshard.com, onirealms.com) | Yield is elapsed time × a clamped rate. A rate sampled during a booster paid a fortune. Moving the PC clock granted resources. | Use the steady rate, never the peak. Use server game time. |
| Woot (GitHub `Ipsis/Woot`), Hostile Neural Networks | Virtual mob farms, per mechanism. | Plugins add precision per mechanism. |
| create_power_loader issue #81 | "Wasn't designed to work with fast moving entities". | Chunk loading fails for moving things. |
| Kenshi (Steam discussion 2801756152808050895) | Raids never trigger on unloaded bases. | Ben chose otherwise (C17), so every attack is bounded and paced (§13.9). |

---

## 2. Districts

### 2.1 What a district is (I2)

A **district** is a set of chunks in one dimension; each chunk belongs to at most one. It is the unit
that is measured, simulated, reconciled and attacked.

**Proposal I2 (Ben): automatic detection.** Recommended.
- **Industrial chunks** hold a block entity a player placed (`BlockEvent.EntityPlaceEvent`) that has an
  item, fluid or energy capability, or that a plugin recognises (Create kinetics, PowerGrid nodes). A
  district is a connected group of them (8-neighbour).
- **Closed under physical connection:** belts, chutes, pipes, shafts, chain conveyors and PowerGrid
  wires (plugin) merge the chunks they touch, so only ports (§3.3) and the links of §8.6 cross a
  boundary.
- **Name:** the first Create station inside (`GlobalStation.name`), else its coordinates. **Editing:**
  `/rtsim district rename | merge | split | exclude`. No new items or blocks (D3).
- **This retires P5.** The unit is the district (C19), and a logistics network is plugin detail (§8.4).

**Alternatives:** (b) Player-designated only, by command; (c) One Create logistics network per district
(P5), which misses factories without stock links; (d) A chunk loader's footprint.

### 2.2 The boundary (I3)

The box needs **stock**, where goods wait, and **ports**, where goods cross.

**Proposal I3 (Ben): stock is tagged storage plus Create's own stock inventories.** Recommended.
- **Boundary stock** is inventories inside the district, reached through
  `Capabilities.ItemHandler.BLOCK`, `FluidHandler.BLOCK` and `EnergyStorage.BLOCK`, on: blocks in a
  datapack tag `#mic_rtsim:district_stock` (default: Create vaults and fluid tanks, Create Connected
  silos and vessels, the `c:` chest and barrel tags, FE storage blocks); the inventory behind each
  Create packager (`PackagerBlockEntity.targetInventory`); postbox and frogport inventories, counting
  package contents (`PackageItem.getContents`); and plugin buffers such as PowerGrid batteries.
- **Machine inventories** (basins, depots, crusher and vat contents) are work in progress. They are
  never sampled for rates and never written.

**Alternatives:** (b) Count every capability inventory, but write only tagged stock; (c) Stock-link
inventories only.

### 2.3 District states

| State | Meaning | While unloaded | The player sees |
|---|---|---|---|
| Live | chunks ticking; Create runs it | n/a | rates being measured |
| Learning | Live, with no steady model yet | frozen, as in vanilla | "learning: N min more" |
| Learned | passed the test of §3.4 | runs abstractly | rates, next breakpoint |
| Stale | a player edited it since it was learned | frozen (I5) | what changed |
| Not learnable | a hard case with no plugin (§6) | frozen | the reason, in words |
| Suspended | part of it is ticking (I7) | live part real, rest frozen | which part is live |
| Damaged | an attack destroyed a machine | stopped until reloaded (I23) | the attack report |

---

## 3. Measuring while loaded

### 3.1 The identity

- **A good** `g` is an item id plus its data components, a fluid plus its components (a Destroy
  `MixtureFluid` is its own good), or an energy kind.
- `S_g(t)` is boundary stock, and `P_g(t)` is cumulative net import through ports. Internal net
  production is `N_g(t) = S_g(t) - S_g(t0) - P_g(t)`.
- **Machines never need to be seen.** In steady state, work in progress on belts and in machines is
  constant, so it does not bias the slope of `N_g`. On unload it freezes in place, and it resumes on
  load.

### 3.2 Sampling and its cost

- **How:** the bridge reads boundary inventories round-robin on the server thread, K per tick
  (TUNABLE, sized from the 1 ms bridge budget in phase 0), and hands over a sorted (good, count)
  vector per inventory.
- **Reuse:** packager inventories reuse Create's 20-tick network summary (`LogisticsManager.SUMMARIES`).
  An optional mixin, a change flag on Create vaults, lets the sampler skip unchanged ones (P7 ledger).
- **Cost** grows with the boundary inventories of loaded districts ÷ the sample period, never with the
  items moved.

### 3.3 Ports

| Port | Hook |
|---|---|
| Train cargo at a district station | `Train.arriveAt(GlobalStation)` and `leaveStation()`: diff `Carriage.storage` (`TrainCargoManager`, items and fluids). This also records the exchange pattern for I15. |
| Packages | postbox inventories are stock; the train-side diff is the port |
| Players | `PlayerContainerEvent.Open/Close` on stock (the session's diff is the player's); `ItemTossEvent` and `ItemEntityPickupEvent.Post` inside the district; `BlockEvent.BreakEvent` of a stock block by a player |
| Ender Transmission channels, PowerGrid lines | plugins (§8.6) |

**What is not a change:** machine edits (deployers, drills, saws, harvesters) and chunk loaders
switching on and off. Only `BlockEvent`s with a player make a district stale.

### 3.4 Steady state, cycles and confidence (I5)

**Proposal I5 (Ben): a windowed steady test; frozen until learned.** Recommended.
- **The fit:** sample every Δ ticks (TUNABLE) and fit each good's slope `r_g` of `N_g` by least
  squares over a window W.
- **The window:** W is at least max(W_min, k × the longest quiet gap in any good's stock), both
  TUNABLE. A train calling every T minutes moves stock in steps, so W spans several calls. A Create
  gauge requests at most once per `factoryGaugeTimer` (100 ticks). Daylight-dependent machines
  (PowerGrid solar, via plugin) force W up to a full 24,000-tick day.
- **Steady:** for every good above a rate floor (TUNABLE), the two half-window slopes agree within ε
  (TUNABLE).
- **Discarded windows:** an unattributed container session, creative-only blocks (creative motor and
  crate), or any player edit.
- **Rates are the steady slope, never the peak.** While loaded, learning never stops: a newer steady
  window replaces the model.
- **Unlearned, stale and not-learnable districts freeze while unloaded.** That is today's behaviour,
  so the sim never makes things worse.

**Alternatives:** (b) Stale districts run their last model; (c) Run at reduced confidence (a TUNABLE
factor).

---

## 4. The model and the replay

### 4.1 The box process (I6)

**Proposal I6 (Ben): one all-or-nothing process per district.** Recommended.
- **Running or stopped:** `r` splits into consumed goods (`r_g < 0`) and produced goods (`r_g > 0`), and
  the district runs at full measured rate or not at all (`x ∈ {0, 1}`). That is how a Create line
  behaves: it runs, or it jams.
- **It stops when** any consumed good reaches 0, the output space shared by the produced goods fills,
  or a produced good reaches a **learned target**: a stock level at which production was seen to stop
  while inputs were available, which is what a gauge or a threshold switch does.
- **It restarts** on an event that relieves the constraint: an import, an export, or a repair. It never
  over-produces: if one output backs up, everything stops.

**Alternatives:** (b) One process per output, which over-produces when a byproduct jams; (c) Start from
the recommended option, then learn the coupling from starvation and jam events seen while loaded.

### 4.2 Breakpoints and catch-up

- **Segments:** between events, stocks are linear. The next breakpoint is the earliest of an input
  emptying (at `ceil(S_g × den_g / num_g)` ticks, rates as rationals as in the economy research), the
  output space filling, or a target being reached.
- **Integers:** produced counts are `floor((acc + num × Δt) / den)` with the remainder carried
  (SCALE.md §9), so `catch_up` composes exactly. Chance outputs (crushing byproducts) replay at the
  measured mean, with no RNG.
- **Cost:** O(breakpoints + events), independent of elapsed time. **Clock:** `gameTime`, never
  `dayTime`.

**Proposal I4 (Ben): time passes only while the server runs.** Recommended. With the server stopped,
nothing is simulated (no production, no trains, no threats); Create's trains do not move then either.

**Alternative:** (b) Wall-clock catch-up with a cap, idle-game style. Districts would run while trains
stood still, and it is open to clock tricks.

### 4.3 Imports and exports are events

- **Trains really run,** so an import is a real arrival, delivered by the station hook or by mail. On
  each arrival the district catches up to now, applies the transfer, and recomputes its next breakpoint.
- **No forecast ever moves goods.** SCALE.md §7's flow solve is kept only for the links of §8.6.

### 4.4 Plugin processes

- **A plugin may split the box** into processes with exact coefficients, sharing stock in a fixed order
  (by plugin, then process id).
- **Create factory gauges are exact:** `targetedBy` gives up to 9 inputs with an amount per batch,
  `recipeOutput` the output per batch, the filter and its count the target. `factoryGaugeTimer` allows
  at most one batch per 100 ticks by default: 240 per gauge per Minecraft day (24,000 ÷ 100).

---

## 5. Reconciliation on load (I7, I8)

**Proposal I7 (Ben): a district suspends while any of its chunks ticks.** Recommended.
- **When the first chunk reaches block ticking,** the abstract run catches up to now and stops. Loaded
  chunks get their deltas; the others keep pending deltas until each one loads.
- **While suspended,** the live part runs for real and the rest stays frozen, as in vanilla. When every
  chunk stops ticking again, the run resumes from sampled stock for the chunks that were live and from
  sim stock for the rest.
- **Learning resets only on a player edit.** A brief wake (a create_power_loader train ticket passing
  through) costs one reconcile of the woken chunks.

**Alternatives:** (b) Keep simulating the frozen remainder. Goods would have to cross the live-frozen
edge on belts, which C19 forbids; (c) Force-load the remainder, which is chunk-loading cost (D1).

**Proposal I8 (Ben): exactly-once deltas through a chunk attachment.** Recommended.
- **Deltas per inventory:** removals come from the inventories that held the good at the last sample,
  in proportion; additions go to the inventories that received it while learning, up to capacity. The
  replay stopped at that same capacity, so additions always fit.
- **Epochs:** each chunk carries an attachment `mic_rtsim:reconcile` holding (district id, applied
  epoch). Chunk attachments are written into the same chunk NBT as its block entities
  (`ChunkSerializer`, `neoforge:attachments`), so the contents and "epoch e applied" reach disk together.
- **Applying:** `ChunkEvent.Load` queues the chunk. On the next tick, under the bridge quota, every
  delta with an epoch above the chunk's is applied, removals first. A crash anywhere replays correctly,
  because an older chunk also carries an older epoch.
- **Shortfall:** a machine that ticked first may have taken an input. The missing amount then trims the
  outputs it funded, by the model's ratios. The ledger shows it, and conservation holds.
- **Fit with the rest of the plan:** this is PLAN §4.9's operation ids for vault deltas, and it answers
  P29 for them.

**Alternatives:** (b) The global journal SavedData only, which is not atomic with chunk saves; (c)
Rewrite NBT in `ChunkDataEvent.Load`, which depends on every mod's NBT layout.

---

## 6. The hard cases

**Box:** the black box handles it unchanged. **Plugin:** it needs a plugin. **Out:** not simulated; the
district freezes while unloaded, and the ledger gives the reason.

| # | Case | Treatment | How, and what the player sees |
|---|---|---|---|
| 1 | Kinetic power crossing the boundary on shafts | **Box**, by construction | Shafts and cogs merge chunks into one district (I2), so stress never crosses a boundary. Inside, waterwheels and motors are part of the box. If a player splits a district across a shaft: "not learnable: rotation crosses the boundary". |
| 2 | PowerGrid electricity | **Box** inside, **Plugin** across | A wire line between two districts couples their running states (I13). Without the plugin: "not learnable: electric line crosses the boundary". |
| 3 | Destroy mixtures, reactions, pollution | **Box** for steady mixtures, **Plugin** for pollution (I14) | A steady mixture is a good. A drifting composition never passes the steady test: "mixture composition not steady". Explosions are case 11. |
| 4 | Chunk-bound deposits: Ore Excavation `OreDataAttachment`, Diesel `OilChunksSavedData`, Destroy `chunk_crude_oil` | **Plugin** (I11) | Read while loaded, deplete as a finite input, write back. Without the plugin: "not learnable: finite deposit unknown". |
| 5 | World-mutating machines | **Box** if cyclic, **Out** if progressive (I10) | Stationary machines on regrowing crops and trees are fine, and their edits never make a district stale. A drifting contraption: "not learnable: moving drill". |
| 6 | Mob farms, spawners, Cobblemon pastures | **Out** for spawn-fed farms (I9); **Box** for breeding and shearing | Spawns need a nearby player even under chunk loaders (§0): "mob drops not simulated". Cobblemon is leaving the pack. |
| 7 | Create Nuclear: fuel and cooler timers, heat, meltdown | **Box** when steady; meltdown **Out** | Fuel is a consumed input through the reactor input. A plugin reads core heat; a rising trend is not steady. "Reactor steady; meltdown not simulated." |
| 8 | Redstone links, CC: Tweaked, Drive-By-Wire | **Box**, as the observed average | Throttling on stock appears as a learned target. A controller driven from outside the district replays its observed pattern: "controlled from outside: replaying the observed pattern". |
| 9 | Mobile assets | trains **real**; ships **Out** (I17) | "Ship frozen at x, z". Backpack upgrades tick only when carried or placed. |
| 10 | Season, day and night, weather, temperature | **Box**, conditioned (I12) | The conditions seen while learning are stored. A season change is a breakpoint. |
| 11 | Explosions and fires | **Out** for accidents | No abstract industrial accidents. Attacks are §13. |

**Proposal I9 (Ben): replay what a chunk-loaded district with no player nearby would make.**
Recommended.
- **Crops grow,** because the pack's loaders use ticking tickets. **Spawn-fed farms do not.**
- **While learning,** `LivingDropsEvent` from non-player kills inside the district, and spawner blocks
  inside it, mark their drops, and the marked amounts are subtracted from the learned rates.

**Alternatives:** (b) Replay everything. A chunk-loaded mob farm would then make *less* than an unloaded
one; (c) Replay nothing from the world: no crops, no trees.

**Proposal I10 (Ben): cyclic contraptions are learnable; progressive ones freeze the district.**
Recommended. Contraption entities are tracked at each sample; one that returns to its start each cycle (bearings,
pistons, elevators) is cyclic.

**Alternatives:** (b) Replay anyway, and a tunnel bore mines forever; (c) Plugins for known quarry
designs later.

**Proposal I11 (Ben): deposit plugins.** Recommended.
- **Ore Excavation:** `OreData.getResourcesRemaining` is read while loaded and the vein becomes a finite
  consumed good, whose exhaustion is a breakpoint. `OreData.extract` writes the depletion back on load,
  with the epoch. Destroy `chunk_crude_oil` (a chunk attachment) is handled the same way.
- **Diesel:** `OilChunksSavedData.get/setChunkOilAmount` is level SavedData, writable even while
  unloaded. Infinite veins stay infinite.

**Alternatives:** (b) Treat deposits as infinite, which is an exploit; (c) Freeze every drilling
district.

**Proposal I12 (Ben): per-season models.** Recommended.
The model records the season (Serene Seasons, or mic-climate's `YearClock`), the day and night share,
and the weather while learning. A season change while unloaded freezes the district, unless a model
learned in that season exists.

**Alternatives:** (b) Ignore the season, which is wrong for seasonal crops; (c) Scale by a mod's own
data, such as Serene Seasons crop fertility.

---

## 7. Power

- **Create stress** is impact × |RPM| (`KineticNetwork.getStressMultiplierForSpeed`,
  `BlockStressValues.getImpact`), with no capability and no saved network total. Rotation stays inside
  a district by construction (case 1), so the box never measures it; the Create plugin reads the live
  network only to discard overstressed windows.
- **FE** (`Capabilities.EnergyStorage.BLOCK`) exists on a few blocks only: the PowerGrid FE inverter,
  Propulsion, Ore Excavation IO. FE storage blocks are stock.
- **PowerGrid batteries** hold `energy` and `capacity` (`BatteryBlockEntity`), drained by power × 0.05
  per tick (one 1/20 s tick). The plugin reads them as stock in integer units (TUNABLE). Generators and
  loads inside are part of the box; lines between districts are §8.6.
- **Fuel** is a consumed input: coal and blaze cakes into burners, diesel into engines, mic-petrochem
  fuels.

---

## 8. Supply lines

### 8.1 Trains are real

- **Create moves every train** (§0); the sim never does. The Create plugin observes arrivals and
  departures at district stations (`Train.arriveAt`, `leaveStation`) and each `Navigation.destination`.
- **Each leg is booked** into the region and time-window index (A15) from its graph path and speed, so
  threats meet trains as scheduled events (§13.3).

### 8.2 Packages

- **Mail keeps working** at unloaded stations (§0). A district's postbox offline buffers are its
  boundary stock: packages dropped there are imports, and packages the district sends are written in as
  real Create packages (`PackageItem.containing`, `addAddress`). Fetch-packages trains come for them.
- **Capacity:** a full 18-slot buffer is an export-blocked breakpoint, so more postboxes mean more
  throughput while unloaded.

### 8.3 Station cargo (I15)

**Proposal I15 (Ben): emulate the observed station exchange.** Recommended.
- **Learning:** while loaded, the plugin records per good what each stop unloaded and loaded. Usually
  that is a Portable Storage Interface design: "unload all ore, load all ingots".
- **While simulated,** for carriages with no entity in that dimension, at `Train.arriveAt` the
  observed kinds are unloaded into stock (up to capacity) and loaded from stock (up to the carriage's
  capacity). Both go through `TrainCargoManager`'s handlers, whose `changeDetected` resets the
  idle-cargo counter, so "cargo idle for N s" and threshold conditions behave as at a loaded station.

**Alternatives:** (b) Packages only (PLAN §10.4). Interface stations stop working while unloaded; (c) No
exchange.

### 8.4 Gauge networks that span districts (I16)

**The problem:** a home gauge whose network has an outpost link waits while the outpost is unloaded
(§0). Players will hit this first.

**Proposal I16 (Ben): warn first, then make simulated links count.** Recommended.
- **In the train phase,** the ledger warns: "network Iron spans Home and Iron Coast; Home's gauges wait
  while Iron Coast is unloaded".
- **In the Create precision phase,** mixins on `GlobalLogisticsManager.getUnloadedLinkCount` and
  `LogisticsManager.getSummaryOfNetwork` count a simulated district's links as present, with its stock
  coming from the sim.
- **Requests routed to that district** become addressed packages in its postbox offline buffer, which
  a fetch-packages train brings home. Three mixins, in the patch ledger.

**Alternatives:** (b) Warn only, forever; (c) Fulfil requests only.

### 8.5 Belts, pipes, chain conveyors, shafts

They stop in unloaded chunks (§0), and I2 merges every chunk they touch into one district. A long belt
between a mine and a mill is one district: while you stand at one end, the far end is frozen, as in
vanilla (I7). Splitting the line with a train or package hop keeps both ends alive. That is a real
consequence, in Create's own idiom.

### 8.6 Links that persist unloaded (I13)

**Proposal I13 (Ben): plugins for Ender Transmission and PowerGrid lines.** Recommended.
- **Ender Transmission:** channel buffers persist unloaded in `MatterWorldSavedData`, though transfer
  stops. A buffer is shared boundary stock for every district with a transmitter on that channel: one
  exports into it at its breakpoints, another imports. No chunk is needed.
- **PowerGrid:** `TransmissionLine`s span unloaded chunks, but their end nodes drop. The plugin records
  which district's generators feed which district's loads, and couples their running states: the sink
  stops at the source's stop breakpoint.

**Alternatives:** (b) Merge linked districts into giant ones, which suspend whenever any part loads; (c)
Not learnable.

### 8.7 Ships (I17)

**Proposal I17 (Ben): player ships stay frozen while unloaded, as today.** Recommended. The ledger
shows where each ship is: "Skylark frozen at x, z".

**Alternative:** (b) A Sable plugin books an unloaded voyage as a leg and moves the sub-level on arrival.
Its feasibility is unverified, and it belongs with the naval phase.

### 8.8 How flows represent player lines

- **SCALE.md §3 flows are traffic the sim owns.** Player trains are Create's, so they are observed,
  not flowed.
- **The district network** (SCALE.md §7) has **line edges** (station to station, from observed trips:
  cargo per trip per good, trips per day) and **link edges** (§8.6). Line edges feed the ledger's
  forecasts and the threat layer's hazard bookings. They never move goods; only real arrivals do.

---

## 9. Chunk loaders alongside

- **A chunk-loaded district is Live,** and gives the best data: long windows, no player interference.
- **When a power-gated loader loses power** (create_power_loader needs rotation; mic-grid-loader draws
  100 W per chunk loaded), the district switches to its learned model with no gap.
- **A partly loaded district is Suspended** (I7). The ledger names the chunks the loader misses.
- **Under I9, a farm makes the same whether it is chunk-loaded or simulated.** Chunk-loading never
  makes a factory worse. It is worth its cost only where real-time behaviour matters: real defence
  fights (§13.1), or ships.

---

## 10. What the player sees (I18)

**Proposal I18 (Ben): Create's own displays, the atlas, and a chat ledger.** Recommended.
- **A Display Source, "District ledger",** registered in Create's `display_source` registry and bound
  through `DisplaySource.BY_BLOCK.add` to `create:stock_link` and `create:track_station`. A Display Link
  on either block shows any district (chosen in the link's options) on a Display Board or Nixie tubes:
  its state (§2.3), top goods per hour, next breakpoint ("iron ore runs out in H h M min"), last attack
  and defence rating.
- **Antique Atlas markers** (Surveyor plugin): districts by state colour, attack sites, frozen ships.
- **`/rtsim district ledger <name>`:** the model, the last reconcile (deltas and any trim), attack
  reports, and what froze it. Later, a CC: Tweaked peripheral.

**Alternatives:** (b) Chat only; (c) A custom screen or item. D3 allows that only if necessary, and it is
not.

---

## 11. Abuse cases

| Abuse | What stops it |
|---|---|
| Overclock during learning (hand-feeding, a creative motor) | Player transfers are ports. Creative blocks discard the window. Rates are long-window steady slopes. Hand-fed input is consumed stock, so the district starves when it ends. |
| Dump items into a vault so they look like production | Container session diffs are the player's. Items tossed toward a hopper are caught by `ItemTossEvent`. |
| Learn a district, then gut it and keep the model | Player breaks make it stale, and stale freezes. |
| Empty a vault right after load | Reconciliation runs on the first tick after load, long before a player who loads chunks at view distance can reach the vault. A machine that takes first causes a shortfall trim. |
| Reload a chunk to double a reconcile | Epochs make each delta exactly-once (I8). |
| Infinite deposits; mob farms with nobody near | I11; I9. |
| Log off to dodge attacks | C17. In single-player, nothing happens while the world is closed, to the factory or to its attackers (I4). |
| Defences that only work while watched | Abstract outcomes are capped by observed ones (§13.6). Ammunition is a consumed good. |
| Duplicate through a rebuild schematic | Depends on I1. A sim repair schematic stores block states only. The Schematicannon's own NBT filter (catnip `NBTProcessors.process`) is UNVERIFIED here. |

---

## 12. Layering: black box and plugins

**Recommended layering.** C19 fixes layers 1 and 2; the rest is covered by the I proposals.
1. **Core, mod-agnostic:** districts, capability sampling, ports through vanilla and NeoForge events,
   the box, reconciliation, the ledger. No pack mod (D4).
2. **Create plugin** (`mic_rtsim_create`): station hooks, cargo emulation, postbox buffers, Display
   Sources, belt and kinetic merging, gauge processes, the I16 mixins.
3. **One plugin per mod:** PowerGrid (batteries, lines), Destroy (pollution, crude oil), Ore Excavation
   and Diesel Generators (deposits), Ender Transmission (channels), Create Nuclear (heat), Create Radars
   and Create Big Cannons (the defence survey).
4. **Optional precision:** recipe mixins (`BasinOperatingBlockEntity.applyBasinRecipe`,
   `PressingBehaviour.applyOnBasin` and `applyInWorld`, `FanProcessing.applyProcessing`) to split a box
   into processes, added only if measurements show the box is too coarse. No mod has a recipe-finished
   event.

**Why not per-mod models first:** gauges cover only Create 6 logistics designs, and most addons have no
hook at all. The box covers every mod on day one.

**Why not the box alone:** spanning gauge networks, deposits, pollution and power lines are invisible
or wrong without their plugins.

**Proposal I14 (Ben): Destroy pollution while unloaded.** Recommended.
- **Attribution:** the plugin wraps `PollutionHelper.changePollution(Level, BlockPos, ...)`, the path
  every emission takes through `pollute`, and attributes each change to the district at that position.
  That gives integer rates per pollution type.
- **While unloaded,** level types (greenhouse, ozone depletion, acid rain) are applied at each
  breakpoint through `changePollution(Level, type, n)`. Chunk types (smog, radioactivity) are queued and
  applied on load through `changePollution(ChunkAccess, ...)`, with the epoch. Conflict reads the same
  rates for grievance (P14).

**Alternatives:** (b) Feed grievance only; (c) Level types only.

---

## 13. Threats (C16 to C18)

### 13.1 Watched, unwatched, offline

| Situation | Fight | Changes |
|---|---|---|
| A player is near, including AFK at the base | real: puppets spawn at the edge of the entity-ticking area (PLAN §10.3) | real, at once |
| Chunk-loaded, no player near | abstract (C18: unwatched) | applied at once, under the block quota |
| Unloaded; owner online elsewhere, or logged off on a dedicated server (C17) | abstract | applied on the next load (C18) |
| Server stopped | nothing (I4) | |

### 13.2 Sources (I19)

The core is source-agnostic. A threat is a sim agent or flow (A13) with a class (ground melee, ground
ranged, flying, siege), a strength, booked legs (A15) and a target rule. Plugins supply the bodies and
the reasons.

**Proposal I19 (Ben): the order the sources arrive.** Recommended.
1. **Natural mobs:** core, no new bodies (§13.4).
2. **Dragons,** with the Ice and Fire plugin (phase 2). Hunger and grudges add districts to the target
   list. The pack sets dragon `griefing` false and `stealItems` false, so dragon damage is the sim's,
   bounded, and dragons destroy rather than steal. The dragons module takes over the In Control spawner
   that already spawns overworld fire dragons.
3. **Bandits,** with the conflict module. Bodies come from P8 and must look like mobs (D7). Stolen stock
   flows to the band's camp market, where players can buy it back or raid it (mining note §2.1).
4. **Bosses.** Structure bosses (L_Ender's Cataclysm, Bosses of Mass Destruction, Mowzie's Mobs) stay
   home; whether any roams is UNVERIFIED. Raiding bosses arrive with kaiju (C10, P27).

**Alternatives:** (b) Dragons first; (c) Bandits first.

### 13.3 Where threats meet infrastructure

Districts and booked train legs are indexed by region and time window (A15). A threat leg looks up
what is booked along its path and schedules encounter events; a threat targeting a district books a leg
to it. Cost: one lookup per leg start, never a scan.

### 13.4 Natural mob pressure on unattended districts (I20)

In vanilla, an unattended base sees no natural mobs at all: spawns need a player within 128 blocks.
So this pressure is added by the sim, and it is grounded in measurement and in vanilla's damage rules.

**Proposal I20 (Ben): measured pressure.** Recommended.
- **The rate:** while a player is in a district at night, the bridge counts natural hostile spawns
  (`FinalizeSpawnEvent.getSpawnType()`) per dark spawnable block, in and around the district (ring width
  TUNABLE). Unwatched, that rate applies to the district's measured dark spawnable area, with one
  counter-RNG draw per (district, night slot) (SCALE.md §4).
- **Effects follow vanilla:** creepers break blocks, and endermen take them, only if the `mobGriefing`
  gamerule is on; most mobs break nothing. Damage is bounded by I23. A fully lit, walled district has
  zero pressure.

**Alternatives:** (b) None, which is vanilla-consistent: natural mobs then threaten only watched bases;
(c) A TUNABLE hazard per biome.

### 13.5 Defences, measured while loaded (I21)

**Proposal I21 (Ben): a defence vector per threat class.** Recommended. It is recomputed on player edits.

| Measure | How | Counts against |
|---|---|---|
| Perimeter | A flood fill over the district's columns from outside. A column blocks if its barrier is at least 2 blocks above the walk surface, or is a fence or wall. These are vanilla collision rules, to verify in the phase. | ground |
| Dark spawnable area | Spawnable top surfaces with block light 0 (the overworld's `monster_spawn_block_light_limit` is 0) | natural mobs |
| Roof cover | The share of machine block entities without sky access (`canSeeSky`) | flying |
| Auto turrets | Create Radars networks whose `TargetingConfig` has `mob`, `autoTarget` and `autoFire` set, with Create Big Cannons or CBC Neo Warfare weapons. Their ammunition is stock and a consumed good. | all classes in range |
| Kill machines at gaps | Create saws, drills, crushing wheels and armed deployers at perimeter gaps (`SawBlock`, `DrillBlock` and `CrushingWheelControllerBlockEntity` hurt entities) | ground melee |
| Guards | Owned or tamed entities: iron golems, wolves, tamed dragons | ground, flying |
| Observed fights | Each real fight in the district: attacker class and strength, losses on both sides | calibration |

**Alternatives:** (b) Observed outcomes only, so untested bases are helpless; (c) Player-declared
defences, which are trivially gamed.

### 13.6 Resolution (I22)

**Proposal I22 (Ben): hazards for pressure, Lanchester for attacks, capped by observation.**
Recommended.
- **Pressure:** hazard draws (§13.4). **Raids, dragons and bosses:** Lanchester per attack, the square
  law for aimed ranged fire and the linear law for melee and unaimed fire. Strengths come from the
  defence vector after the gates: walls stop ground melee, and roofs shield machines from flyers.
- **Never better than observed (the X4 lesson):** the defender's abstract losses are never lower than
  the best observed real outcome against that class at comparable strength.
- **Untested defences** count at a reduced weight (TUNABLE) until a real fight is seen. GameTests stage
  real fights against reference bases to calibrate (PLAN §15).

**Alternatives:** (b) Lookup from observed fights only; (c) Hazards for everything.

### 13.7 Damage and its bounds (I23)

**Proposal I23 (Ben): theft, chosen blocks, and a stop.** Recommended.
- **Stolen stock** is taken from boundary goods, up to the attackers' carry capacity (TUNABLE per class)
  and a per-attack cap (a TUNABLE share).
- **Destroyed blocks** are chosen from the surveyed exposed set (perimeter first for ground attackers,
  roofless first for flyers), capped per attack (TUNABLE), and stored as (position, expected state).
- **Any destroyed machine** stops the abstract run: a Create line missing one machine jams, so the
  district stays Damaged until it is reloaded and relearned.
- **On load, under the block quota,** theft applies as negative deltas with the epoch (I8), and each
  block whose state still matches is broken with `level.destroyBlock(pos, true)`, so its contents drop
  as recoverable items. Blocks the player has changed since are skipped.
- **Fire is never placed.** It would spread once loaded and break the bound. Burning is recorded as
  destroyed blocks.

**Alternatives:** (b) Proportional capacity loss instead of a stop; (c) Theft only; (d) Destroyed blocks
drop nothing.

### 13.8 Trains and packages under attack (I24)

**Proposal I24 (Ben): loot cargo, never cut track while unwatched.** Recommended.
- **A train leg met by a threat** loses cargo from `Carriage.storage`, taken while that carriage has no
  entity (the path mail uses), bounded like I23. Postbox offline buffers at an attacked district are
  stock, so they can be stolen from too.
- **No track or signal is broken unwatched,** because Create's graph would disagree with the blocks
  until load.

**Alternatives:** (b) Derail the train (`Train.derailed`). Recovery is UNVERIFIED; (c) Delay only.

### 13.9 Pacing (I25)

**Proposal I25 (Ben): caps and a cooldown, no shields.** Recommended. Each attack is capped (I23), and
after an attack a district has a cooldown (TUNABLE) per threat class. One long absence cannot strip a
base, but an undefended base keeps losing.

**Alternatives:** (b) A shield after heavy losses; (c) Attack frequency tied to the owner's online time,
which works against C17.

### 13.10 Rebuilding (I1, open: Ben chose "decide later")

**Facts:**
- `SchematicExport.saveSchematic(dir, name, overwrite, level, first, second)` writes a template from the
  world; `SchematicItem.create(level, file, owner)` makes a schematic item; the server reads
  `schematics/uploaded/<owner>/<file>`.
- An upload deletes the uploader's oldest file past `maxSchematics` (10) in their own folder, so sim
  files belong in a folder of their own.
- The Schematicannon places one block per 10 ticks and 400 per gunpowder: at most 2,400 blocks per
  cannon per Minecraft day, using 6 gunpowder.

**Options for when Ben decides:**
- (a) **Repair schematics.** Before damage is applied on load, the bridge records the states of the
  blocks it will break, writes a schematic of only those positions (block states, no block-entity NBT),
  and sends it to the district's postbox as a package. Printed with "replace: empty only", it fills
  exactly the holes. Its size grows with damage, not with the district.
- (b) **Full district snapshots** each time a district is learned. These are large files.
- (c) **None.** Players use the Schematic and Quill, and the ledger lists the destroyed positions.

---

## 14. Walkthroughs

### 14.1 The iron outpost keeps working

1. **You build the outpost** on the far coast: crushers, washers and blaze-burner smelting. Ore arrives
   by train through Portable Storage Interfaces, ingots fill two vaults, and a threshold switch stops
   the smelters at a full vault. The station is *Iron Coast*.
2. **It learns.** A Display Link on your home station shows "Iron Coast: learning, N min more". Your
   train calls every T minutes, so the window spans several calls.
3. **It is learned:** "+N ingots/h, -M crushed ore/h, target: vault full".
4. **You ride home and the outpost unloads.** Create keeps running your train. At Iron Coast the sim
   unloads the ore and loads ingots, as the interfaces did (I15), and the schedule's "cargo idle"
   condition fires as it would at a loaded station.
5. **The ore runs out.** The board reads "Stopped: no crushed ore". The next train's ore restarts it.
6. **You return a week later.** On the first tick the vaults change by the ledger's deltas. The belts
   held the same half-crushed ore all along, and the machines carry on.

### 14.2 The dragon and the roofless smelters

1. **The attack.** You are logged off; the dedicated server runs. A hungry fire dragon's hunt leg
   crosses Iron Coast's region, so an encounter is scheduled. Iron Coast has no roof and no turrets.
2. **The resolution.** You have no real dragon fight there, so the untested weight applies. Roofless
   blocks are destroyed up to the cap, including a blaze burner, and the district stops: Damaged.
3. **You log in.** The board says "Iron Coast: fire dragon, N blocks destroyed, stopped".
4. **You ride out.** The blocks break as the chunks load, and their contents drop. If I1 picks (a), a
   repair schematic waits in the postbox, and your Schematicannon, fed from a vault, prints the holes
   back.
5. **You fortify.** You roof the smelters, and add a Create Radars fire controller set to auto-target
   mobs, on a Create Big Cannons autocannon with an ammo vault.
6. **The next dragon** comes while you are there, and the turret fights for real. That fight becomes
   the observation that caps the abstract fights after it.

### 14.3 The gauge network that froze

1. Your home gauges order crushed ore from network *Iron*, whose stock links are at Iron Coast.
2. You leave, and Create freezes your home gauges (`waitingForNetwork`). The ledger says why.
3. Before the Create precision phase, you split the network or ship by train.
4. After it (I16), your gauges stay live: their orders become packages in Iron Coast's postbox buffer,
   and a fetch-packages train brings them home.

---

## 15. Cost

This follows SCALE.md §0.

| Work | Thread | Grows with | Never with |
|---|---|---|---|
| Sampling | server, quota | boundary inventories of loaded districts ÷ Δ | items moved |
| Learning | engine | samples | |
| Unloaded replay | engine | breakpoints + arrivals + attacks | elapsed time, district count |
| Station exchange | server | arrivals at simulated stations | |
| Reconcile, damage | server, quota | deltas and destroyed blocks, once per load | elapsed time |
| Threats | engine | booked legs, slot draws, encounters | district count |

Quiet districts are free per tick, however many there are (SCALE.md §7). A district keeps its model,
boundary list, pending deltas and defence vector, all paged with its region (A16).

---

## 16. Phases (I26)

**Proposal I26 (Ben): this order.** Recommended.

| Phase | Builds | Ben sees |
|---|---|---|
| **I-A Ledger, read only** | detection, sampling, ports, learning (I2, I3, I5), Display Source, `/rtsim district` | A Create line's rates appear on a Display Board. Walk away and come back, and the ledger says what the sim *would* have made, with the vaults untouched. |
| **I-B Replay** | the box (I6), breakpoints, reconcile (I7, I8), I4, I9, I10, items only | A GameTest runs two identical factories, one chunk-loaded, one simulated. After N ticks, the simulated vaults are at or below the control and within a TUNABLE tolerance. Conservation and composition property tests pass. |
| **I-C Fluids and energy** | fluids, FE, PowerGrid (I13), Destroy pollution (I14), deposits (I11), seasons (I12) | a refinery and a power plant that run while you are away |
| **I-D Trains** | station hooks, cargo emulation (I15), postbox buffers, Ender Transmission, I16 warnings, line edges | walkthrough 14.1 |
| **I-E Create precision** | gauge processes, I16 mixins, overstress checks | walkthrough 14.3 |
| **I-F Threats 1** | survey (I21), pressure (I20), resolution (I22), damage (I23), trains (I24), pacing (I25) | an unlit outpost loses blocks while you are away; a lit, walled one does not |
| **I-G Threats 2** | dragons (with phase 2), bandits (with conflict), bosses (with kaiju) | walkthrough 14.2 |
| **I-H Rebuild** | per I1 | the repair print |

**Alternatives:** (b) Threats first, on loaded bases; (c) Trains before replay.

---

## 17. Reuse table

| Need | Mod (version) | Hook | Status |
|---|---|---|---|
| Inventories | NeoForge 21.1 | `Capabilities.ItemHandler.BLOCK`, `FluidHandler.BLOCK`, `EnergyStorage.BLOCK` | verified |
| Player transfers, edits, drops, spawns | NeoForge 21.1 | `PlayerContainerEvent.Open/Close`, `ItemTossEvent`, `ItemEntityPickupEvent.Post`, `BlockEvent.BreakEvent/EntityPlaceEvent`, `LivingDropsEvent`, `CropGrowEvent.Post`, `FinalizeSpawnEvent` | verified |
| Load and epochs | NeoForge 21.1 | `ChunkEvent.Load/Unload`; chunk attachments in chunk NBT | verified |
| Trains | Create 6.0.10 | `GlobalRailwayManager.trains`, `Train.arriveAt/leaveStation`, `Navigation.destination`, `GlobalStation.name/connectedPorts/getPresentTrain` | verified |
| Train cargo | Create 6.0.10 | `Carriage.storage` (`TrainCargoManager`; handlers call `changeDetected`) | verified |
| Packages | Create 6.0.10 | `GlobalPackagePort.offlineBuffer`, `PackageItem.containing/addAddress/getContents`, `FetchPackagesInstruction` | verified |
| Network stock | Create 6.0.10 | `LogisticsManager.getSummaryOfNetwork`, `LogisticsNetwork.totalLinks/loadedLinks`, `GlobalLogisticsManager.getUnloadedLinkCount` | verified |
| Gauges | Create 6.0.10 | `FactoryPanelBehaviour.targetedBy/recipeOutput/recipeAddress/network/restockerPromises`; `factoryGaugeTimer` | verified; the count and `upTo` fields sit in its filtering superclass (UNVERIFIED) |
| Stress | Create 6.0.10 | `BlockStressValues.getImpact/getCapacity` | verified |
| Displays | Create 6.0.10 | `DisplaySource.BY_BLOCK`, `CreateRegistries.DISPLAY_SOURCE`, `AllBlocks.STOCK_LINK` | verified |
| Rebuild | Create 6.0.10 | `SchematicExport.saveSchematic`, `SchematicItem.create`, `CreatePaths.UPLOADED_SCHEMATICS_DIR` | verified |
| Kill machines | Create 6.0.10 | `SawBlock`, `DrillBlock`, `CrushingWheelControllerBlockEntity` | verified |
| Turrets | Create Radars 0.4.9.4 | `TargetingConfig`, `FireControllerBlockEntity`, `AutoYaw/AutoPitchControllerBlockEntity` | verified (javap) |
| Batteries | PowerGrid 0.6.2 | `BatteryBlockEntity.energy/capacity` | verified (source) |
| Pollution | Destroy 0.4.1 | `PollutionHelper.pollute`, `changePollution` overloads | verified (javap) |
| Deposits | Ore Excavation 1.6.8, Diesel Generators 1.3.15, Destroy | `OreData.extract/getResourcesRemaining`, `OilChunksSavedData.get/setChunkOilAmount`, `chunk_crude_oil` | inventory |
| Channels | Ender Transmission 2.1.1 | `MatterWorldSavedData` | inventory |
| Dragon rules | Ice and Fire CE 2.1-beta.1 | `iaf-common.json` dragon `griefing`, `stealItems` | verified |
| Loaders | create_power_loader 2.0.5, mic-grid-loader 1.0.0 | `ChunkLoadManager`, `forceChunk(..., true)` | verified |
| Map | Surveyor 1.2.4 | `WorldLandmarks.of(level).put` | PLAN §1.3 |

---

## 18. Custom-roll ledger

No new blocks, items or entities (D3).

| Piece | Where | Size | Notes |
|---|---|---|---|
| District detector and index | Java + Rust | M | placements, grouping, merges, commands |
| Sampler and port hooks | Java | M | capability reads under quota; sessions; tosses |
| Rate learner | Rust | M | slopes, windows, steady test, flags, season models |
| Box replay | Rust | M | rationals, breakpoints, error diffusion, property tests |
| Reconciler | Java + Rust | M | per-inventory deltas, epoch attachment, shortfall trim |
| Ledger and Display Source | Java | S | one source, two bindings, chat ledger |
| Create plugin | Java | L | stations, cargo emulation, postbox writes, merges, gauges, three I16 mixins |
| PowerGrid, Destroy, deposit and Ender Transmission plugins | Java | S each | batteries and lines; one `changePollution` wrap; Ore Excavation and Diesel; channels |
| Defence surveyor | Java | M | flood fill, light, roof, radar networks, guards |
| Threat resolution | Rust | M | hazards, Lanchester, observation caps, cooldowns |
| Damage applier | Java | S | theft deltas, `destroyBlock` under quota |
| Repair schematics (if I1 picks a) | Java | S | template from recorded states |
| GameTests | Java | M | control vs simulated; crash points; trains in other dimensions; staged fights |

**Mixins (P7 ledger):** in Create, the optional vault change flag, `getUnloadedLinkCount`,
`getSummaryOfNetwork` and request fulfilment; in Destroy, `changePollution`. Everything else is events,
capabilities or public fields.

---

## 19. Risks and unknowns

1. **Steady-state windows for real Create factories are unmeasured:** bursty gauges, trains and batch
   machines. Phase I-A measures them before replay exists.
2. **The cost of sampling big vaults on the potato.** A vault block holds 20 stacks, so a large vault is
   many slots. The change flag may be required.
3. **Deltas against the first machine tick after load:** the trim keeps conservation, but a GameTest
   must show how often it fires.
4. **Trains in other dimensions:** the source says they tick; PLAN and the inventory say they do not.
   A Nether GameTest settles it.
5. **Cargo emulation:** `carriage.storage.getAllItems()` can be null (Create's mail skips it). The
   absence of a carriage entity must be checked per dimension.
6. **Attribution gaps:** items entering by other routes (a backpack upgrade feeding a hopper, say) look
   like production.
7. **Combat calibration:** most districts will have no observed fights, so the untested weight decides
   early balance.
8. **Damage to Create multiblocks** (belts, vaults, tanks) and to contraption anchors may cascade on
   load. It needs a GameTest per kind.
9. **Create internals can move** (`FactoryPanelBehaviour`, `GlobalPackagePort`, `TrainCargoManager`).
   Contract tests (P7) catch it.
10. **Modded spawns** (In Control, L2 Hostility) may bypass `Monster.isDarkEnoughToSpawn`. That is why
    I20 is empirical.

---

## 20. Proposals for Ben

| # | Question | Recommended | Alternatives |
|---|---|---|---|
| I1 | Rebuild schematics (Ben: decide later) | (a) repair schematics of destroyed blocks only | (b) full snapshots; (c) none, ledger lists positions |
| I2 | What a district is | automatic, closed under belts, pipes, shafts and wires; named after its station; command edits; retire P5 | (b) player-designated; (c) one logistics network (P5); (d) loader footprint |
| I3 | Boundary stock | tagged storage + packager inventories + postboxes + plugin buffers; machines never | (b) all inventories counted, tagged written; (c) stock links only |
| I4 | When time passes | game ticks only; nothing while the server is stopped | (b) wall-clock catch-up with a cap |
| I5 | Learning | windowed steady test; unlearned, stale and not-learnable freeze | (b) stale runs old model; (c) reduced confidence |
| I6 | Box coupling | one all-or-nothing process; any empty input, full output or target stops it | (b) one process per output; (c) learned coupling |
| I7 | Partial loads | suspend while any chunk ticks; pending deltas per chunk | (b) simulate the frozen remainder; (c) force-load it |
| I8 | Exactly-once reconcile | epoch in a chunk attachment; removals first; shortfall trims | (b) global journal only; (c) NBT rewrite at load |
| I9 | Proximity production | as chunk-loaded with no player near: crops yes, spawn-fed farms no | (b) everything; (c) nothing from the world |
| I10 | Moving contraptions | cyclic learnable; progressive freezes | (b) replay anyway; (c) design plugins later |
| I11 | Finite deposits | Ore Excavation, Diesel and Destroy crude plugins | (b) infinite; (c) freeze drilling districts |
| I12 | Seasons | per-season models; season change is a breakpoint | (b) ignore; (c) mod-data multipliers |
| I13 | Links across districts | Ender Transmission channels as shared stock; PowerGrid lines couple running states | (b) merge; (c) not learnable |
| I14 | Destroy pollution | attribute `changePollution` by position; level types live, chunk types on load | (b) grievance only; (c) level types only |
| I15 | Station cargo | emulate the observed exchange through `TrainCargoManager` | (b) packages only; (c) none |
| I16 | Gauge networks across districts | warn first, then mixins count simulated links and fulfil into offline buffers | (b) warn only; (c) fulfilment only |
| I17 | Player ships | frozen, shown on the ledger | (b) Sable voyage plugin later |
| I18 | Visibility | Display Source on stock links and stations, atlas, chat ledger | (b) chat only; (c) custom screen |
| I19 | Threat order | natural mobs, dragons, bandits, then bosses with kaiju | (b) dragons first; (c) bandits first |
| I20 | Natural mob pressure | measured spawns per dark block; vanilla griefing rules | (b) none; (c) TUNABLE per biome |
| I21 | Defence survey | perimeter, dark area, roof, radar turrets with ammo, kill machines, guards, observed fights | (b) observed only; (c) declared |
| I22 | Resolution | hazards for pressure; Lanchester for attacks; never better than observed | (b) observed lookup; (c) hazards for all |
| I23 | Damage | theft to carry and cap; exposed blocks to cap, with drops; machine loss stops it; no fire | (b) proportional loss; (c) theft only; (d) no drops |
| I24 | Trains attacked | loot carriage cargo; never cut track unwatched | (b) derail; (c) delay only |
| I25 | Pacing | per-attack caps, per-class cooldown, no shields | (b) shields; (c) tie to online time |
| I26 | Phases | I-A read-only ledger, then replay, fluids and energy, trains, Create precision, threats, rebuild | (b) threats first; (c) trains before replay |

---

## Appendix A. Verified facts (2026-10-20)

**Sources:**
- Create `mc1.21.1/dev` at `7ff2760e`; "jar" marks a javap check against `create-1.21.1-6.0.10.jar`.
- Vanilla 1.21.1 (`server-1.21.1-20240808.144430-srg.jar`) and NeoForge 21.1.249 (patched server and
  universal jars), all from `mods/deep-time/smoke/mic/server/`.

**Create, trains:**
- `GlobalRailwayManager.tick(Level)` returns unless the level is the overworld. It is called from
  `CommonEvents.onServerWorldTick` for every level, then ticks `waitingTrains` and `movingTrains` (jar).
- `Train.tick` calls `runMailTransfer` once `ticksSinceLastMailTransfer > 20` (jar).
- `Carriage.manageEntities` resolves each dimension through `server.getLevel`.
- A stall needs a player passenger within 32 blocks and a 16-block lookahead that is not entity-ticking.
- Public members: `Train.arriveAt`, `leaveStation`, `derailed`, `speed`, `navigation`
  (`destination`, `distanceToDestination`), `carriages`; `GlobalStation.name`, `connectedPorts`.

**Create, stations and packages:**
- `runMailTransfer` uses the postbox when `level.isLoaded(pos)`, else `port.offlineBuffer` (jar).
- `offlineBuffer` is `new ItemStackHandler(18)` (jar). Packages `stacksTo(1)`. `PackageItem.SLOTS` is 9.
- `FetchPackagesInstruction.start` reads offline buffers.
- `StationUnloadedCondition` uses `isPositionEntityTicking`, and `IdleCargoCondition` uses
  `getTicksSinceLastExchange`. `TrainCargoManager` handlers call `changeDetected`.

**Create, logistics:**
- `isValidLink` requires `!isRemoved()` and `!isChunkUnloaded()` (jar).
- `LogisticsNetwork` saves `totalLinks`, `owner`, `locked` and `panelPromises`.
- `getUnloadedLinkCount` is `totalLinks.size() - loadedLinks.size()` (jar).
- `tickStorageMonitor` sets `waitingForNetwork`. `addConnection` caps `targetedBy` at 9. The §17 fields
  exist (jar).
- `PackagerBlockEntity.targetInventory` uses `Capabilities.ItemHandler.BLOCK`.

**Create, config defaults (jar):**
- `factoryGaugeTimer` 100 (minimum 5), `vaultCapacity` 20, `psiTimeout` 60, `chainConveyorCapacity` 20,
  `linkRange` 256, `fluidTankCapacity` 8 buckets.
- `maxSchematics` 10, `maxTotalSchematicSize` 256 KB, `schematicannonDelay` 10,
  `schematicannonShotsPerGunpowder` 400.

**Create, machines and kinetics:**
- `getStressMultiplierForSpeed` is `|speed|` (jar).
- `KineticBlockEntity` saves `Speed` and `Network {Stress, Capacity, Size}`.
- `MillstoneBlockEntity.getProcessingSpeed` is `clamp(|RPM| / 16, 1, 512)` (jar).
- `.hurt` is called in `SawBlock`, `DrillBlock` and `CrushingWheelControllerBlockEntity`.

**Create, displays and schematics:**
- `DisplaySource.BY_BLOCK` and `BY_BLOCK_ENTITY` are `SimpleRegistry.Multi` with `add` (jar).
- `SchematicExport.saveSchematic` (jar) uses `fillFromWorld(..., true, AIR)`. `SchematicItem.create`
  (jar) loads from `UPLOADED_SCHEMATICS_DIR/<owner>/<file>`.
- `tryDeleteOldestSchematic` limits only the uploader's folder.
- `SchematicProcessor` uses catnip's `NBTProcessors.process`, which was not read.

**Vanilla 1.21.1:**
- `LevelChunk.isTicking` needs `BLOCK_TICKING`.
- Spawn distances: `playerIsCloseEnoughForSpawning` uses 16384.0 (128²), and `NaturalSpawner` uses 576.0
  (24²).
- `LocalMobCapCalculator.canSpawn` is false with no player near.
- The overworld's `monster_spawn_block_light_limit` is 0, and its `monster_spawn_light_level` is uniform
  0 to 7.
- `BaseSpawner` has `requiredPlayerRange` 16 and a delay of 200 to 800.
- Sleep sets `dayTime`, and `tickTime` adds 1 to `gameTime`.

**NeoForge 21.1.249:**
- The patched `tickChunks` calls `tickChunk` when `anyPlayerCloseEnoughForSpawning` or
  `shouldForceTicks`.
- `TicketController.forceChunk(..., add, ticking)`.
- `ChunkSerializer` writes `neoforge:attachments` through `ChunkAccess.writeAttachmentsToNBT`.
- `FinalizeSpawnEvent.getSpawnType()` and the §17 events exist.

**Pack:**
- create_power_loader `ChunkLoadManager` calls `forceChunk(..., add, true)` (source).
- `iaf-common.json` has dragon `griefing` false and `stealItems` false, and cyclops and sea serpent
  `griefing` true.
- Create Radars `TargetingConfig` has `player`, `contraption`, `mob`, `animal`, `projectile`,
  `autoTarget`, `autoFire` and `lineOfSight` (javap).
- Destroy 0.4.1 has `pollute` and four `changePollution` overloads (javap).
- PowerGrid `BatteryBlockEntity` does `energy -= power * 0.05f` per tick (source,
  `architectury-1.21.1/dev`).
